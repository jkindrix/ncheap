# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.0] - 2026-06-07

The two largest user-facing features since 0.2.0.

### Added

- `audit` — every read-only safety check across the account as one
  command (3 + 2N API calls): expiry horizon, auto-renew × balance
  funding, expired/registry-hold states, transfer locks, privacy,
  DNS posture, contact consistency. Findings ranked critical/warning/
  info; findings are data (exit 0).
- `dns add` / `dns remove` — host-record editing built on the
  full-replace setHosts: complete zone pre-image journaled, EmailType
  (mail routing) preserved across rewrites, duplicate adds and
  empty-zone removals refused, MX requires --mx-pref. Concurrent edits
  to one zone are last-writer-wins (documented).

## [0.6.0] - 2026-06-07

Cross-process coordination and operator-safety polish.

### Added

- Concurrent ncheap processes on one machine now serialize API calls
  through a state-directory lock file (fail-open)
- `--expect-profile NAME`: refuse before any API call when the resolved
  profile differs — guards against a leaked `NCHEAP_PROFILE`
- Human-mode output strips C0/C1 control characters from
  server-controlled strings (`raw` remains verbatim)
- SECURITY.md and GitHub private vulnerability reporting

### Changed (breaking for builders only)

- MSRV 1.88 -> 1.89 (`std::fs::File::lock`; removes the need for any
  file-locking dependency)
- The spend-cap check holds an exclusive lock across check-and-reserve:
  concurrent purchases cannot both pass the cap
- Privacy ID resolution stops paging once the domain is found

## [0.5.0] - 2026-06-07

Spend caps and live verification of the failure modes. With this release
every precondition for arming production mutations is met.

### Added

- `max_daily_spend` profile field: a rolling-24h purchase budget enforced
  via a fail-closed 0600 ledger (`spend.jsonl`), recording listed prices
  at reservation. Config-file-only — the environment cannot raise a
  budget. **Production purchases are refused entirely until a cap is
  set**, so arming the mutation gate never exposes unlimited spend;
  sandbox is unlimited when uncapped (still recorded).

### Changed

- HTTP 405 now maps to `rate_limit`/exit 5: a deliberate burst against
  the live (sandbox) API showed the real rate limiter answers 405 with an
  HTML page — not the conventional 429 and not the third-party-reported
  in-band error 500000. All three shapes map to exit 5, each best-effort.

### Verified

- Interrupted-mutation handling end-to-end: a SIGKILL mid-renew leaves an
  intent record without an outcome in the journal, and reconciliation via
  `domains info` correctly distinguishes committed from not-committed.

## [0.4.0] - 2026-06-07

Purchase-path hardening from the fifth external review, plus the mutation
journal. Envelope changes are additive (schema stays 3).

### Added

- Mutation journal: append-only 0600 JSONL at
  `~/.local/state/ncheap/mutations.jsonl` — fsync'd intent record before
  every mutation, outcome record after, pre-image notes (previous
  nameservers, previous lock state). If intent cannot be recorded, the
  mutation is refused. An interrupted mutation is detectable as an
  intent without an outcome.
- `domains lock --lock` / `--unlock` — registrar transfer lock toggle
  (envelope command `domains.lock.set`), with the pre-image in the result
- `charged_exceeded_max_price` flag on register/renew results
- `previous_nameservers` on `dns set` results

### Changed

- Mutation-outcome response fields no longer default: upstream drift
  fails as a parse error instead of a false "registered: false"
- The price guard matches pricing rows by action category and product,
  not just duration
- Early Access Phase (EAP) domains are refused at register, like premium
- The production-mutation gate fires before any preparatory read: a
  refused mutation generates zero API traffic
- Pricing cache files are written 0600
- Key redaction skips sub-8-char keys and covers the percent-encoded form

## [0.3.0] - 2026-06-07

Envelope schema 3 — driven by first-consumer feedback from an AI agent.

### Changed (breaking)

- All dates in envelope data are normalized to ISO-8601 (`YYYY-MM-DD`);
  the API's `MM/DD/YYYY` strings caused a real consumer to string-sort
  expiry dates wrong. Unrecognized date formats pass through verbatim.
  `raw` output is unaffected.
- `is_locked` renamed to `registry_hold` (and the human table column
  `LOCK` to `HOLD`): the field reports the API's registry/dispute hold,
  not the registrar transfer lock, and the old name misled two
  independent consumers. Transfer lock remains `domains lock`.

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

[0.7.0]: https://github.com/jkindrix/ncheap/releases/tag/v0.7.0
[0.6.0]: https://github.com/jkindrix/ncheap/releases/tag/v0.6.0
[0.5.0]: https://github.com/jkindrix/ncheap/releases/tag/v0.5.0
[0.4.0]: https://github.com/jkindrix/ncheap/releases/tag/v0.4.0
[0.3.0]: https://github.com/jkindrix/ncheap/releases/tag/v0.3.0
[0.2.0]: https://github.com/jkindrix/ncheap/releases/tag/v0.2.0
[0.1.0]: https://github.com/jkindrix/ncheap/releases/tag/v0.1.0
