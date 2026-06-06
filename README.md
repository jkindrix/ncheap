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

Credentials live in `~/.config/ncheap/config.toml` (must be `chmod 600`;
ncheap refuses group/other-readable config files):

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
ncheap privacy list                    # domain privacy subscriptions
ncheap account balances                # amounts redacted unless --full
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
| 5 | Rate-limited after backoff |

## Safety model

- The API key is never written to logs, error messages, or request traces.
- Read-only commands are the only ones implemented today. Mutating commands
  (nameservers, privacy toggles, registration/renewal) will ship
  sandbox-gated and disabled against production until explicitly enabled in
  config, with `--yes` required for non-interactive use.
- Client-side throttling spaces requests under Namecheap's 50/min key-wide
  rate limit, with backoff on HTTP 429/5xx.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
