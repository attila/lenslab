# lenslab — Claude Plugin & Skill Design

The plugin is the conversational front end. It **orchestrates the `lenslab` binary and interprets
its JSON** — it does not measure anything itself. Keep it thin; all determinism lives in the CLI
(see DECISIONS D6).

Current implementation note: this document describes the target coaching and verdict flow. The Rust
CLI currently ships explicit-file JSON evidence through schema `0.1-target-qa`: acutance/contrast,
vignetting, lateral CA, straight-line distortion, inferred field curvature, target QA, and
evidence-only left/right decentring aggregation. Folder-based coaching, Markdown/HTML output,
artefacts, MTF50, and copy verdict interpretation remain future plugin work.

## Layout

```
plugin/
  .claude-plugin/plugin.json
  skills/
    lens-test/
      SKILL.md
      references/
        shooting-guide.md           # how to shoot a copy test (port from below)
        interpreting-results.md     # how to read the JSON verdict (port from ALGORITHMS §Decentring)
```

`plugin.json` (sketch): name `lenslab`, version, description, one skill `lens-test`. Follow the
current Claude plugin manifest schema when implementing.

## What the skill does

The skill's `SKILL.md` instructs the agent to:

1. **Locate the binary.** Prefer `lenslab` on `PATH`; otherwise offer to `cargo install`/build from
   the repo. Never reimplement measurement in the skill.
2. **Gather input.** Ask for the folder of frames; run `lenslab inspect` on a sample to confirm
   lens, body, aperture spread, and that **no corrections are baked in**.
3. **Decide target vs scene.** If the user wants a copy verdict and there is no flat-target series,
   **coach the shoot** (shooting-guide.md) rather than guessing from scenes.
4. **Run measurement.** `lenslab analyse <folder> --format json,md` (plus `decentre` for a focused
   copy gate). Capture the JSON.
5. **Interpret, do not recompute.** Read the JSON; produce the human brief using the
   verified/inferred framing and the decentring discriminators from `interpreting-results.md`.
   Present the artifact PNGs (contact sheet, corner crops, curves).
6. **Coach re-shoots** when `verdict.copy = inconclusive` or `qa.gated = true` (e.g. keystone over
   threshold).

## Narrative contract

Output style the skill should produce (matches what the origin session delivered):

- Lead with the **copy verdict** (centred / decentred / inconclusive) + confidence + the evidence
  list from JSON.
- Then quantified performance: optimum aperture, corner lag, vignetting curve, CA, distortion — each
  tagged measured vs inferred.
- Keep/return steer framed against use case and cost, not MTF perfection.
- British English, terse, no padding; distinguish verified from inferred explicitly.

## shooting-guide.md (content to port)

Copy-test target shoot, validated in the origin session:

- **Target:** flat, evenly-textured, fronto-parallel surface (brick wall, newspaper spread, detailed
  poster). Must fill the frame with **all four corners on texture**.
- **Distance:** for an ultra-wide, frame's long edge ≈ `1.75 × distance` (≈83° horizontal on 44×33).
  Fill with ~10% margin. Generalise: back up until the target fills the frame, corners included.
- **Square-on:** camera axis perpendicular to the surface; use the texture's lines to keep
  horizontals/verticals parallel to the frame. Keystone fakes decentring. Use a bubble/digital level
  to better than ~1° if available (eye-levelling leaves ~3% tilt — measurable, and it contaminates
  the top/bottom axis).
- **Focus:** single central point (or magnified live-view), then **do not refocus** through the
  series.
- **Aperture ladder:** one frame each at f/4, f/5.6, f/8, f/11 (extend if characterising
  diffraction). Two per stop for shake insurance.
- **Disambiguator (optional, valuable):** one extra wide-open frame focused on a **corner**; if it
  sharpens, corner softness is field curvature, not decentring.
- **Light/ISO:** even, flat light (overcast/open shade); base ISO; tripod + timer/mirror-up ideally,
  else ≥1/250 s.

## interpreting-results.md (content to port)

Port the decentring discriminator hierarchy from `ALGORITHMS.md §Decentring` (left/right symmetry →
aperture consistency → visual smear → field-curvature vs decentring), the vignetting
aperture-difference logic, and the rule that a hard `decentred` verdict requires a **gated target
series**, never scenes alone.

## Versioning

The plugin targets a `schema_version` range of the CLI JSON. On mismatch, tell the user to update
the binary rather than parsing best-effort.
