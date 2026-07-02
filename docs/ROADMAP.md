# lenslab — roadmap

Living execution document: what is planned, in progress, and done, with status. The design — the
_why_ and _what_ — lives in `GENESIS.md` and the rest of `docs/`; this file is the _when_ and _in
what order_. It is the first source for "what's next".

Built in the open: entries here are the committed, public record of the plan.

## Conventions

Each item names its dependencies and an acceptance criterion (how we know it is done). Move an item
to **Done** in the same change that completes it, with the commit or pull-request reference.

## Up Next

- **Real DNG fixture + `RawlerDecoder` validation** — owner to supply a real camera DNG (plus
  ground-truth camera/lens/exposure values, verified independently with `exiftool`, not just
  trusted) so `RawlerDecoder` gets exercised end-to-end for the first time; currently it is only
  built against `rawler`'s documented API with no real file to decode. Storage is specced and built:
  `docs/DECISIONS.md` D10 (GitHub Release asset, checksum-fetched), `scripts/fetch-dng-fixtures.sh`
  / `just fixtures` with SHA256-pinned entries — still blocked on the files landing on the
  `fixtures-dng-v1` release. Fixture ground truth is captured at
  `tests/fixtures/dng/xtrans_xt3.exiftool.txt` and `tests/fixtures/dng/bayer_k1.exiftool.txt`. The
  Fujifilm X-T3 RAF converted to DNG via Lightroom Classic covers more than expected — the 6×6
  X-Trans `CFARepeatPatternDim` confirms a second CFA layout, and Lightroom attached `OpcodeList2`
  (`FixVignetteRadial`) + `OpcodeList3` (`WarpRectilinear`) for the recognised lens profile, so it
  exercises the corrections-present positive case. The Pentax K-1 native DNG covers the plain Bayer
  case without opcode lists.
  - _Depends on:_ Decode backend + `lenslab inspect`; owner supplies the file and its expected
    values.
  - _Done when:_ `lenslab inspect` against the real DNG is asserted against known-correct
    camera/lens/dimensions/black-and-white-level/CFA-pattern values in a test, and — if the sample
    carries baked-in DNG opcode lists — the corrections-present detection is exercised against a
    real positive case too (currently only tested against their absence).
- **Image model + zone geometry** — `lenslab-core::image` (`LinearImage`/`CfaImage`, planes,
  metadata), single-green-plane extraction, and zone geometry (`docs/ALGORITHMS.md` §Channel,
  §Zones). First step that needs real pixel data rather than just decode metadata — a real DNG to
  decode and check the extraction against de-risks this before it's picked up.
  - _Depends on:_ Decode backend + `lenslab inspect`. Benefits from, but does not strictly require,
    the real DNG fixture above landing first.
  - _Done when:_ a decoded frame can be split into the default 5-point zone layout with patch
    sizing, covered by a unit test.

## In Progress

- _(none yet)_

## Done

- **Scaffold the Cargo workspace** — `lenslab-core`, `lenslab-decode`, `lenslab-cli` (binary
  `lenslab`), wired with the licence boundaries from `docs/DECISIONS.md` (LGPL confined to
  `lenslab-decode`). `just ci` green on the empty-but-wired workspace.
- **CI and release workflows** — `.github/workflows/ci.yml` runs `just ci` plus a four-target
  cross-compile matrix on every push and pull request to `main`, gated by an aggregator `ci` job.
  `.github/workflows/release.yml` cuts a tagged release (`verify` → `build` → owner-approval-gated
  `publish`), backed by `CHANGELOG.md`, `scripts/release-prep.sh`, and `docs/release-process.md`.
  _Done when:_ both workflow files exist and `just ci` runs green in GitHub Actions — met (all 10
  checks, including the 4-target cross-compile matrix, passed on PR #2's head commit).
- **Configure the `release` GitHub Environment** — required reviewers + tag deployment policy for
  `release.yml`'s `publish` job. Owner-confirmed as configured; not independently verified from this
  session (no `gh` CLI or environments-API access available here). See `docs/release-process.md` for
  the verification commands to re-run before the first real tag.
- **Decode backend + `lenslab inspect`** — `Decoder` trait in `lenslab-decode` with two
  implementations: `RawlerDecoder` (DNG and other camera raws via `rawler`, the LGPL-2.1 boundary)
  and `TiffDecoder` (already-demosaiced TIFF via the permissive `tiff`/`kamadak-exif` crates, no
  `rawler` dependency). `lenslab inspect <file>` prints EXIF, decode info, and a DNG
  opcode-list-derived corrections-present flag as JSON — metadata only, no pixel data and no
  measurement, the smallest end-to-end slice through decode (`docs/GENESIS.md` "Start here", step
  1). `TiffDecoder` is covered by an integration test against a synthetic TIFF fixture written with
  the `tiff` crate's own encoder (including a regression test for multi-channel `BitsPerSample`,
  caught by manually running the built binary before committing). `RawlerDecoder` has no equivalent
  fixture yet — no camera raw exists in this repository, and `rawler`'s own crates.io package ships
  only digest/metadata files for its test corpus, not the raw samples themselves; see "Real DNG
  fixture" above, now the next item up.

## Deferred / known gaps

Carried from initial workspace setup; revisit when the noted condition is met.

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
- **macOS binaries are unsigned** — the cross-compile matrix and release pipeline produce
  `aarch64-apple-darwin`/`x86_64-apple-darwin` binaries with no Developer ID signature or
  notarization, so Gatekeeper quarantine-blocks them by default on download (a manual
  right-click-Open or `xattr -d com.apple.quarantine` override is needed). Owner has an Apple
  Developer Program membership to wire in when this is picked up; revisit alongside
  `docs/release-process.md` before macOS binaries are meant for anyone other than the owner
  building/running them directly.
