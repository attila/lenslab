---
title: Metric aggregators validate public schema inputs
date: 2026-07-03
category: architecture
module: lenslab-core::metrics
problem_type: architecture_pattern
component: tooling
severity: medium
applies_when:
  - Adding core metrics that consume public schema DTOs
  - Deriving aggregate evidence from frame-level measurements
  - Deriving support or verdict-adjacent evidence from lower-level metrics
  - Treating excluded samples differently from included samples
  - Preserving frame-level blockers in group-level evidence
  - Summarising aperture, focus, or repeat series before making support decisions
tags: [metrics, schema, validation, aggregation, decentring, ca, blockers, copy-assessment]
---

# Metric aggregators validate public schema inputs

## Context

`lenslab-core::schema` owns serialisable DTOs for the public JSON contract. Those DTOs are also
convenient inputs for small pure metric aggregators, such as decentring evidence over existing
`FrameMeasurement` values.

That convenience does not make schema values trusted domain objects. Their fields are public so
tests, CLI wiring, and future consumers can construct them directly.

## Guidance

Metric aggregators that accept schema DTOs must validate the numeric invariants they depend on at
the metric boundary, even when a sample is later excluded from the aggregate.

For decentring aggregation, validate both pair acutance values before deciding whether the pair is
excluded for unknown corrections or low texture:

```rust
let left_acutance = finite_acutance(left)?;
let right_acutance = finite_acutance(right)?;

if !aggregation_eligible {
    self.unknown_corrections += 1;
    return Ok(());
}
```

Tests should cover invalid values on included and excluded paths. A low-texture or
aggregation-ineligible sample must not become a way to smuggle `NaN` or infinity through derived
evidence.

Keep blocker evidence and sample exclusion counts as separate concerns. An ineligible frame may need
an `unknown_corrections` exclusion while still carrying a structural blocker such as
`unsupported_colour_channels` into the group summary. Preserve the blocker so consumers know why a
measurement is unavailable, but do not count one frame as two excluded samples.

Support or verdict-adjacent aggregators must also propagate blockers from prerequisite evidence.
Checking only that a candidate value exists is not enough. If a pair summary carries
`reliability_blockers`, or a countercheck such as field-curvature inference is `blocked`, the
support layer should usually remain inconclusive and carry the blocker forward. Otherwise a later
layer can turn untrusted evidence into hard support just because the lower-level metric still
emitted a numeric mean for inspection.

For cross-series decisions, do not average away contradiction before applying semantic gates.
Compute the aggregate numbers, but also inspect the per-condition candidates that produced them. A
real decentring signal should keep the same side/corner relationship across apertures; if aperture
groups flip sign, the result is evidence against a fixed optical fault even when the average remains
large.

## Why This Matters

Exclusions describe why a valid sample did not contribute to an aggregate. They are not validation
errors and should not hide invalid numeric data. If excluded samples bypass finite-value checks,
later changes can accidentally serialise invalid derived values or make failure behaviour depend on
gate ordering rather than data validity.

Keeping validation in the metric module also preserves crate boundaries: `lenslab-cli` orchestrates
measurement and grouping, while `lenslab-core` owns the rules that make derived evidence safe.

Cross-group aggregators that partition by lens, focal length, aperture, or similar public DTO fields
should preserve deterministic first-seen output order without falling back to repeated linear
partition scans. Keep a `Vec` for serialisation order when that matters, but use an index map for
lookup once the metric can see many groups. Key floats only after finite-value validation, using a
stable representation such as `to_bits()` for lookup.

## When to Apply

- A metric accepts `schema` DTOs instead of private validated domain structs.
- Public DTO fields can be constructed outside the normal CLI path.
- A metric has include/exclude gates before computing derived values.
- A future schema adds optional/null evidence fields that could mask invalid input.
- A metric partitions multiple report groups before deriving cross-aperture or cross-series
  evidence.
- A support layer consumes lower-level summaries with their own reliability blockers.
- A series metric could hide sign flips, trend reversals, or other per-condition contradictions in a
  mean value.

## Examples

The decentring aggregation review caught this exact failure mode: non-finite acutance was rejected
for included samples, but not for samples excluded as `low_texture` or `unknown_corrections`.

The fix moved finite acutance validation before the exclusion gates and added tests for:

- included non-finite samples;
- low-texture non-finite samples;
- aggregation-ineligible non-finite samples.

The lateral CA aggregation review added the sibling evidence-preservation case. A true Gray TIFF has
no colour channels for CA measurement, so frame-level evidence carries
`unsupported_colour_channels`. TIFF correction status is still unknown, so the same frame is also
excluded from aggregation as `unknown_corrections`. The group summary must retain
`unsupported_colour_channels` as a blocker without adding a second exclusion count for the same
frame.

The field-curvature inference review added the cross-group partitioning case. The first
implementation preserved first-seen summary order by scanning a `Vec<Partition>` for each group,
which made the partitioning phase quadratic when every group belonged to a distinct lens/focal
identity. The fix kept the `Vec` for deterministic output order and added a `HashMap` from validated
partition identity to vector index.

The copy-assessment support review added the support-layer case. The first implementation could emit
hard support from a one-sample-per-aperture target ladder because it saw a pair mean and ignored
`ReliabilityBlocker::InsufficientSamples`. It could also turn aperture sign flips into hard
`supports_decentred` by averaging before checking consistency, and it treated blocked
field-curvature counterchecks as passed unless the blocker was ambiguous peak. The fix propagated
pair reliability blockers and field-curvature blockers into copy-assessment blockers, and required
per-aperture sign consistency before hard decentred support.

## Related

- [Machine-readable commands validate before stdout](machine-readable-commands-validate-before-stdout.md)
- [Decode boundaries preserve sensor data before analysis support exists](decode-boundary-preserves-sensor-data.md)
