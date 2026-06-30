# lenslab — roadmap

Living execution document: what is planned, in progress, and done, with status. The design — the
_why_ and _what_ — lives in `GENESIS.md` and the rest of `docs/`; this file is the _when_ and _in
what order_. It is the first source for "what's next".

Built in the open: entries here are the committed, public record of the plan.

## Conventions

Each item names its dependencies and an acceptance criterion (how we know it is done). Move an item
to **Done** in the same change that completes it, with the commit or pull-request reference.

## Up Next

- **Scaffold the Cargo workspace** — create `lenslab-core`, `lenslab-cli`, and `lenslab-decode` with
  the licence boundaries from `docs/DECISIONS.md` (LGPL confined to `lenslab-decode`).
  - _Depends on:_ nothing.
  - _Done when:_ `just ci` runs green on an empty-but-wired workspace.
- **CI and release workflows** — add `.github/workflows/` once the workspace builds a binary
  (deferred from initial setup, since both reference a binary that does not exist yet).
  - _Depends on:_ Cargo workspace scaffold.
  - _Done when:_ CI runs `just ci` on push and pull request and is green.

## In Progress

- _(none yet)_

## Done

- _(nothing committed yet)_

## Deferred / known gaps

Carried from initial workspace setup; revisit when the noted condition is met.

- **`deny.toml` targets** — currently `x86_64-unknown-linux-gnu` only (mirrored from the reference
  setup). lenslab reads cross-platform; widen the target list when the workspace lands.
