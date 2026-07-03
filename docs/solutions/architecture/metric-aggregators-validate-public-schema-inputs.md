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
  - Treating excluded samples differently from included samples
tags: [metrics, schema, validation, aggregation, decentring]
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

## Why This Matters

Exclusions describe why a valid sample did not contribute to an aggregate. They are not validation
errors and should not hide invalid numeric data. If excluded samples bypass finite-value checks,
later changes can accidentally serialise invalid derived values or make failure behaviour depend on
gate ordering rather than data validity.

Keeping validation in the metric module also preserves crate boundaries: `lenslab-cli` orchestrates
measurement and grouping, while `lenslab-core` owns the rules that make derived evidence safe.

## When to Apply

- A metric accepts `schema` DTOs instead of private validated domain structs.
- Public DTO fields can be constructed outside the normal CLI path.
- A metric has include/exclude gates before computing derived values.
- A future schema adds optional/null evidence fields that could mask invalid input.

## Examples

The decentring aggregation review caught this exact failure mode: non-finite acutance was rejected
for included samples, but not for samples excluded as `low_texture` or `unknown_corrections`.

The fix moved finite acutance validation before the exclusion gates and added tests for:

- included non-finite samples;
- low-texture non-finite samples;
- aggregation-ineligible non-finite samples.

## Related

- [Machine-readable commands validate before stdout](machine-readable-commands-validate-before-stdout.md)
- [Decode boundaries preserve sensor data before analysis support exists](decode-boundary-preserves-sensor-data.md)
