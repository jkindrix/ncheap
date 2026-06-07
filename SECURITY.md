# Security Policy

## Reporting a vulnerability

Use GitHub's private vulnerability reporting on this repository
(Security → Report a vulnerability). Please do not open public issues
for security reports.

This is a single-maintainer project: reports are read on a best-effort
basis with no response-time guarantee. If a report concerns credential
exposure, rotate your Namecheap API key first (Profile → Tools → API
Access) — rotation invalidates the old key immediately and does not
depend on any fix here.

## Scope notes

- The Namecheap API key is account-wide; ncheap's gates are client-side
  accident protection, not a security boundary (see the README's safety
  model).
- Only released artifacts (GitHub releases, crates.io) are supported.
  Release artifacts carry build-provenance attestations; they are not
  reproducible-from-source byte-for-byte.
