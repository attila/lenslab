---
title: Straight-line metrics trace polarity and side bands separately
date: 2026-07-03
category: architecture
module: lenslab-core::metrics
problem_type: architecture_pattern
component: metrics
severity: medium
applies_when:
  - Measuring straight reference lines from image intensity profiles
  - Selecting frame-level metric candidates from multiple possible references
  - Distinguishing measured edge evidence from weak inferred geometry
tags: [metrics, distortion, tracing, blockers, measured-evidence]
---

# Straight-line metrics trace polarity and side bands separately

## Context

Distortion evidence measures bow from straight reference lines. The first implementation traced one
dark candidate per orientation across the full frame, then fitted the best horizontal or vertical
curve.

That was enough for simple synthetic dark-line fixtures, but it missed common reference shapes:
bright lines on dark backgrounds, and frames with separate top/bottom or left/right references.

## Guidance

Straight-reference metrics should trace candidate families, not a single full-frame profile:

- trace both dark-on-light and light-on-dark polarity;
- trace near-side bands separately from the full frame;
- rank measured-eligible candidates ahead of weak inferred candidates;
- reject broad support that looks like background fill rather than a line;
- keep poor or absent references as blockers, not numeric zero evidence.

The support check is deliberately about reference geometry, not pixel validity. Non-finite samples
remain errors; broad or discontinuous traces become blockers such as `fit_residual_too_high`,
`line_discontinuous`, or `profile_too_short`.

## Why This Matters

A full-frame weighted centroid can collapse two side references into a centre line. That downgrades
measured side evidence into weak inferred geometry, or worse, reports a plausible bow for the wrong
reference.

Polarity matters for the same reason. A metric that only searches for dark references silently
misses bright chart edges, while a naive bright search can treat the bright background around a dark
line as the reference. Tracing polarity and side bands together keeps the measured/inferred split
honest.

## When to Apply

Apply this to metrics that fit lines or curves from scene/chart intensity profiles:

- distortion straight-line bow;
- future target keystone or chart-edge QA;
- field-curvature or focus-lag helpers if they infer reference geometry from edges;
- any metric that chooses one candidate from multiple traceable profiles.

## Examples

Regression coverage should include:

- dark and bright reference lines;
- paired side references that must not collapse to the centre;
- minimum-dimension blockers distinct from weak short-span references;
- noisy traces where broad background support must not become a candidate.

Release-level binary UAT is still useful after these unit fixtures. In the distortion work, the
release Bayer fixture exposed that stricter line-support filtering changed real-frame distortion
from a candidate to blockers, which is acceptable only because the JSON still reports the evidence
honestly and the command contract remains intact.

## Related

- [Machine-readable commands validate before stdout](machine-readable-commands-validate-before-stdout.md)
- [Profile correlation treats unusable overlaps as blockers](profile-correlation-overlap-blockers.md)
