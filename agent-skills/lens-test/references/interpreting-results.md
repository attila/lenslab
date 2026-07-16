# Interpreting Results

Use `copy_assessment` as the support boundary. Do not derive a harder claim from lower-level
measurements than the CLI already supports.

## Schema

This skill targets:

```json
"schema_version": "0.1-copy-assessment-support"
```

Required support fields:

- `copy_assessment.state`
- `copy_assessment.hard_support_eligible`
- `copy_assessment.evidence`
- `copy_assessment.blockers`
- `copy_assessment.reshoot`

If the schema version differs, stop and say the binary/skill versions do not match.

## State Language

`supports_centred`

- Say the evidence supports a centred copy.
- Mention passed target quality, correction provenance, aperture-series, left/right consistency, and
  field-curvature counterevidence when those gates are passed.
- Do not say the copy is lab-certified or perfect.
- Do not give keep/return advice.

`supports_decentred`

- Say the evidence supports a decentred copy.
- Mention aperture-consistent left/right asymmetry and the field-curvature counterevidence gate.
- Keep this as support evidence, not a consumer-law or return recommendation.

`inconclusive`

- Say the run is inconclusive.
- Explain the blocker codes in user terms.
- Do not say the lens is centred or decentred.
- Treat `copy_assessment.reshoot` as a strict allowlist for capture changes, replacement input, and
  protocol advice.
- When the list is empty, say that the CLI prescribed no capture action. Do not infer any next step
  from blockers or lower-level evidence, even when the user asks what to do next.

## Measured vs Inferred

Measured examples:

- acutance and contrast values
- target QA keystone evidence
- luminance falloff
- lateral CA shifts

Inferred/support examples:

- `copy_assessment.state`
- field-curvature aperture-lag interpretation
- blockers and reshoot categories

Phrase inferred/support claims as support, not direct measurement.

## Non-Claims

Never add these from this skill:

- keep/return advice
- price/value judgement
- MTF50 claim
- report artefact claim
- scene-only hard copy verdict
- hard centred/decentred claim when `copy_assessment.state` is `inconclusive`
