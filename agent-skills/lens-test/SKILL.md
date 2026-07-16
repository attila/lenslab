---
name: lens-test
description: Interpret lenslab analyse JSON, relay only CLI-prescribed capture fixes, and keep the Rust CLI as the only measurement engine. Use when a user wants to assess a lens copy from DNG/TIFF captures, understand copy_assessment support, or get reshoot guidance.
---

# Lens Test

Use this skill to help a user prepare DNG/TIFF lens-test captures, run `lenslab inspect` and
`lenslab analyse`, interpret the resulting `copy_assessment` evidence, and recommend the smallest
CLI-prescribed reshoot when the result is inconclusive.

## Contract

- The Rust CLI measures; the agent interprets and coaches.
- This is the shared skill core. Claude, Codex, opencode, and future harness adapters should point
  at these instructions rather than copying the interpretation rules.
- Never re-measure, recompute target QA, recalculate decentring asymmetry, or infer optical support
  from pixels yourself.
- Treat `lenslab analyse` stdout JSON as the source of truth.
- Keep measured facts separate from inferred support.
- Do not give keep/return advice in this slice.
- Do not call a lens centred or decentred when `copy_assessment.state` is `inconclusive`.

## Input Preparation

Accept either explicit capture files or one flat capture folder.

For explicit files:

- Keep the user-supplied order unless the user asks otherwise.
- Reject paths that are not local files before running the CLI.

For one folder:

- Read only direct children.
- Keep files ending `.dng`, `.tif`, or `.tiff`, case-insensitively.
- Sort accepted files by path before analysis.
- Do not recurse.
- Ignore sidecars and other raw extensions such as `.nef` for folder expansion.
- If no direct child DNG/TIFF files exist, ask for usable files instead of running analysis.

## CLI Flow

1. Locate `lenslab` on `PATH`.
2. If `lenslab` is unavailable, stop and tell the user the CLI is not installed or not on `PATH`. Do
   not clone the repository, build from source, or install tools unless the user explicitly asks for
   setup work.
3. Run `lenslab inspect <representative-file>` before analysis.
4. Use `inspect` output to surface early blockers: mixed lens/body identity, missing aperture
   spread, or correction provenance that makes the input unsuitable.
5. Run `lenslab analyse <paths...>` with explicit files only. Do not pass folders directly to
   `analyse`.
6. Parse JSON from stdout. Treat stderr as diagnostics.
7. Require `schema_version: "0.1-copy-assessment-support"` for this skill version. On mismatch, tell
   the user to update the binary or skill instead of parsing best-effort.

## Interpretation

Read these fields first:

- `copy_assessment.state`
- `copy_assessment.evidence`
- `copy_assessment.blockers`
- `copy_assessment.reshoot`

State mapping:

- `supports_centred`: lead with centred support, then explain which gates passed.
- `supports_decentred`: lead with decentred support, then explain the asymmetric evidence and any
  counterevidence that was ruled out.
- `inconclusive`: lead with inconclusive and explain the blockers. Do not call the lens centred or
  decentred.

Use `references/interpreting-results.md` for the full interpretation rules and
`references/reshoot-coaching.md` for blocker-to-reshoot wording.

## Reshoot Coaching

When support is blocked or inconclusive:

- Show a prioritised blocker shortlist.
- Treat `copy_assessment.reshoot` as a strict allowlist for capture changes, replacement input, and
  protocol advice.
- When the list is non-empty, show only its actions and prefer the smallest one that can unblock
  hard support.
- When the list is empty, make the entire next-step content: "The CLI prescribed no capture action."
  Do not suggest ideal input or a shooting protocol, or tell the user to rerun.
- A generic request such as "what should I do next?" does not override an empty list. Reference
  `references/shooting-guide.md` only when the user explicitly asks for the general protocol, and
  make clear that it is not advice derived from this run.

## Required Output Shape

Keep the answer short and factual:

1. Support state: centred support, decentred support, or inconclusive.
2. Evidence: measured facts and inferred support, labelled honestly.
3. Blockers or counterevidence.
4. Prioritised reshoot actions only when `copy_assessment.reshoot` is non-empty.
5. Explicit non-claims when the evidence does not support a hard answer.

Use the examples in `references/examples/` as the interpretation contract. The checklists are the
marking scheme; exact prose does not need to match.
