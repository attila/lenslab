# lenslab — Specification (v0.1)

Scope, architecture, CLI surface, JSON contract. Measurement maths is in `ALGORITHMS.md`; this
document is the _shape_ of the tool.

## 1. Goals / non-goals

**Goals**

- From a folder of DNG/TIFF (+EXIF), characterise a lens and judge a specific copy.
- Measure on _uncorrected, linear_ sensor data; refuse or loudly warn when corrections are baked in.
- Decentring (copy verdict) is a first-class output with explicit confidence and evidence.
- Auto-group frames by lens + focal length + aperture from EXIF.
- Canonical, versioned JSON as the machine contract; visual artifacts (contact sheet, corner crops,
  curves) as humans-facing output.
- Deterministic: identical inputs produce byte-stable JSON.
- Honest provenance: every measurement tagged `measured` vs `inferred`, with confidence.

**Non-goals**

- Not a raw developer/converter, not a DAM/cataloguer, no GUI.
- No aesthetic scoring (bokeh, rendering), no online leaderboard.
- v0.1 does not do longitudinal CA (LoCA) or flare quantification.

## 2. Inputs

- **DNG** (primary), any camera `rawler` supports; **TIFF** with EXIF (treated as already-demosaiced
  RGB).
- A directory (recursed) or an explicit file list.
- Frame _role_: `target` (flat fronto-parallel test chart/wall) or `scene` (real-world). Detected
  heuristically (`--auto`, default) or forced (`--target`/`--scene`) or per-file via filename tag /
  sidecar.
- Lens identity: from EXIF where present; `rawler` camera id otherwise; `--lens "<id>"` override
  (Pentax and others leave EXIF `LensModel` empty — lens lives in MakerNotes).

## 3. Architecture

Cargo workspace, three crates plus plugin:

### `lenslab-core` (lib, MIT/Apache-2.0)

Pure measurement. No report rendering, no process spawning. Modules:

- `image` — `LinearImage` (f32, scene-linear, black/white normalised), `CfaImage` (mosaic +
  `CfaPattern`), planes (R/G1/G2/B), metadata struct.
- `demosaic` — single-green-plane extraction (default for sharpness); bilinear RGB for crops/CA.
  Pluggable.
- `zones` — zone geometry (5-point default; `3x3`, `9x9` grids), patch sizing, equal-radius corner
  placement.
- `qa` — squareness/keystone estimate, global defocus/shake check, exposure clipping check. Produces
  per-frame `FrameQa` and gating decisions.
- `group` — group frames by `(lens, focal_mm, f_number)`; classify role.
- `metrics::{acutance, mtf, decentre, vignette, ca, distortion, fieldcurv}` — each consumes
  images+zones, emits `Measurement` values with provenance/confidence.
- `synth` — derive per-copy verdict, optimum aperture, corner-lag summary from the measurement set.
- `schema` — serde types for the JSON contract (the public API; versioned).

### `lenslab-decode` (lib) — the LGPL boundary

Defines `trait Decoder { fn decode(&self, path) -> Result<DecodedFrame> }` returning CFA-or-RGB
linear data + raw metadata + a `corrections_present` flag. Impls: `RawlerDecoder` (LGPL-2.1),
`TiffDecoder` (permissive). Keep all LGPL-linked code here so it can be swapped wholesale (see
DECISIONS option (c)).

### `lenslab-cli` (bin, MIT/Apache-2.0)

`clap` parsing, orchestration, report rendering (JSON via serde; Markdown brief; PNG/SVG artifacts
via `plotters` + `image`). Owns process exit codes.

### `plugin/` — Claude plugin

`.claude-plugin/plugin.json` + `skills/lens-test/`. Orchestration and narrative only; shells out to
the binary. See `SKILL_PLUGIN.md`.

## 4. Pipeline

```
ingest → frame-role → qa-gate → measure(per group) → synthesise → report
```

Each stage is a trait so it is independently unit-testable. `qa-gate` may exclude target frames that
fail squareness/shake thresholds (reported, not silently dropped).

## 5. CLI surface

Current implementation note: the shipped `analyse` command is a narrow explicit-file skeleton:
`lenslab analyse <paths…>` emits pretty JSON using schema `0.1-copy-assessment-support`. It includes
acutance/contrast zones, correction provenance, vignetting falloff evidence, lateral CA evidence,
straight-line distortion evidence, inferred field-curvature evidence, frame-level target QA keystone
evidence, group-level left/right decentring evidence gated by target quality, and top-level
`copy_assessment` support evidence with blockers and reshoot guidance. It still emits no human copy
verdict. Directory recursion, `--format`, frame-role detection, config files, CLI keystone threshold
flags, artefact generation, and the exit-code taxonomy below are target contract, not current
behaviour.

```
lenslab analyse <paths…>
    [--auto | --target | --scene]      # frame role (default --auto)
    [--lens "<id>"]                    # override lens identity
    [--grid 5pt|3x3|9x9]               # zone layout (default 5pt)
    [--metric auto|mtf50|acutance]     # default auto: mtf50 on targets, acutance on scenes
    [--gate-keystone <pct>]            # reject target frames over this tilt (default 1.5)
    [--channel green|luma]             # measurement channel (default green)
    [--out <dir>] [--format json,md,html]   # default json,md
    [--ref-aperture <f>]               # vignetting aperture-difference reference (default most-stopped)

lenslab decentre <paths…>             # focused copy verdict; exit non-zero if decentred
lenslab vignette <paths…>
lenslab mtf <edge-shot> --roi X,Y,W,H # single slanted-edge MTF50 readout
lenslab contact  <paths…> --out <file> # contact sheet PNG
lenslab inspect  <file>               # EXIF + decode info + corrections-present (no measurement)
```

- Config file `lenslab.toml` (zone geometry, thresholds, camera-pitch overrides) merged under CLI
  flags.
- **Exit codes**: `0` ok; `2` decentred/failed copy gate (for `decentre`/`analyse --gate`); `3`
  insufficient/invalid input (e.g. corrections baked in, no valid target frames); `1` internal
  error.

## 6. JSON contract (canonical output)

Current implementation note: the skeleton schema is `"0.1-copy-assessment-support"`, not the full
`"1.0"` shape below. It includes evidence families that the Rust code can populate honestly:
acutance/contrast, vignetting falloff and controlled-series deltas, lateral CA, straight-line
distortion, inferred field curvature, target QA, left/right decentring aggregation with blockers and
exclusions, and top-level copy support evidence. It omits `generated_utc`, human verdicts, MTF50,
report artefacts, folder input metadata, and the final `1.0` summary shape until those values can be
populated honestly.

Versioned (`schema_version`), stable, documented. Shape:

```json
{
  "schema_version": "1.0",
  "tool_version": "0.1.0",
  "generated_utc": "…",
  "inputs": [{ "path": "...", "sha256": "...", "role": "target|scene" }],
  "lens": { "id": "smc D FA 645 25mm F4 AL[IF] SDM AW", "source": "exif|rawler|override" },
  "body": { "model": "PENTAX 645D", "pixel_pitch_um": 6.05, "pitch_source": "derived|db|unknown" },
  "groups": [
    {
      "f_number": 8.0,
      "focal_mm": 25.0,
      "frames": ["_IGP1691"],
      "qa": { "keystone_pct": 3.0, "tilt_axis": "vertical", "gated": false },
      "measurements": {
        "sharpness": {
          "zones": {
            "C": { "value": 1.58, "unit": "acutance", "method": "measured", "confidence": 0.9 },
            "TL": { "value": 1.31, "unit": "acutance", "method": "measured", "confidence": 0.9 },
            "...": {}
          },
          "mtf50": { "C": { "value": 0.31, "unit": "cy/px", "method": "measured" } } // when target+edge
        },
        "vignetting": {
          "corner_mean_stops": -0.33,
          "optical_stops": -0.0,
          "symmetry": "lighting-biased"
        },
        "ca_lateral": { "corner_max_px": 2.2, "unit": "px@fullres" },
        "distortion": {
          "central_bow_pct": 0.1,
          "method": "inferred",
          "note": "lines do not reach corners"
        }
      }
    }
  ],
  "verdict": {
    "copy": "centred|decentred|inconclusive",
    "confidence": 0.85,
    "evidence": [
      "LR corner asymmetry mean +0.006 over 27 frames",
      "diagonal asymmetry flips sign with aperture",
      "no directional corner smear"
    ],
    "optimum_aperture": "f/8–f/11",
    "corner_lag_at_optimum": "≈16–21% behind centre",
    "caveats": ["handheld/eye-levelled target, ~3% keystone"]
  },
  "artifacts": ["contact.png", "crops_f8.png", "vignette_curve.svg"]
}
```

Every numeric leaf is a
`Measurement { value, unit, method: measured|inferred, confidence, provenance? }`.

## 7. Output artifacts

- `contact.png` — labelled contact sheet (frame, f-number).
- `crops_<ap>.png` — 5-zone 100% crop montage (centre + 4 corners), independently normalised, the
  decentring evidence.
- `vignette_curve.svg`, `mtf_by_aperture.svg` — curves (`plotters`).
- `report.md` (+ `report.html` in v0.2) — the human brief generated from JSON.

## 8. Testing

- **Synthetic fixtures** (`fixtures/`, Rust): generate CFA/RGB with a _known_ injected slanted edge
  (known MTF), radial vignette (known stops), and barrel distortion (known %). Assert measured
  values within tolerance. This is what validates the maths independent of any real camera.
- **Golden integration**: a tiny committed sample set (a few small frames) with checked-in expected
  JSON (tolerance-compared).
- **CI**: `cargo test`, `clippy -D warnings`, `fmt --check`; cross-build static binaries
  (macOS/Linux/Windows) via `cargo-dist`.

## 9. Distribution

- `cargo install lenslab` + prebuilt release binaries.
- Plugin shipped from `plugin/` (Claude plugin; portable to other agents later).
