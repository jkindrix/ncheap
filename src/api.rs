pub mod xml;

use std::cell::Cell;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::Profile;

/// Minimum spacing between API calls within this process. Namecheap's FAQ
/// documents 50/min key-wide (700/hour, 8000/day); older third-party
/// reports say 20/min; 3100ms spaces for the conservative figure.
/// Concurrent processes do not coordinate (a cross-process budget is
/// planned, not built).
const MIN_SPACING: Duration = Duration::from_millis(3100);
/// Backoff before the single retry on HTTP 429/5xx. The API documents no
/// rate-limit error shape, so this is conservative, not tuned.
const RETRY_BACKOFF: Duration = Duration::from_secs(5);

/// Every read-only API method this tool may issue, lowercase canonical
/// (no "namecheap." prefix). Client::call refuses anything not listed
/// (fail-closed); mutating methods must go through Client::call_mut, which
/// carries the production gate and never auto-retries.
pub const READ_ONLY_COMMANDS: &[&str] = &[
    "domains.getlist",
    "domains.check",
    "domains.getregistrarlock",
    "domains.getinfo",
    "domains.getcontacts",
    "domains.gettldlist",
    "domains.dns.getlist",
    "domains.dns.gethosts",
    "whoisguard.getlist",
    "users.getbalances",
    "users.getpricing",
];

/// Lowercase canonical command name: "namecheap." prefix stripped.
pub fn canonical_command(command: &str) -> String {
    let stripped = command
        .strip_prefix("namecheap.")
        .or_else(|| command.strip_prefix("Namecheap."))
        .unwrap_or(command);
    stripped.to_ascii_lowercase()
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Config(#[from] crate::config::ConfigError),
    #[error("transport: {0}")]
    Transport(String),
    #[error("rate limited by Namecheap ({0}); retry later")]
    RateLimited(String),
    #[error("Namecheap API error {code}: {message}")]
    Api { code: String, message: String },
    #[error("unexpected API response: {0}")]
    Parse(String),
    #[error("{0}")]
    Usage(String),
    /// Refused by the safety policy (mutation gate); maps to the config
    /// kind/exit-code so the external contract stays unchanged.
    #[error("{0}")]
    Policy(String),
}

impl Error {
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::Api { .. } | Error::Parse(_) => 1,
            Error::Usage(_) => 2,
            Error::Config(_) | Error::Policy(_) => 3,
            Error::Transport(_) => 4,
            Error::RateLimited(_) => 5,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Error::Api { .. } => "api",
            // Split from "api" in envelope schema 2: a malformed response
            // is our problem or upstream drift, not a registrar verdict.
            Error::Parse(_) => "parse",
            Error::Usage(_) => "usage",
            Error::Config(_) | Error::Policy(_) => "config",
            Error::Transport(_) => "transport",
            Error::RateLimited(_) => "rate_limit",
        }
    }

    pub fn code(&self) -> Option<&str> {
        match self {
            Error::Api { code, .. } => Some(code),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum TransportFailure {
    Status(u16),
    Other(String),
}

pub trait Transport {
    fn send(&self, endpoint: &str, params: &[(String, String)])
    -> Result<String, TransportFailure>;
}

pub struct HttpTransport {
    agent: ureq::Agent,
}

impl HttpTransport {
    pub fn new() -> Self {
        // https_only + no redirects: a credential-bearing request must never
        // be re-routed or downgraded by a server-side redirect. Debug builds
        // relax https_only so the NCHEAP_ENDPOINT test override can point at
        // a localhost mock; release builds always enforce it.
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .https_only(cfg!(not(debug_assertions)))
            .max_redirects(0)
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl Default for HttpTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for HttpTransport {
    fn send(
        &self,
        endpoint: &str,
        params: &[(String, String)],
    ) -> Result<String, TransportFailure> {
        // POST with a form body keeps the ApiKey out of URLs (query strings
        // are exposed to proxies and intermediary logging; bodies are not).
        let form: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        match self.agent.post(endpoint).send_form(form) {
            // With max_redirects(0), ureq hands a 3xx back as Ok; that is a
            // transport anomaly here, not an API response to parse.
            Ok(resp) if resp.status().is_redirection() => {
                Err(TransportFailure::Status(resp.status().as_u16()))
            }
            Ok(mut resp) => resp
                .body_mut()
                .read_to_string()
                .map_err(|e| TransportFailure::Other(e.to_string())),
            Err(ureq::Error::StatusCode(code)) => Err(TransportFailure::Status(code)),
            Err(e) => Err(TransportFailure::Other(e.to_string())),
        }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Replace the key (raw and percent-encoded) in an error string. Keys
/// shorter than 8 chars are not replaced: real keys are 32-char hex, and
/// substring-replacing a tiny test key only mangles unrelated words.
fn redact(msg: &str, key: &str) -> String {
    if key.len() < 8 {
        return msg.to_owned();
    }
    let encoded: String = key
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let out = msg.replace(key, "<redacted>");
    if encoded != key {
        return out.replace(&encoded, "<redacted>");
    }
    out
}

pub struct Client<T: Transport> {
    transport: T,
    profile: Profile,
    last_call: Cell<Option<Instant>>,
    calls: Cell<u32>,
    spacing: Duration,
    retry_backoff: Duration,
    /// Mutation journal directory. When set, every call_mut appends an
    /// intent record (fsync'd, fail-closed) before the request and an
    /// outcome record after — the only host-local substrate for
    /// reconciling an interrupted mutation. None disables journaling
    /// (library/test use); the binary always sets it.
    journal_dir: Option<std::path::PathBuf>,
}

impl<T: Transport> Client<T> {
    pub fn new(transport: T, profile: Profile) -> Self {
        Self {
            transport,
            profile,
            last_call: Cell::new(None),
            calls: Cell::new(0),
            spacing: MIN_SPACING,
            retry_backoff: RETRY_BACKOFF,
            journal_dir: None,
        }
    }

    pub fn set_journal_dir(&mut self, dir: Option<std::path::PathBuf>) {
        self.journal_dir = dir;
    }

    /// Override throttle spacing and retry backoff (tests use zero so the
    /// retry and pagination paths run without real sleeps).
    pub fn set_timing(&mut self, spacing: Duration, retry_backoff: Duration) {
        self.spacing = spacing;
        self.retry_backoff = retry_backoff;
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn calls(&self) -> u32 {
        self.calls.get()
    }

    /// Issue one read-only API call. `command` may be short
    /// ("domains.getList") or "namecheap."-prefixed. Fail-closed: any
    /// command not on READ_ONLY_COMMANDS is refused here — mutations must
    /// use call_mut, which carries the production gate. Returns the raw
    /// XML body. Retries once on HTTP 429/5xx (reads are idempotent).
    pub fn call(&self, command: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let canonical = canonical_command(command);
        if !READ_ONLY_COMMANDS.contains(&canonical.as_str()) {
            return Err(Error::Policy(format!(
                "{command:?} is not a known read-only command; \
                 mutating commands must use the mutation path"
            )));
        }
        self.dispatch(&canonical, params, true)
    }

    /// The production-mutation gate, callable by command implementations
    /// BEFORE any preparatory reads: a refused mutation should spend no
    /// rate budget and leak no intent to the wire. call_mut re-checks it
    /// as defense in depth.
    pub fn require_mutations_permitted(&self) -> Result<(), Error> {
        if !self.profile.sandbox && !self.profile.allow_production_mutations {
            return Err(Error::Policy(
                "mutations against production are disabled; use a sandbox profile \
                 or set allow_production_mutations = true in this profile"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Issue one mutating API call. Never auto-retries (an ambiguous
    /// failure after a mutation must surface, not double-submit), and is
    /// gated: refused against production unless the profile explicitly
    /// sets allow_production_mutations.
    pub fn call_mut(&self, command: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        self.require_mutations_permitted()?;
        let canonical = canonical_command(command);
        let seq = format!("{}-{}", std::process::id(), self.calls.get() + 1);
        self.journal_intent(&seq, &canonical, params)?;
        let result = self.dispatch(&canonical, params, false);
        self.journal_outcome(&seq, &result);
        result
    }

    /// Reserve purchase budget against the rolling-24h ledger. Fail-closed
    /// throughout: production with no cap configured refuses purchases
    /// outright (arming the mutation gate must never expose unlimited
    /// spend); a cap with no usable ledger refuses; ledger read/write
    /// errors refuse. Records the LISTED price at reservation time —
    /// conservative, since the listed price is what the guard approved.
    pub fn reserve_spend(&self, amount: f64, command: &str, domain: &str) -> Result<(), Error> {
        let cap = match self.profile.max_daily_spend {
            Some(cap) => cap,
            None if self.profile.sandbox => {
                // Unlimited fake money, but still recorded for parity.
                if self.journal_dir.is_some() {
                    self.append_spend(amount, command, domain)?;
                }
                return Ok(());
            }
            None => {
                return Err(Error::Policy(
                    "purchases against production require max_daily_spend in the profile".into(),
                ));
            }
        };
        if self.journal_dir.is_none() {
            return Err(Error::Policy(
                "max_daily_spend is set but no state directory is available to track it".into(),
            ));
        }
        let spent = self.spend_last_24h().map_err(|e| {
            Error::Policy(format!(
                "cannot read the spend ledger ({e}); refusing to purchase"
            ))
        })?;
        if spent + amount > cap {
            return Err(Error::Policy(format!(
                "daily spend cap would be exceeded: {spent:.2} spent in the last 24h \
                 + {amount:.2} requested > max_daily_spend {cap:.2}"
            )));
        }
        self.append_spend(amount, command, domain)
    }

    fn append_spend(&self, amount: f64, command: &str, domain: &str) -> Result<(), Error> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let Some(dir) = &self.journal_dir else {
            return Ok(());
        };
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(dir)?;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .mode(0o600)
                .open(dir.join("spend.jsonl"))?;
            writeln!(
                file,
                "{}",
                serde_json::json!({
                    "ts": unix_now(),
                    "profile": self.profile.name,
                    "sandbox": self.profile.sandbox,
                    "command": command,
                    "domain": domain,
                    "amount": amount,
                })
            )?;
            file.sync_all()
        };
        write().map_err(|e| {
            Error::Policy(format!(
                "cannot record the spend reservation ({e}); refusing to purchase"
            ))
        })
    }

    /// Sum of reservations for THIS profile in the trailing 24 hours.
    fn spend_last_24h(&self) -> std::io::Result<f64> {
        let Some(dir) = &self.journal_dir else {
            return Ok(0.0);
        };
        let path = dir.join("spend.jsonl");
        if !path.exists() {
            return Ok(0.0);
        }
        let cutoff = unix_now().saturating_sub(24 * 60 * 60);
        let mut total = 0.0;
        for line in std::fs::read_to_string(&path)?.lines() {
            let Ok(rec) = serde_json::from_str::<serde_json::Value>(line) else {
                // A corrupt ledger must not silently under-count: refuse.
                return Err(std::io::Error::other("corrupt spend ledger line"));
            };
            let ts = rec["ts"].as_u64().unwrap_or(0);
            let same_profile = rec["profile"].as_str() == Some(self.profile.name.as_str());
            if ts >= cutoff && same_profile {
                total += rec["amount"].as_f64().unwrap_or(0.0);
            }
        }
        Ok(total)
    }

    /// Best-effort context record (e.g. a pre-image) ahead of a mutation.
    pub fn journal_note(&self, command: &str, data: serde_json::Value) {
        let _ = self.journal_append(
            serde_json::json!({
                "ts": unix_now(),
                "kind": "note",
                "profile": self.profile.name,
                "sandbox": self.profile.sandbox,
                "command": command,
                "data": data,
            }),
            false,
        );
    }

    /// Fail-closed: if the intent cannot be durably recorded, the mutation
    /// does not happen — an unjournaled ambiguous outcome is the one state
    /// this control exists to prevent. Params here never contain auth
    /// fields (dispatch adds those after).
    fn journal_intent(
        &self,
        seq: &str,
        command: &str,
        params: &[(&str, &str)],
    ) -> Result<(), Error> {
        if self.journal_dir.is_none() {
            return Ok(());
        }
        let params: serde_json::Map<String, serde_json::Value> = params
            .iter()
            .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
            .collect();
        self.journal_append(
            serde_json::json!({
                "ts": unix_now(),
                "seq": seq,
                "kind": "intent",
                "profile": self.profile.name,
                "sandbox": self.profile.sandbox,
                "command": command,
                "params": params,
            }),
            true,
        )
        .map_err(|e| {
            Error::Policy(format!(
                "cannot record mutation intent in the journal ({e}); refusing to mutate"
            ))
        })
    }

    /// Best-effort: the mutation already happened; a journal hiccup must
    /// not turn a success into an error.
    fn journal_outcome(&self, seq: &str, result: &Result<String, Error>) {
        let record = match result {
            Ok(body) => serde_json::json!({
                "ts": unix_now(),
                "seq": seq,
                "kind": "outcome",
                "ok": true,
                "body_excerpt": body.chars().take(500).collect::<String>(),
            }),
            Err(e) => serde_json::json!({
                "ts": unix_now(),
                "seq": seq,
                "kind": "outcome",
                "ok": false,
                "error": e.to_string(),
            }),
        };
        let _ = self.journal_append(record, false);
    }

    fn journal_append(&self, record: serde_json::Value, fsync: bool) -> std::io::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let Some(dir) = &self.journal_dir else {
            return Ok(());
        };
        std::fs::create_dir_all(dir)?;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(dir.join("mutations.jsonl"))?;
        writeln!(file, "{record}")?;
        if fsync {
            file.sync_all()?;
        }
        Ok(())
    }

    fn dispatch(
        &self,
        canonical: &str,
        params: &[(&str, &str)],
        retry: bool,
    ) -> Result<String, Error> {
        let mut all: Vec<(String, String)> = vec![
            ("ApiUser".into(), self.profile.api_user.clone()),
            ("ApiKey".into(), self.profile.api_key.expose().to_owned()),
            ("UserName".into(), self.profile.username.clone()),
            ("Command".into(), format!("namecheap.{canonical}")),
            ("ClientIp".into(), self.profile.client_ip.clone()),
        ];
        all.extend(
            params
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        );

        // Counts logical API commands; a retried attempt is still one call.
        self.calls.set(self.calls.get() + 1);
        self.throttle();
        let mut attempt = self.transport.send(self.profile.endpoint(), &all);
        if retry
            && matches!(attempt, Err(TransportFailure::Status(code)) if code == 429 || code >= 500)
        {
            thread::sleep(self.retry_backoff);
            self.throttle();
            attempt = self.transport.send(self.profile.endpoint(), &all);
        }
        match attempt {
            Ok(body) => Ok(body),
            Err(TransportFailure::Status(429)) => Err(Error::RateLimited("HTTP 429".into())),
            Err(TransportFailure::Status(code)) => {
                Err(Error::Transport(format!("HTTP status {code}")))
            }
            // Last-line defense: no error string leaves this layer containing
            // the key, regardless of what the HTTP library embeds.
            Err(TransportFailure::Other(msg)) => Err(Error::Transport(redact(
                &msg,
                self.profile.api_key.expose(),
            ))),
        }
    }

    fn throttle(&self) {
        if let Some(prev) = self.last_call.get()
            && prev.elapsed() < self.spacing
        {
            thread::sleep(self.spacing - prev.elapsed());
        }
        self.last_call.set(Some(Instant::now()));
    }
}
