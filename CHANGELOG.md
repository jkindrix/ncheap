# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-06-06

Phase 2: mutating commands, all behind a client-layer safety gate, plus
envelope schema 2.

### Added

- `dns set` — point a domain at custom nameservers
- `privacy enable` / `privacy disable` — domain privacy toggles; the
  WhoisguardID resolves from the domain, `--forward-to` is required and
  never defaulted
- `domains register` / `domains renew` — purchases guarded by a required
  `--max-price` compared against **live** pricing (the cache is never
  consulted for purchase decisions); registration contacts copied from an
  owned domain via `--contacts-from`; premium domains refused
- Mutation gate enforced inside the client: refused against production
  unless the profile file sets `allow_production_mutations = true` (not
  overridable from the environment); mutations never auto-retry; `--yes`
  required non-interactively
- Envelope schema 2: top-level `schema` field, `meta.version` (producing
  binary), `meta` populated on failures once a profile resolved,
  `error.kind` gains `parse` (split from `api`)
- The API's in-band rate-limit error (500000 inside HTTP 200) now maps to
  `rate_limit`/exit 5, giving agents a real back-off signal
- GitHub build-provenance attestations on release artifacts
- End-to-end success-envelope test through the real binary (debug-only
  endpoint override; release builds reach only the two Namecheap hosts)

### Changed

- Safety model documentation now states blast radius plainly: the key is
  account-wide and client-side gates reduce accident probability, not
  compromise impact
- `--max-price` documented as a ceiling on the listed price (ICANN fees
  may apply on top; both figures are reported)

### Fixed

- Error responses carrying a junk `CommandResponse` element no longer mask
  the real API error (two-pass envelope parse)

## [0.1.0] - 2026-06-06

Initial release: the complete read-only command surface.

### Added

- `domains list` — all domains, auto-paginated past the API's silent
  20-item default page size
- `domains check` — availability for up to 50 domains per call, premium
  pricing attributes included
- `domains info` — registration, privacy, DNS provider, modification rights
- `domains lock` — registrar (transfer) lock status
- `domains contacts` — contact sets with PII redacted by default
  (`--full` opts in), plus a contact-set consistency signal
- `dns get` — nameserver mode and host records (when Namecheap-hosted)
- `privacy list` — domain privacy subscriptions, auto-paginated
- `account balances` — amounts redacted by default with a
  covers-auto-renew signal (`--full` opts in)
- `account pricing` — flattened pricing rows with filters, cached 24h
- `raw` — direct API access restricted to a read-only allowlist
- `--json` envelope (`ok`/`command`/`data`/`error`/`meta`) and a documented
  exit-code table on every command, for script and AI-agent consumption
- TOML config with named profiles (production/sandbox), 0600-enforced,
  env-var override, pure-env operation
- Client-side throttling under the documented 20/min key limit, single
  retry with backoff on HTTP 429/5xx
- Credential hygiene: POST form bodies (no key in URLs), HTTPS-only,
  no redirects, `Secret` type with redacted debug output, key scrubbed
  from transport errors
- IDN (punycode) normalization and Public Suffix List-aware domain
  validation on all domain arguments

[0.2.0]: https://github.com/jkindrix/ncheap/releases/tag/v0.2.0
[0.1.0]: https://github.com/jkindrix/ncheap/releases/tag/v0.1.0
