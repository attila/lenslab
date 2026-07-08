# lenslab — Agent Skill & Claude Plugin Design

The product skill is the conversational front end. It **orchestrates the `lenslab` binary and
interprets its JSON** — it does not measure anything itself. Keep it thin; all determinism lives in
the CLI (see DECISIONS D6).

Current implementation note: the Rust CLI ships explicit-file JSON evidence through schema
`0.1-copy-assessment-support`: acutance/contrast, vignetting, lateral CA, straight-line distortion,
inferred field curvature, target QA, evidence-only left/right decentring aggregation, and top-level
`copy_assessment` support with blockers and reshoot guidance. The first skill slice interprets those
support states and coaches reshoots; Markdown/HTML output, artefacts, MTF50, CLI distribution, and
keep/return advice remain future work.

## Layout

```
agent-skills/
  lens-test/
    SKILL.md
    references/
      shooting-guide.md
      interpreting-results.md
      reshoot-coaching.md
      manual-uat.md
      examples/
        *.json
        *-checklist.md
plugin/
  .claude-plugin/plugin.json
  skills/
    lens-test/
      SKILL.md                      # thin Claude adapter pointing at agent-skills/lens-test/
```

`plugin.json`: name `lenslab`, version, description. Follow the current Claude plugin manifest
schema when changing the adapter.

## What the skill does

The shared skill core instructs the agent to:

1. **Locate the binary.** Require `lenslab` on `PATH`. If it is missing, stop with a setup error. Do
   not clone, build, install, or download unless the user explicitly asks for setup work.
2. **Gather input.** Accept explicit DNG/TIFF files or one flat folder. Folder expansion is
   skill-side only: direct children, DNG/TIFF, sorted, no recursion.
3. **Inspect a representative file.** Run `lenslab inspect <representative-file>` before analysis to
   surface lens/body identity, aperture spread, and correction-provenance blockers.
4. **Run measurement.** Run `lenslab analyse <paths...>` with explicit file paths only. Capture JSON
   from stdout and treat stderr as diagnostics.
5. **Interpret, do not recompute.** Read `copy_assessment.state`, `.evidence`, `.blockers`, and
   `.reshoot`. Explain centred/decentred/inconclusive support without moving judgement into the CLI
   or recalculating metrics.
6. **Coach reshoots** when `copy_assessment.state = inconclusive` or blockers are present. Use the
   smallest prioritised reshoot that can unblock hard support.

## Narrative contract

Output style the skill should produce:

- Lead with the **support state**: centred support, decentred support, or inconclusive.
- Then quantified performance: optimum aperture, corner lag, vignetting curve, CA, distortion — each
  tagged measured vs inferred.
- Do not give keep/return advice in this slice.
- British English, terse, no padding; distinguish verified from inferred explicitly.

## shooting-guide.md

Copy-test target shoot:

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

## interpreting-results.md

Use the decentring discriminator hierarchy from `ALGORITHMS.md §Decentring` (left/right symmetry →
aperture consistency → field-curvature counterevidence), the vignetting aperture-difference logic,
and the rule that hard support requires a gated target series, never scenes alone.

## Versioning

The plugin targets a `schema_version` range of the CLI JSON. On mismatch, tell the user to update
the binary rather than parsing best-effort.
