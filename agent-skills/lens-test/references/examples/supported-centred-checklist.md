# Supported Centred Checklist

Source: `supported-centred.json`.

## Required Facts

- Lead with centred support from `copy_assessment.state = "supports_centred"`.
- Say hard support is eligible from `copy_assessment.hard_support_eligible = true`.
- Mention that target quality, correction provenance, aperture series, left/right consistency, and
  field-curvature counterevidence all passed.
- Mention that pair deltas are below the centred-support threshold.

## Forbidden Claims

- Do not call the copy optically perfect.
- Do not give keep/return advice.
- Do not imply the agent measured the image itself.
- Do not mention reshoot actions, because `copy_assessment.reshoot` is empty.
