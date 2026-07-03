# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Added the `lenslab inspect` command for DNG and TIFF inputs, emitting deterministic JSON metadata
  including camera/lens fields, dimensions, CFA details, exposure fields, and correction provenance.
- Added decoded image modelling and five-zone geometry for measurement on linear Bayer green,
  RGB-derived luma, and luma input planes.
- Added `lenslab contact <paths…> --out <file>` for deterministic labelled PNG contact sheets from
  DNG/TIFF inputs.
- Added `lenslab analyse <paths…>` with measured acutance and contrast evidence across the default
  centre/corner zones using skeleton schema `0.1-acutance`.
- Added decentring aggregation evidence and the first target-QA gate, emitting left/right
  corner-pair evidence without promoting ungated scene evidence to a lens-copy verdict.
- Added measured vignetting falloff evidence with unknown-correction exclusions and blocked
  aperture-difference machinery for uncontrolled input sets.
- Added lateral chromatic aberration evidence in px@fullres, including per-frame red/blue shifts,
  per-corner group summaries, blockers, and unknown-correction exclusions using skeleton schema
  `0.1-ca`.
- Added checksum-pinned real-camera DNG fixtures and fixture tests for Bayer and X-Trans decode
  behaviour.
- Added CI and release workflows covering formatting, clippy, tests, dependency checks, docs,
  fixture tests, four-target cross-compilation, and owner-approved GitHub Releases.
