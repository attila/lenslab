# lenslab — roadmap

Living execution document: what is planned, in progress, and done, with status. The design — the
_why_ and _what_ — lives in `GENESIS.md` and the rest of `docs/`; this file is the _when_ and _in
what order_. It is the first source for "what's next".

Built in the open: entries here are the committed, public record of the plan.

## Conventions

Each item names its dependencies and an acceptance criterion (how we know it is done). Move an item
to **Done** in the same change that completes it, with the commit or pull-request reference.

## Up Next

- **Guided capture workflow / plugin interpretation boundary** — turn the `copy_assessment` evidence
  into user-facing coaching and narrative interpretation without moving judgement into the CLI. The
  first slice should inspect an explicit or folder-backed capture set, explain why support is
  centred/decentred/inconclusive, and coach the smallest reshoot when hard support is blocked.

## Remaining v0.1 Measurement Backlog

- _(none queued beyond Up Next)_

## Product-Grade Blockers

These are the bars that turn the evidence engine into a product someone can trust for a keep/return
decision. They may be implemented as separate PRs or folded into the backlog items above when the
acceptance criteria are met.

- **Trust calibration before hard verdicts** — a hard `centred`/`decentred` verdict must require a
  gated target series, stable correction provenance, and structured evidence explaining why
  decentring is separated from field curvature, framing tilt, lighting gradients, and scene content.
  _Done when:_ the verdict JSON/plugin boundary refuses unsupported evidence, emits `inconclusive`
  with reshoot guidance when capture is not good enough, and has tests covering the main
  false-positive paths.
- **Guided capture workflow** — users should not have to infer the shoot protocol from docs before
  getting useful output. The tool/plugin must inspect a sample set, explain missing aperture/target
  evidence, and coach the next capture without guessing from weak scene data. _Done when:_ a user
  can point the tool at a folder, get either a valid measurement run or precise reshoot
  instructions, and no copy verdict is produced from uncontrolled inputs alone.
- **Golden schema and numeric regression corpus** — schema stability and metric accuracy need
  durable fixtures, not only unit tests for blockers. _Done when:_ controlled synthetic/target
  fixture sets assert known MTF/vignetting/CA/distortion/keystone/aperture-series values within
  documented tolerances, and byte-stable golden JSON snapshots catch accidental public-contract
  drift.
- **Vignetting threshold calibration** — the controlled-series and symmetry thresholds should be
  tuned against synthetic boundary cases and the local real aperture-ladder pool after the first
  conservative implementation lands. _Done when:_ each threshold has a named product meaning,
  synthetic boundary fixtures around pass/fail edges, local real-capture evidence from at least one
  prominent-vignetting lens, and documented changes when the defaults move.

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
- **Acutance metric + `analyse` skeleton** — `lenslab analyse <paths…>` accepts explicit DNG/TIFF
  files, rejects known-corrected inputs, measures acutance and contrast across the default five
  zones, and emits deterministic pretty JSON using skeleton schema `0.1-acutance`. The JSON records
  correction provenance, texture usability, and aggregation eligibility; it deliberately emits no
  lens-copy verdict, QA result, artefacts, vignetting, CA, distortion, field curvature, or MTF50.
  _Done when:_ synthetic TIFF and real-fixture tests cover stdout/stderr separation, byte-stable
  output, Bayer DNG measurement, TIFF unknown-correction provenance, and corrected-input rejection.
- **Decentring aggregation + first QA gate** — `lenslab analyse <paths…>` emits skeleton schema
  `0.1-decentring` with group-level left/right corner-pair evidence derived from measured acutance,
  pair-local exclusions for unknown corrections and low texture, and target quality marked as not
  assessed until real keystone estimation exists. It still emits no centred/decentred copy verdict.
  _Done when:_ decentring signals and QA exclusions are represented in JSON without presenting a
  scene-only or ungated inference as a copy verdict — met by commit `2e43560`.
- **Vignetting aperture-difference skeleton** — `lenslab analyse <paths…>` emits skeleton schema
  `0.1-vignetting` with measured centre/corner luminance falloff, per-group raw falloff evidence,
  unknown-correction exclusions, and reference-relative aperture-difference machinery blocked until
  controlled aperture-series evidence exists. It still emits no optical verdict or radial symmetry
  conclusion. _Done when:_ measured falloff evidence is reported without inferring optical
  vignetting from uncontrolled scene-only data; unknown-correction inputs remain measurable for
  inspection but excluded from optical aggregation; synthetic tests cover known falloff,
  deterministic output, and stdout-empty failures — met by this change.
- **Lateral CA skeleton** — `lenslab analyse <paths…>` emits skeleton schema `0.1-ca` with measured
  per-frame red/blue lateral CA evidence in px@fullres, per-corner group summaries, blockers, and
  unknown-correction exclusions. It still emits no lens-copy verdict, LoCA, distortion,
  field-curvature, MTF50, artefacts, or plugin interpretation. _Done when:_ synthetic RGB tests
  cover a known injected channel shift, zero shift, flat-profile blockers, deterministic output, and
  unknown-correction exclusion; real fixture tests keep Bayer success and X-Trans/corrected
  rejection behaviour intact — met by commit `7f49d6b`.
- **Distortion skeleton** — `lenslab analyse <paths…>` emits skeleton schema `0.1-distortion` with
  frame-level straight-line bow candidates, measured/inferred method codes, blocker evidence for
  unsupported geometry, and group summaries that exclude weak references and unknown-correction
  frames from optical aggregation. It still emits no lens-copy verdict, calibrated edge distortion,
  checkerboard calibration, field-curvature, MTF50, artefacts, or plugin interpretation. _Done
  when:_ synthetic tests cover known straight-line bow, weak-reference inference, no-reference
  blockers, deterministic output, and stdout-empty failures; real fixture tests keep Bayer success
  and X-Trans/corrected rejection behaviour intact — met by this change.
- **Field-curvature inference** — `lenslab analyse <paths…>` emits skeleton schema
  `0.1-field-curvature` with top-level inferred aperture-lag evidence derived from measured
  centre/corner acutance across aperture groups. Unknown-correction inputs remain visible at frame
  level but excluded from inference, missing or weak evidence reports blockers, and focus-bracket
  measurement remains deferred to v0.2. It still emits no copy verdict, focus-shift quantity, MTF50,
  artefacts, target role, or plugin interpretation. _Done when:_ core tests cover supported,
  not-supported, blocked, exclusion, ambiguity, and numeric-error paths; CLI tests cover
  deterministic JSON, unknown-correction exclusion, stdout-empty failures, and real-fixture
  success/rejection behaviour — met by this change.
- **Target QA / keystone gate** — `lenslab analyse <paths…>` emits skeleton schema `0.1-target-qa`
  with frame-level target QA evidence from suitable periodic target geometry and group-level
  decentring target-quality gating derived from that evidence. Unknown-correction observations
  remain visible but cannot make group target quality pass, unsupported scene-like geometry reports
  machine-readable blockers, and the command still emits no centred/decentred copy verdict, frame
  role detection, MTF50, checkerboard calibration, artefacts, or plugin interpretation. _Done when:_
  schema, core, CLI, deterministic-output, stdout-empty failure, and real-fixture tests cover
  passed, gated, blocked, unknown-correction, and corrected-input rejection paths — met by this
  change.
- **Controlled vignetting reference + symmetry assessment** — `lenslab analyse <paths…>` emits
  schema `0.1-vignetting-control` with controlled same-lens/same-focal aperture-series optical
  deltas, reference-aperture labelling, product-readable symmetry status, and numeric residual
  evidence. Missing or unstable metadata, unknown corrections, centre-luminance drift, repeat
  scatter, mixed identity, and contradictory aperture trends block optical deltas. It still emits no
  lens-copy verdict, plugin interpretation, report artefacts, public threshold configuration, or
  calibrated absolute vignetting claim. _Done when:_ synthetic oracle tests cover exact deltas,
  fixed-bias cancellation, threshold blockers, deterministic JSON, and verdict omissions; decoded
  TIFF tests keep unknown-correction exclusion honest; `just test-local-vignetting` provides a
  local-only real-DNG gate that skips when unconfigured — met by this change.
- **Copy assessment support evidence** — `lenslab analyse <paths…>` emits schema
  `0.1-copy-assessment-support` with top-level `copy_assessment` evidence derived from target-QA
  gated left/right acutance asymmetry, correction provenance, aperture-series sufficiency, and
  field-curvature counterevidence. The CLI remains a gatekeeper, not the judge: it can support
  centred, support decentred, or report inconclusive with blockers and reshoot guidance, but it
  still emits no human copy verdict or plugin narrative. _Done when:_ core synthetic oracle tests
  cover centred/decentred/inconclusive/blocker paths, CLI tests cover the public JSON contract and
  verdict omissions, decoded TIFF tests keep unknown-correction exclusion honest, and
  `just test-local-copy-assessment` provides a local-only real-DNG product-realism gate that skips
  when unconfigured — met by this change.

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
- **`analyse` large-batch performance is unmeasured** — review surfaced plausible optimisation
  points: grouping currently scans accumulated groups, and acutance materialises full-patch diff
  buffers. These are not known bottlenecks for the explicit-file skeleton, so do not rewrite them
  speculatively. Revisit if a real batch, benchmark, or repeated review finding shows measurable
  time or memory cost. _Done when:_ a benchmark identifies a concrete bottleneck and the fix is
  measured, or confirms the simple implementation is adequate.
- **Grouping key tolerance is still exact-float** — `analyse` currently groups by exact
  `(lens_model, focal_length_mm, f_number)` equality. This is acceptable while those values come
  from the current decoded EXIF path, but sidecars, overrides, mixed backends, or derived metadata
  may produce semantically identical values with tiny representation differences. _Done when:_ the
  input model for directory/sidecar/override support defines canonical aperture and focal-length
  keys before broadening grouping semantics.
- **Schema contract module split** — `lenslab-core/src/schema.rs` now carries the public JSON
  contract for every analysed evidence family in one large module. Keep the contract behaviour
  unchanged, but split the schema by evidence area once the next schema expansion makes review
  locality worse than centralised reading. _Done when:_ serde output remains byte-stable for
  existing fixtures, schema versioning stays central, and new measurement families can add DTOs
  without editing an oversized catch-all module.
- **Mixed scene/target copy scoring** — option 3 from the copy-assessment planning remains
  intentionally deferred. Scene or mixed captures may become useful as soft evidence after hard
  target-series support exists, but they must not promote uncontrolled scene data to a hard copy
  decision. _Done when:_ the plugin/CLI boundary defines separate soft-evidence fields, preserves
  target-only hard support, and has false-positive tests for scene texture, framing tilt, lighting
  gradients, and field curvature.
- **Calibrated distortion model** — fit a lens distortion model only after target geometry and pose
  are measurable; depends on the Target QA / keystone gate producing reusable chart geometry
  evidence, so scene-only straight-line bow evidence is not mistaken for calibrated optical
  distortion. _Done when:_ checkerboard or equivalent controlled-target inputs produce a fitted
  distortion model with residuals and confidence gates, while uncontrolled scenes remain blocked or
  reported as non-calibrated evidence.
- **Controlled optical validation corpus** — add synthetic and controlled-target fixture sets with
  known MTF, vignetting, CA, distortion, keystone, and aperture-series behaviours, then assert
  measured values within documented tolerances, backed by golden JSON snapshots for schema
  evolution, so metric evidence is calibrated beyond schema and blocker correctness.
- **Manual-lens aperture input for controlled series** — old manual-focus lenses may not disclose
  aperture metadata at all, but a user-supplied controlled series should still be analysable when
  the capture order or sidecar data provides the aperture ladder. Do not keep missing EXIF aperture
  as a hard blocker once the product has an explicit way to accept user-confirmed aperture values.
  _Done when:_ guided capture or sidecar/override input can bind frames to aperture values, the JSON
  distinguishes user-supplied aperture from measured EXIF aperture, missing camera metadata is a
  warning rather than a blocker for those confirmed series, and tests cover manual-lens DNGs with no
  aperture tags.
- **CI workflow pins emit maintenance warnings** — GitHub Actions currently reports Node.js 20
  deprecation annotations for pinned actions such as `actions/checkout` and `mlugg/setup-zig`, and
  `taiki-e/install-action` falls back to `cargo-binstall` for `dprint@0.55.1` on the Ubuntu runner.
  These are not failing checks today, but they are release-engineering drift: review and update the
  pinned action SHAs/comments, confirm the replacements' runtime declarations, and decide whether to
  keep installing dprint through `taiki-e/install-action` or switch to a quieter pinned install
  path. _Done when:_ CI and release workflow runs complete without these maintenance annotations
  while preserving pinned third-party actions, read-only CI permissions, and the existing release
  approval boundary.
