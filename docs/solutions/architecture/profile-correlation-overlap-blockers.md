---
title: Profile correlation treats unusable overlaps as blockers
date: 2026-07-03
category: architecture
module: lenslab-core::metrics
problem_type: architecture_pattern
component: metrics
severity: medium
applies_when:
  - Measuring one-dimensional profile shifts with a bounded search window
  - Turning weak image evidence into frame-level blockers
  - Keeping machine-readable analysis commands from failing on unmeasurable patches
tags: [metrics, ca, correlation, blockers, stdout-contract]
---

# Profile correlation treats unusable overlaps as blockers

## Context

Lateral CA measurement estimates red/blue channel displacement by correlating row and column
profiles across a bounded shift window. Some shifted overlaps can be flat or too low-energy even
when the full corner profile has enough texture to attempt measurement.

## Guidance

Profile-level correlation should skip unusable overlap windows and keep searching. If no candidate
window produces a valid peak, return a typed evidence blocker such as `flat_profile`,
`profile_too_short`, or `correlation_peak_not_found`.

Keep truly invalid sample data as an error. `NaN` or infinity in decoded channel samples is an input
invariant violation; a low-energy overlap is only weak evidence.

## Why This Matters

`lenslab analyse` must write either complete valid JSON or no stdout. A low-texture corner patch is
ordinary measurement evidence and should appear in JSON as a blocker. If one bad correlation window
returns a metric error, the whole command fails and loses otherwise valid frame evidence.

This distinction preserves the measured/inferred split: blockers say the scene did not support the
measurement; errors say the data or implementation violated an invariant.

## When to Apply

- A metric scans multiple candidate windows, offsets, or model fits.
- Some candidates can be unusable while others are still valid.
- The caller can honestly serialise "unmeasurable" evidence for a zone or frame.
- Non-finite decoded samples must still stop before serialisation.

## Examples

In the lateral CA metric, a shifted overlap with near-zero profile energy is not a command failure:

```rust
if left_energy <= MIN_PROFILE_VARIANCE || right_energy <= MIN_PROFILE_VARIANCE {
    return Ok(None);
}
```

The caller skips that shift and only emits `correlation_peak_not_found` when no usable candidate
remains. A separate regression keeps `NaN` samples on the error path.

## Related

- [Machine-readable commands validate before stdout](machine-readable-commands-validate-before-stdout.md)
- [Metric aggregators validate public schema inputs](metric-aggregators-validate-public-schema-inputs.md)
