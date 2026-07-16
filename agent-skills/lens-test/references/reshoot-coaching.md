# Reshoot Coaching

Use `copy_assessment.blockers` and `copy_assessment.reshoot` together. Blockers explain why support
is blocked; reshoot values identify the next capture action. A blocker need not have a matching
reshoot value.

## Priority Order

1. Target/control problem.
2. Correction provenance problem.
3. Texture or sample sufficiency problem.
4. Aperture-series problem.
5. Field-curvature disambiguation problem.
6. Ambiguous asymmetry problem.

## Reshoot Actions

| Reshoot value                               | User-facing action                                                                                  |
| ------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `capture_controlled_target_aperture_series` | Shoot a flat textured target at f/4, f/5.6, f/8, and f/11 without refocusing.                       |
| `improve_target_alignment`                  | Reshoot square-on; use a level or frame-aligned verticals so keystone cannot fake corner asymmetry. |
| `use_uncorrected_raw_input`                 | Use uncorrected DNG/TIFF input. Disable lens profiles and avoid corrected exports.                  |
| `add_textured_corner_coverage`              | Make sure all four corners land on detailed target texture.                                         |
| `add_repeat_frames`                         | Add repeat frames at the same aperture to separate copy behaviour from shake or random variation.   |
| `add_aperture_ladder`                       | Include the missing aperture ladder frames, normally f/4, f/5.6, f/8, and f/11.                     |
| `add_corner_focus_disambiguator`            | Add a corner-focused wide-open frame to separate field curvature from decentring.                   |

## Blocker Wording

| Blocker                               | Explanation                                                                                          |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `no_controlled_target_series`         | The capture is not a controlled target aperture series, so it cannot support a hard copy assessment. |
| `target_quality_not_passed`           | Target geometry is not trustworthy enough for a hard corner comparison.                              |
| `unknown_corrections`                 | Correction provenance is unknown, so optical evidence cannot be trusted for hard support.            |
| `low_texture`                         | The target does not give enough corner texture for reliable acutance comparison.                     |
| `insufficient_samples`                | There are too few usable frames to separate lens behaviour from noise or shake.                      |
| `missing_aperture`                    | Required aperture metadata or frames are missing.                                                    |
| `insufficient_aperture_series`        | The aperture ladder is too thin for hard support.                                                    |
| `ambiguous_field_curvature`           | Field curvature could explain the softness pattern.                                                  |
| `field_curvature_counterevidence`     | Field-curvature evidence argues against a simple decentring interpretation.                          |
| `inconsistent_asymmetry`              | The asymmetry is not stable enough across the aperture series.                                       |
| `asymmetry_below_decentred_threshold` | The asymmetry is below the decentred-support threshold.                                              |
| `asymmetry_above_centred_threshold`   | The asymmetry is too high for centred support.                                                       |

## Output Rule

Show the top blockers in order. Treat `copy_assessment.reshoot` as a strict allowlist for capture
changes, replacement input, and protocol advice. When the list is empty, explain the blockers and
make the entire next-step content: "The CLI prescribed no capture action." Do not suggest input,
protocol, or rerun actions. A generic request for next steps does not override the empty list.
Provide the full shooting guide only when the user explicitly asks for the general protocol, and
make clear that it is not advice derived from this run.
