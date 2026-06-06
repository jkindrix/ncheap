pub mod xml;

use std::cell::Cell;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::Profile;

/// Minimum spacing between API calls: the key-wide limit is 50/min.
const MIN_SPACING: Duration = Duration::from_millis(1300);
/// Backoff before the single retry on HTTP 429/5xx. The API documents no
/// rate-limit error shape, so this is conservative, not tuned.
const RETRY_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Config(#[from] crate::config::ConfigError),
    #[error("transport: {0}")]
    Transport(String),
    #[error("rate limited by Namecheap (HTTP {0}); retry later")]
    RateLimited(u16),
    #[error("Namecheap API error {code}: {message}")]
    Api { code: String, message: String },
    #[error("unexpected API response: {0}")]
    Parse(String),
    #[error("{0}")]
    Usage(String),
}

impl Error {
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::Api { .. } | Error::Parse(_) => 1,
            Error::Usage(_) => 2,
            Error::Config(_) => 3,
            Error::Transport(_) => 4,
            Error::RateLimited(_) => 5,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Error::Api { .. } => "api",
            Error::Parse(_) => "api",
            Error::Usage(_) => "usage",
            Error::Config(_) => "config",
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
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
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
        let mut req = self.agent.get(endpoint);
        for (k, v) in params {
            req = req.query(k, v);
        }
        match req.call() {
            Ok(mut resp) => resp
                .body_mut()
                .read_to_string()
                .map_err(|e| TransportFailure::Other(e.to_string())),
            Err(ureq::Error::StatusCode(code)) => Err(TransportFailure::Status(code)),
            Err(e) => Err(TransportFailure::Other(e.to_string())),
        }
    }
}

pub struct Client<T: Transport> {
    transport: T,
    profile: Profile,
    last_call: Cell<Option<Instant>>,
    calls: Cell<u32>,
}

impl<T: Transport> Client<T> {
    pub fn new(transport: T, profile: Profile) -> Self {
        Self {
            transport,
            profile,
            last_call: Cell::new(None),
            calls: Cell::new(0),
        }
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

    /// Issue one API call. `command` may be short ("domains.getList"); the
    /// "namecheap." prefix is added when absent. Returns the raw XML body.
    pub fn call(&self, command: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let command = if command.starts_with("namecheap.") {
            command.to_owned()
        } else {
            format!("namecheap.{command}")
        };
        let mut all: Vec<(String, String)> = vec![
            ("ApiUser".into(), self.profile.api_user.clone()),
            ("ApiKey".into(), self.profile.api_key.expose().to_owned()),
            ("UserName".into(), self.profile.username.clone()),
            ("Command".into(), command),
            ("ClientIp".into(), self.profile.client_ip.clone()),
        ];
        all.extend(
            params
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        );

        self.throttle();
        let mut attempt = self.transport.send(self.profile.endpoint(), &all);
        if matches!(attempt, Err(TransportFailure::Status(code)) if code == 429 || code >= 500) {
            thread::sleep(RETRY_BACKOFF);
            self.throttle();
            attempt = self.transport.send(self.profile.endpoint(), &all);
        }
        match attempt {
            Ok(body) => Ok(body),
            Err(TransportFailure::Status(429)) => Err(Error::RateLimited(429)),
            Err(TransportFailure::Status(code)) => {
                Err(Error::Transport(format!("HTTP status {code}")))
            }
            // Last-line defense: no error string leaves this layer containing
            // the key, regardless of what the HTTP library embeds.
            Err(TransportFailure::Other(msg)) => Err(Error::Transport(
                msg.replace(self.profile.api_key.expose(), "<redacted>"),
            )),
        }
    }

    fn throttle(&self) {
        if let Some(prev) = self.last_call.get() {
            let elapsed = prev.elapsed();
            if elapsed < MIN_SPACING {
                thread::sleep(MIN_SPACING - elapsed);
            }
        }
        self.last_call.set(Some(Instant::now()));
        self.calls.set(self.calls.get() + 1);
    }
}
