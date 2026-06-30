# Security

## Status

lenslab is pre-implementation. This document states the intended threat model and how to report
issues; specific mitigations will be documented here as the code lands.

## Threat model

lenslab is a **local, single-user command-line tool**. It runs under the invoking user's
permissions, with no network surface, no listener, no authentication, and no multi-user access. The
Claude plugin orchestrates the same local binary.

The primary surface is **untrusted input files**: lenslab decodes DNG/TIFF/raw frames, and a
malformed or hostile file is the realistic attack vector.

- **Decoder robustness** — raw/TIFF parsing (via `rawler`/`rawloader` in `lenslab-decode`) is the
  main surface. A crafted file should fail cleanly, not crash, hang, or read out of bounds.
- **Resource exhaustion** — oversized or pathological frames driving unbounded memory or time.
- **Path handling** — input folders and output artefact paths must stay within the directories the
  user named: no traversal, no writes outside the output target.

## Posture

- Memory-safe Rust throughout; `unsafe_code = "deny"` globally, with any FFI exception isolated to
  the smallest scope and a `// SAFETY:` note.
- Decode is confined to `lenslab-decode`; `lenslab-core` and `lenslab-cli` do not touch third-party
  raw parsers directly.
- Dependencies are audited with `cargo-deny` (advisories, licences, bans).

## Reporting

Report vulnerabilities privately through [GitHub Security Advisories](../../security/advisories/new)
or to the maintainer directly — never through public channels. lenslab is not accepting external
code contributions yet (see [`CONTRIBUTING.md`](CONTRIBUTING.md)), but security reports are always
welcome.
