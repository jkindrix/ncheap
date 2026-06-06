# ncheap

A command-line tool for the Namecheap registrar API, built for terminal use
and AI-agent operability: structured `--json` output, meaningful exit codes,
and non-interactive operation by default.

**Status: early development.** Phase 1 (read-only commands) in progress.

## Build

```
cargo build --release
```

Produces a single binary at `target/release/ncheap`.

## Configuration

Credentials live in `~/.config/ncheap/config.toml` on Linux, or
`~/Library/Application Support/ncheap/config.toml` on macOS (must be
`chmod 600`; ncheap refuses group/other-readable config files):

```toml
default_profile = "production"

[profile.production]
api_user = "your-namecheap-username"
api_key = "your-api-key"
client_ip = "203.0.113.10"   # your whitelisted outbound IPv4

[profile.sandbox]
api_user = "your-sandbox-username"
api_key = "your-sandbox-api-key"
client_ip = "203.0.113.10"
sandbox = true
```

`username` defaults to `api_user`. Environment variables override the config
file: `NCHEAP_API_USER`, `NCHEAP_API_KEY`, `NCHEAP_USERNAME`,
`NCHEAP_CLIENT_IP`, `NCHEAP_SANDBOX`, `NCHEAP_PROFILE`. Pure-env operation
(no config file) is supported.

Namecheap's API requires the calling IP to be whitelisted (IPv4 only) under
Profile → Tools → API Access in the Namecheap dashboard.

## Usage

```
ncheap domains list                    # all domains, auto-paginated
ncheap domains check example.com ...   # availability (up to 50 per call)
ncheap domains info example.com        # registration, privacy, DNS details
ncheap domains lock example.com        # registrar (transfer) lock status
ncheap domains contacts example.com    # contacts; PII redacted unless --full
ncheap dns get example.com             # nameserver mode + host records
ncheap dns set example.com ns1.host ns2.host   # mutating; see safety model
ncheap privacy list                    # domain privacy subscriptions
ncheap privacy enable example.com --forward-to you@example.org   # mutating
ncheap privacy disable example.com     # mutating
ncheap account balances                # amounts redacted unless --full
ncheap domains register new.com --max-price 15 --contacts-from owned.com
ncheap domains renew owned.com --max-price 20   # both mutating, price-guarded
ncheap account pricing --action REGISTER --product com   # cached 24h
ncheap raw domains.getTldList          # direct API call, raw XML out
ncheap raw domains.getInfo --param DomainName=example.com
```

`raw` only calls methods on a read-only allowlist (the wrapped Phase 1
methods plus `domains.getTldList`); mutating methods are refused, and
authentication parameters cannot be supplied via `--param`.

Any command takes `--json` for the machine-readable envelope. Domains for
`dns` commands may be IDN (normalized to punycode) and are split SLD/TLD via
the Public Suffix List, so `example.co.uk` works; subdomains are rejected
with a suggestion rather than silently trimmed.

List commands auto-paginate: accounts with more than 20 domains are fetched
completely, not truncated at the API's default page size.

### JSON envelope

Every command with `--json` emits one envelope on stdout:

```json
{
  "ok": true,
  "command": "domains.list",
  "data": [ ... ],
  "error": null,
  "meta": { "profile": "production", "sandbox": false, "api_calls": 1 }
}
```

On failure `ok` is `false` and `error` carries `kind`
(`config|transport|auth|api|rate_limit`), `code` (Namecheap error number, if
any), and `message`.

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success (per-item results such as an unavailable domain are data, not errors) |
| 1 | Namecheap API returned an error response |
| 2 | Usage error (bad arguments) |
| 3 | Configuration / credential error |
| 4 | Transport / network error |
| 5 | Rate-limited after backoff (defensive: Namecheap documents no rate-limit response shape; a real limit breach may instead surface as an API error, exit 1) |

#### Envelope compatibility

The envelope's five top-level keys (`ok`/`command`/`data`/`error`/`meta`),
the `error.kind` values, and the exit-code meanings are stable. New fields
or new `kind` values may be added in minor versions (additive); removing or
renaming any of them is a breaking change and bumps the major version.
Per-command `data` shapes follow the same rule. Note: if stdout closes
mid-write (e.g. piping to `head`), ncheap exits 0 like standard tools —
consumers should treat truncated JSON as incomplete output, not as a
command result.

## Releasing

Releases are automated by [dist](https://opensource.axo.dev/cargo-dist/):
bump the version in `Cargo.toml`, update `CHANGELOG.md`, run
`cargo update -p psl` (the embedded Public Suffix List snapshot is frozen
into each binary at build time), commit, then tag `vX.Y.Z` and push the
tag. CI builds the binaries, checksums, and installer.

## Safety model

- The API key is never written to logs, error messages, or request traces.
  Requests are sent as POST with a form body, so the key never appears in a
  URL; the HTTP agent is HTTPS-only and follows no redirects.
- Purchasing commands (`domains register`, `domains renew`) additionally
  require `--max-price` and refuse pre-flight if the **live** listed price
  exceeds it — the pricing cache is never consulted for purchase decisions.
  Registration contacts are copied from an owned domain (`--contacts-from`);
  ncheap stores no contact data. Premium domains are refused. The actual
  charge can exceed the listed price slightly (ICANN fees); both figures are
  reported in the result.
- Mutating commands (`dns set`, `privacy enable/disable`, `domains
  register/renew`) are enforced at the client layer,
  not per-command: they are refused against production unless the profile
  sets `allow_production_mutations = true` **in the config file** (the
  environment deliberately cannot arm this), they require `--yes`
  non-interactively (or an interactive confirmation), and they never
  auto-retry — an ambiguous failure after a mutation surfaces instead of
  double-submitting. Sandbox profiles may always mutate.
- Client-side throttling spaces requests ~3s apart **within one invocation**,
  with backoff on HTTP 429/5xx. Namecheap's FAQ has stated the per-minute
  key-wide limit as both 20/min and 50/min at different times (700/hour and
  8000/day are consistent across sources); ncheap spaces for the
  conservative reading. Concurrent ncheap processes do not coordinate: they
  share one key budget, so avoid running many instances in parallel against
  the same key.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
