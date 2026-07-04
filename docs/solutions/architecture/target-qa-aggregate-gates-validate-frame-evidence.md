---
title: Target QA aggregate gates validate frame evidence
date: 2026-07-04
category: architecture
module: lenslab-core::metrics
problem_type: architecture_pattern
component: metrics
severity: medium
applies_when:
  - Adding frame-level QA evidence that feeds group-level trust gates
  - Aggregating public schema DTOs with nullable measured evidence
  - Combining measurement blockers with correction-provenance blockers
  - Extending machine-readable analyse output with new trust states
tags: [metrics, target-qa, schema, aggregation, blockers, stdout-contract]
---

# Target QA aggregate gates validate frame evidence

## Context

Target QA added frame-level keystone evidence under `FrameMeasurement.qa.target` and group-level
decentring trust under `DecentringEvidence.target_quality`. That made target QA both a measured
frame observation and a trust gate for later copy verdict synthesis.

The frame evidence is a public schema DTO. Tests and future callers can construct it directly, so
the group aggregator cannot assume the CLI estimator produced a self-consistent shape.

## Guidance

Treat frame-level QA as evidence to validate, not as an already-trusted domain object.

For each assessed frame, validate the DTO before applying correction-provenance gates:

- `passed` and `gated` evidence must carry method, keystone measurement, and tilt axis;
- the top-level method and the measurement method must agree;
- confidence and keystone values must be finite, and confidence must stay in range;
- `passed` must not carry an over-threshold keystone value, and `gated` must not carry an
  under-threshold value;
- non-finite or inconsistent evidence is an error before serialisation, not a blocker.

Then apply provenance and geometry separately. Unknown-correction frames cannot make group target
quality pass, but their frame-level blocker still matters. If an unknown-correction frame is already
blocked for geometry, preserve both facts at group level:

```rust
if !frame.aggregation_eligible {
    push_blocker(&mut self.blockers, TargetQualityBlocker::UnknownCorrections);
}
for blocker in &target.blockers {
    push_blocker(&mut self.blockers, *blocker);
}
```

Do not double-count the same frame as multiple blocked samples. Count the frame once, while keeping
all distinct blocker reasons.

For candidate selection inside the estimator, ambiguity rules must include zero-valued estimates.
Two supported axes with equal zero keystone are still ambiguous: choosing one axis would present
arbitrary target-gated trust.

For CLI output, keep the full report assembled and serialised before the first stdout write:

```rust
let mut output = serde_json::to_vec_pretty(&report)?;
output.push(b'\n');
stdout.write_all(&output)?;
```

That keeps the machine-readable contract intact if a future DTO validation or serialisation path
fails.

## Why This Matters

Target QA sits between measurement and judgement. A frame-level observation may be useful for
inspection even when it cannot authorise group-level trust. Collapsing geometry blockers, correction
provenance, and DTO consistency into one status loses the reason the gate blocked.

The failure is subtle because every field can be individually serialisable while the combination is
false. For example, `status: passed` with a three-percent keystone value is valid JSON but invalid
evidence. If the aggregator accepts it, later verdict work may trust corner asymmetry that should
have been gated out.

## When to Apply

- A metric adds frame-local QA evidence and a group-level trust summary.
- Public schema DTOs are reused as aggregator inputs.
- Unknown-correction imagery may still carry useful observations for inspection.
- A blocker means "the frame did not support this measurement" rather than "the command failed".
- A command emits JSON that scripts or agents consume from stdout.

## Examples

The target-QA review found these regressions before merge:

- unknown-correction frames that were already blocked for geometry preserved only the geometry
  blocker, dropping `unknown_corrections`;
- equally supported horizontal and vertical references with zero keystone bypassed the
  `ambiguous_tilt_axis` blocker;
- a constructed `passed` DTO could carry an over-threshold keystone value unless the aggregator
  validated status and measurement consistency;
- direct `serde_json::to_writer_pretty(stdout, ...)` could leak partial JSON if serialisation ever
  failed after writing began.

The fix added focused tests for blocked unknown-correction aggregation, inconsistent assessed DTOs,
zero-keystone ambiguity, weak/too-few reference blockers, and process-level output stability.

## Related

- [Metric aggregators validate public schema inputs](metric-aggregators-validate-public-schema-inputs.md)
- [Machine-readable commands validate before stdout](machine-readable-commands-validate-before-stdout.md)
- [Profile correlation treats unusable overlaps as blockers](profile-correlation-overlap-blockers.md)
- [Straight-line metrics trace polarity and side bands separately](straight-line-metrics-trace-polarity-and-side-bands.md)
