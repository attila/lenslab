# lenslab — roadmap

Living execution document: what is planned, in progress, and done, with status. The design — the
_why_ and _what_ — lives in `GENESIS.md` and the rest of `docs/`; this file is the _when_ and _in
what order_. It is the first source for "what's next".

Built in the open: entries here are the committed, public record of the plan.

## Conventions

Each item names its dependencies and an acceptance criterion (how we know it is done). Move an item
to **Done** in the same change that completes it, with the commit or pull-request reference.

## Up Next

- **Acutance metric + `analyse` skeleton** — add the first machine-readable measurement path:
  consume decoded frames, measure per-zone relative acutance on the existing green/luma planes, and
  emit the initial canonical JSON shape without adding verdict synthesis or extra metrics.
  - _Depends on:_ Image model + zone geometry; decode pixel path.
  - _Done when:_ `lenslab analyse <paths…>` emits deterministic JSON with measured per-zone acutance
    values for synthetic inputs, keeps diagnostics off stdout, and preserves the
    measured-vs-inferred split.

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
  caught by manually running the built binary before committing).
- **Real DNG fixture + `RawlerDecoder` validation** — checksum-pinned real-camera DNG fixtures are
  hosted as GitHub Release assets under `fixtures-dng-v1`, fetched by
  `scripts/fetch-dng-fixtures.sh` / `just fixtures`, and exercised by `just test-fixtures` plus CI's
  fixture test job. The fixture ground truth is captured at
  `tests/fixtures/dng/xtrans_xt3.exiftool.txt` and `tests/fixtures/dng/bayer_k1.exiftool.txt`. The
  Fujifilm X-T3 DNG covers a 6x6 X-Trans CFA plus positive DNG opcode-list corrections; the Pentax
  K-1 native DNG covers a plain Bayer case without opcode lists. _Done when:_ `lenslab inspect`
  against real DNGs is asserted against known-correct camera, dimensions, black/white level, CFA
  pattern, exposure, and corrections-present values — met by PR #4 (`fc6352c`).
- **Image model + zone geometry** — `lenslab-core::image` now owns validated `LinearImage`,
  `CfaImage`, RGB/luma input buffers, provenance, CFA pattern metadata, and borrowed patch views.
  Core extracts a single native Bayer green phase without demosaic interpolation or G1/G2 averaging,
  computes Rec.709 luma for RGB/TIFF inputs, and projects the documented five-zone source-frame
  layout into measurement-plane coordinates. `lenslab-decode::Decoder` now has a pixel-bearing
  `decode` path that returns plain project image types while keeping `rawler` and `tiff` confined to
  `lenslab-decode`; `inspect` remains metadata-only. _Done when:_ a decoded or synthetic frame can
  be split into the default five-point zone layout with patch sizing, covered by tests — met by the
  core composition tests for synthetic CFA and RGB frames.
- **Contact sheet output** — `lenslab contact <paths…> --out <file>` decodes DNG/TIFF frames,
  derives a deterministic display plane from Bayer green or RGB/luma data, and writes a labelled PNG
  contact sheet without changing `inspect` or starting metric work. Output writes are atomic, stdout
  stays empty for the human artefact command, unsupported CFA display fails honestly, and the
  real-fixture gate covers both Bayer success and X-Trans failure. _Done when:_ synthetic TIFF and
  real-fixture tests cover deterministic PNG creation, labels, output safety, and stdout/stderr
  separation — met by PR #7.

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
- **X-Trans green extraction is still unsupported** — core preserves unsupported CFA pattern
  metadata and returns a typed error instead of approximating a green plane. Add correct X-Trans
  extraction later if it becomes small enough to fit the v0.1 path without violating the
  measured-vs-inferred rule.
- **CLI exit-code taxonomy is not defined yet** — `lenslab` currently distinguishes success from
  failure, but not all user-facing failure classes have stable numeric exit codes. Define the
  command-wide taxonomy first (usage/config error, unsupported input, decode failure, render/output
  failure, internal bug), then implement typed error mapping across existing commands instead of
  changing `contact` alone.
- **CI workflow pins emit maintenance warnings** — GitHub Actions currently reports Node.js 20
  deprecation annotations for pinned actions such as `actions/checkout` and `mlugg/setup-zig`, and
  `taiki-e/install-action` falls back to `cargo-binstall` for `dprint@0.55.1` on the Ubuntu runner.
  These are not failing checks today, but they are release-engineering drift: review and update the
  pinned action SHAs/comments, confirm the replacements' runtime declarations, and decide whether to
  keep installing dprint through `taiki-e/install-action` or switch to a quieter pinned install
  path. _Done when:_ CI and release workflow runs complete without these maintenance annotations
  while preserving pinned third-party actions, read-only CI permissions, and the existing release
  approval boundary.
