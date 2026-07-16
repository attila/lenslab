# Manual UAT

Run this before treating the skill as ready.

## Fixture-JSON UAT

1. Pick one example JSON file from `references/examples/`.
2. Ask an agent using the `lens-test` skill to interpret it.
3. Compare the answer with the matching `*-checklist.md`.
4. Record whether the answer included each required fact and avoided each forbidden claim.

## Folder UAT

1. Point the skill at a folder containing direct child DNG/TIFF files.
2. Confirm the skill expands only direct children and does not pass the folder directly to
   `lenslab analyse`.
3. Confirm the skill runs `lenslab inspect <representative-file>` before `lenslab analyse`.
4. Confirm inconclusive output explains the blockers and gives only actions present in
   `copy_assessment.reshoot`; an empty list must not produce any prescriptive next step, including
   replacement input, protocol, or rerun advice.

If live plugin execution is unavailable, record that as residual risk. A dry review can check the
instructions, but it does not prove the packaged skill is usable by an agent runtime.
