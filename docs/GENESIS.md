# lenslab — Genesis / Handover

> Single entry point for a Claude Code (or human) session picking this project up cold. Read this
> top to bottom, then `docs/SPEC.md` → `docs/ALGORITHMS.md` → `docs/DECISIONS.md` →
> `docs/SKILL_PLUGIN.md`. The Rust workspace now contains `inspect`, `contact`, image/zones, real
> fixtures, and an `analyse` acutance/contrast skeleton. Full verdict analysis remains future work.
> The validated Python/C prototype under `reference/prototype/` remains the algorithm reference.

## TL;DR

`lenslab` is an open-source **Rust CLI plus a Claude plugin** that characterises a camera lens and
judges a specific copy from a folder of DNG/TIFF frames. It measures, on uncorrected linear data:
sharpness vs aperture, decentring (the copy verdict), vignetting, lateral CA, distortion, and field
curvature. The CLI does deterministic measurement and emits canonical JSON; the Claude plugin
orchestrates the binary, coaches the test shots, and turns the JSON into a human verdict.

The whole approach was proven end-to-end in a prior session by evaluating a real lens (smc Pentax-D
FA 645 25mm f/4 on a Pentax 645D). That prototype — a from-scratch DNG lossless-JPEG decoder and the
full measurement battery — lives in `reference/prototype/` and is the source of truth for the
algorithms. See `docs/ALGORITHMS.md` for the validated methods, thresholds, and the actual numbers
that came out.

## Origin (why this exists)

A photographer needed a keep-or-return decision on a newly bought ultra-wide, inside a return
window: is this specific copy optically sound (centred), or decentred/tilted? Answering it required
decoding 645D raws with no raw library available, then measuring corner-to-corner symmetry,
sharpness across the aperture range, vignetting, CA and distortion — separating inherent lens
character from a bad sample. That one-off worked. This project generalises it into a reusable tool
for any lens and any DNG/TIFF with EXIF.

The verdict logic that matters most (decentring) is subtle and is fully documented in
`docs/ALGORITHMS.md §Decentring`. Do not reduce it to "corners are soft" — corner softness is
normal; **asymmetry** is the signal, and even that must be disambiguated from framing tilt and field
curvature.

## Locked decisions

See `docs/DECISIONS.md` for rationale. Summary:

- **Language: Rust.** Single statically-linked binary, no runtime package ecosystem.
- **Decode: `rawler`** (LGPL-2.1, 300+ cameras incl. X-Trans/CR3/JXL-DNG) behind a `Decoder` trait,
  so the backend is swappable. TIFF via the `tiff`/`image` crates.
- **Licence strategy: option (a).** Core crates `lenslab-core`/`lenslab-cli` are dual
  **MIT/Apache-2.0**; the distributed binary statically links LGPL-2.1 `rawler`, so the _combined
  binary_ carries LGPL-2.1 obligations, satisfied by the repo being fully open (recipients can
  rebuild against a modified `rawler`). Keep a `NOTICE` documenting this. This keeps the single
  static binary.
- **Name: `lenslab`.**
- **Claude plugin first** (then portable to other agents later).
- **v0.1 = the full measurement battery**: ingest/normalise (DNG+TIFF), `inspect`, `contact`,
  sharpness (MTF50 + acutance), decentring, vignetting, **CA, distortion, field-curvature**.
  **v0.2** = HTML report, focus-bracket support, additional backends.

## What you are building (v0.1)

A Cargo workspace (`lenslab-core`, `lenslab-decode`, `lenslab-cli`) plus a `plugin/` Claude plugin.
Full surface in `docs/SPEC.md`. The measurement maths is already specified and validated in
`docs/ALGORITHMS.md` — port it, don't reinvent it.

Design principle, non-negotiable: **deterministic measurement in Rust; judgement, coaching and
narrative in the plugin; versioned JSON is the boundary.** The plugin never re-measures.

## Repo map

```
lenslab/
  Cargo.toml                 # workspace
  rust-toolchain.toml
  LICENSE-MIT  LICENSE-APACHE  NOTICE
  README.md
  docs/        SPEC.md  ALGORITHMS.md  DECISIONS.md  SKILL_PLUGIN.md
  lenslab-core/              # lib: image model, channels, zones, metrics, schema
  lenslab-decode/            # lib: Decoder trait + rawler/tiff impls (LGPL boundary lives here)
  lenslab-cli/               # bin: clap, orchestration, report rendering
  plugin/                    # target Claude plugin; not implemented yet
    .claude-plugin/plugin.json
    skills/lens-test/SKILL.md
    skills/lens-test/references/{shooting-guide.md,interpreting-results.md}
  tests/                     # real DNG fixture metadata and downloaded fixture location
  reference/
    prototype/               # THIS SESSION'S WORKING CODE — port from here
    sample_outputs/          # example artifacts (corner crops, wall ladder)
  scripts/                   # fixture fetch and release helpers
```

## Start here (suggested order for v0.1)

1. Workspace skeleton + `cargo build` green with `rawler` wired behind the `Decoder` trait.
   Implement `lenslab inspect` first — EXIF dump + decode info + corrections-present flag. Smallest
   end-to-end slice that proves decode+metadata.
2. Image model + single-green-plane extraction + zone geometry
   (`docs/ALGORITHMS.md §Channel, §Zones`).
3. `contact` (contact sheet) — cheap, validates demosaic + I/O.
4. Acutance metric + `analyse` skeleton emitting JSON. **Current implementation reaches this
   acutance/contrast skeleton only; no verdict is emitted.**
5. Decentring aggregate + QA keystone gate.
6. Vignetting (aperture-difference method). CA. Distortion. Field-curvature inference.
7. Slanted-edge MTF50 for target shots (heaviest; acutance is the fallback and already works).
8. Synthetic fixtures with injected MTF/vignette/distortion → assert measured values within
   tolerance. **Build these early enough to validate steps 4–6.**
9. The plugin: `SKILL.md` orchestrating the built binary; port the shooting guide and interpretation
   guide from `docs/`.

## Environment caveats (important)

- The origin session ran in a sandbox with **no `rawpy`/`exiftool` and no network**, which is why a
  bespoke C lossless-JPEG decoder exists in `reference/prototype/ljpeg.c`. You are presumably on a
  normal machine: **use `rawler` via cargo**; the C decoder is reference only (useful if anyone
  later wants a permissive DNG-only fallback backend).
- `rawler` is LGPL-2.1. Keep it confined to `lenslab-decode`. Document in `NOTICE`.
- Pixel pitch (needed for MTF50 in lp/mm) is not always in EXIF; derive from sensor mm ÷ pixel count
  or a small camera DB, and report cy/px when pitch is unknown. See `docs/ALGORITHMS.md §MTF50`.

## Validated worked example (regression target)

The DA 645 25mm copy was judged **centred and sound**. Headline numbers (acutance is a relative
metric, see ALGORITHMS): centre sharpness peaks f/5.6–f/11, corners peak f/11 at ~80–84% of centre;
optical vignetting ~0.7 stop at f/4, gone by f/8; lateral CA ~1–2 px at extreme corners at f/8;
left/right corner symmetry mean +0.006 across 27 real frames. Use these as a sanity check once
`rawler` can read the same files. Full table in `docs/ALGORITHMS.md §Worked example`.
