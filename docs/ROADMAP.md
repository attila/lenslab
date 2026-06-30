# lenslab — roadmap

Living execution document: what is planned, in progress, and done, with status. The design — the
_why_ and _what_ — lives in `GENESIS.md` and the rest of `docs/`; this file is the _when_ and _in
what order_. It is the first source for "what's next".

Built in the open: entries here are the committed, public record of the plan.

## Conventions

Each item names its dependencies and an acceptance criterion (how we know it is done). Move an item
to **Done** in the same change that completes it, with the commit or pull-request reference.

## Up Next

- **Image model + zone geometry** — `lenslab-core::image` (`LinearImage`/`CfaImage`, planes,
  metadata), single-green-plane extraction, and zone geometry (`docs/ALGORITHMS.md` §Channel,
  §Zones). First step that needs real pixel data rather than just decode metadata.
  - _Depends on:_ Decode backend + `lenslab inspect`.
  - _Done when:_ a decoded frame can be split into the default 5-point zone layout with patch
    sizing, covered by a unit test.

## In Progress

- **CI and release workflows** — add `.github/workflows/` once the workspace builds a binary
  (deferred from initial setup, since both reference a binary that does not exist yet).
  - _Depends on:_ Cargo workspace scaffold.
  - _Done when:_ CI runs `just ci` on push and pull request and is green.

## Done

- **Scaffold the Cargo workspace** — `lenslab-core`, `lenslab-decode`, `lenslab-cli` (binary
  `lenslab`), wired with the licence boundaries from `docs/DECISIONS.md` (LGPL confined to
  `lenslab-decode`). `just ci` green on the empty-but-wired workspace.
- **Decode backend + `lenslab inspect`** — `Decoder` trait in `lenslab-decode` with two
  implementations: `RawlerDecoder` (DNG and other camera raws via `rawler`, the LGPL-2.1 boundary)
  and `TiffDecoder` (already-demosaiced TIFF via the permissive `tiff`/`kamadak-exif` crates, no
  `rawler` dependency). `lenslab inspect <file>` prints EXIF, decode info, and a DNG
  opcode-list-derived corrections-present flag as JSON — metadata only, no pixel data and no
  measurement, the smallest end-to-end slice through decode (`docs/GENESIS.md` "Start here", step
  1). `TiffDecoder` is covered by an integration test against a synthetic TIFF fixture written with
  the `tiff` crate's own encoder (including a regression test for multi-channel `BitsPerSample`,
  caught by manually running the built binary before committing). `RawlerDecoder` has no equivalent
  fixture yet — see deferred gaps below.

## Deferred / known gaps

Carried from initial workspace setup; revisit when the noted condition is met.

- **`deny.toml` targets** — currently `x86_64-unknown-linux-gnu` only (mirrored from the reference
  setup). lenslab reads cross-platform; widen the target list when the workspace lands.
- **No DNG decode fixture** — `RawlerDecoder` (the LGPL `rawler` path) has no real or synthetic DNG
  to test against: no camera raw exists in this repository, and `rawler`'s own crates.io package
  ships only digest/metadata files for its test corpus, not the raw samples themselves. The
  implementation is built strictly against `rawler`'s public, documented API
  (`raw_image`/`raw_metadata`/`ifd`), mirroring the access patterns `rawler`'s own DNG decoder uses
  internally, but it has not been exercised end-to-end. Revisit when building the synthetic-fixture
  generator for measurement validation (`docs/GENESIS.md` step 7) — a minimal synthetic DNG would
  cover both needs.
- **`RawlerDecoder::inspect` always fully decompresses** — `raw_image(.., dummy: false)` is called
  unconditionally even though `inspect` never reads pixel data. Checked against `rawler` 0.7.2:
  passing `dummy: true` is honoured for NEF/ARW/CR3/RAF (their decoders skip decompression and leave
  the pixel buffer uninitialised) but ignored for DNG, which always decompresses regardless
  (`plain_image_from_ifd` hardcodes the allocation flag). Switching to `dummy: true` would speed up
  `inspect` on the non-DNG formats `decoder_for` also routes here, but only after confirming none of
  those decoders derive `blacklevel`/`whitelevel` from actual pixel data (e.g. histogram-based auto
  black-level) when `dummy` is set — getting that wrong would silently fabricate a "measured" value,
  which is worse than the current correctness-first slowness. Revisit with that audit done per
  format.
