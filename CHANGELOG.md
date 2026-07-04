# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `lenslab inspect` for DNG and TIFF inputs, emitting deterministic JSON metadata including
  camera/lens fields, dimensions, CFA details, exposure fields, and correction provenance.
- Decoded image modelling and five-zone geometry for measurement on linear Bayer green, RGB-derived
  luma, and luma input planes.
- `lenslab contact <paths…> --out <file>` for deterministic labelled PNG contact sheets from
  DNG/TIFF inputs.
- `lenslab analyse <paths…>` with measured acutance and contrast evidence across the default
  centre/corner zones using skeleton schema `0.1-acutance`.
- Decentring aggregation evidence and the first target-QA gate, emitting left/right corner-pair
  evidence without promoting ungated scene evidence to a lens-copy verdict.
- Measured vignetting falloff evidence with unknown-correction exclusions and blocked
  aperture-difference machinery for uncontrolled input sets.
- Lateral chromatic aberration evidence in px@fullres, including per-frame red/blue shifts,
  per-corner group summaries, blockers, and unknown-correction exclusions using skeleton schema
  `0.1-ca`.
- Distortion evidence with frame-level straight-line bow candidates, measured/inferred method codes,
  blockers for unsupported reference geometry, and group summaries using skeleton schema
  `0.1-distortion`.
- Field-curvature inference evidence from aperture-dependent centre/corner acutance behaviour, with
  blockers and exclusions using skeleton schema `0.1-field-curvature`.
- Target QA keystone evidence and aggregate gating, including per-frame periodic reference quality,
  blocker propagation, left/right axis ambiguity handling, and schema `0.1-target-qa`.
- Controlled vignetting aperture-series evidence using schema `0.1-vignetting-control`, including
  reference-relative optical deltas, symmetry residuals, conservative blockers, and a local-only
  real-DNG validation gate.
- Checksum-pinned real-camera DNG fixtures and fixture tests for Bayer and X-Trans decode behaviour.
- CI and release workflows covering formatting, clippy, tests, dependency checks, docs, fixture
  tests, four-target cross-compilation, and owner-approved GitHub Releases.
