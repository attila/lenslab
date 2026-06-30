# lenslab — roadmap

Living execution document: what is planned, in progress, and done, with status. The design — the
_why_ and _what_ — lives in `GENESIS.md` and the rest of `docs/`; this file is the _when_ and _in
what order_. It is the first source for "what's next".

Built in the open: entries here are the committed, public record of the plan.

## Conventions

Each item names its dependencies and an acceptance criterion (how we know it is done). Move an item
to **Done** in the same change that completes it, with the commit or pull-request reference.

## Up Next

- _(none yet)_

## In Progress

- _(none yet)_

## Done

- **Scaffold the Cargo workspace** — `lenslab-core`, `lenslab-decode`, `lenslab-cli` (binary
  `lenslab`), wired with the licence boundaries from `docs/DECISIONS.md` (LGPL confined to
  `lenslab-decode`). `just ci` green on the empty-but-wired workspace.
- **CI and release workflows** — `.github/workflows/ci.yml` runs `just ci` plus a four-target
  cross-compile matrix on every push and pull request to `main`. `.github/workflows/release.yml`
  cuts a tagged release (`verify` → `build` → owner-approval-gated `publish`), backed by
  `CHANGELOG.md`, `scripts/release-prep.sh`, and `docs/release-process.md`. The `release` GitHub
  Environment (required reviewers + tag deployment policy) is a one-time manual prerequisite — see
  `docs/release-process.md` — not yet configured.

## Deferred / known gaps

Carried from initial workspace setup; revisit when the noted condition is met.

- **`deny.toml` targets** — currently `x86_64-unknown-linux-gnu` only (mirrored from the reference
  setup). lenslab reads cross-platform; widen the target list when the workspace lands.
