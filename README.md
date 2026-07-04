# lenslab

Measure a lens, judge a copy. `lenslab` characterises a camera lens and flags a decentred/tilted
sample from a folder of DNG/TIFF frames — sharpness vs aperture, decentring, vignetting, lateral CA,
distortion, field curvature — on uncorrected linear sensor data, with an honest split between
_measured_ and _inferred_.

A Rust CLI for deterministic measurement (canonical JSON output) and a Claude plugin that coaches
the test shots and turns the numbers into a verdict.

> **Status:** The Rust workspace now has `inspect`, `contact`, the image model/zones, real DNG
> fixtures, and an explicit-file `analyse` command that emits deterministic evidence for
> acutance/contrast, decentring, vignetting, lateral CA, distortion, field curvature, and target QA.
> Full copy verdicts, guided capture, MTF50, and report artefacts remain future work. Start at
> [`GENESIS.md`](docs/GENESIS.md).

## Why

Buying a lens, especially used or a notoriously sample-variable design, you want to know whether
_this_ copy is optically sound before the return window closes. Corner softness alone tells you
little — it is usually inherent. `lenslab` isolates the signal that actually indicates a bad copy
(asymmetric/decentred behaviour) from the traits every copy shares (field curvature, vignetting,
distortion).

## Components

- **`lenslab` CLI** (Rust, single static binary) — current commands are `inspect`, `contact`, and an
  explicit-file `analyse` skeleton using schema `0.1-target-qa`. It reports measured and inferred
  evidence, blockers, and exclusions, but deliberately emits no centred/decentred verdict yet.
  MTF50, directory input, report artefacts, and the exit-code taxonomy remain target work.
- **Claude plugin** — orchestrates the binary, coaches square-on target shots and the aperture
  ladder, interprets the JSON into a keep/return brief.

## Licence

Core (`lenslab-core`, `lenslab-cli`) is dual MIT/Apache-2.0. The distributed binary statically links
[`rawler`](https://crates.io/crates/rawler) (LGPL-2.1) for raw decoding; the combined binary
therefore carries LGPL-2.1 obligations, met by this repository being fully open. See
[`NOTICE`](NOTICE) and [`docs/DECISIONS.md`](docs/DECISIONS.md).

## Docs

[Genesis / handover](docs/GENESIS.md) · [Roadmap](docs/ROADMAP.md) · [Spec](docs/SPEC.md) ·
[Algorithms](docs/ALGORITHMS.md) · [Decisions](docs/DECISIONS.md) ·
[Plugin & skill](docs/SKILL_PLUGIN.md)

## Development

Run `just ci` for the normal offline gate. Run `just test-fixtures` when touching decode, fixture,
or CI code; it downloads the real-camera DNG fixtures from checksum-pinned GitHub Release assets and
runs the `RawlerDecoder` fixture tests.
