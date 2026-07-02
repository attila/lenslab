---
name: post-work-compounding
description: Use after a larger piece of work or specialised targeted bugfix, before merge, to decide what should be compounded as a repo-local skill, a docs/solutions institutional learning, a rule update, or nothing. Start by showing candidates, then produce only the artefacts that earn their place.
---

# Post-work compounding triage

Use this before merging meaningful work to preserve the learning without turning every finding into
prompt weight.

## Process

1. Inspect the actual work:
   - `git status --short --branch`
   - `git log --oneline origin/main..HEAD`
   - `git diff --stat origin/main...HEAD`
   - Read `docs/ROADMAP.md` when the work affects project direction.
   - Read existing `.ai/skills/*/SKILL.md` and `docs/solutions/**` entries relevant to the area.
2. Show candidates first. For each candidate, state:
   - the learning
   - why it may recur
   - proposed home: skill, `docs/solutions`, rule, roadmap, or no artefact
3. Apply the placement test:
   - **Repo-local skill**: repeatable execution checklist or gate that should shape future agent
     behaviour before acting.
   - **`docs/solutions`**: situational bug, domain lesson, investigation trail, or design rationale
     that should be searchable when a similar problem appears.
   - **Rule update**: always-on invariant for this repo or language, especially when violation is
     cheap to repeat and expensive to catch late.
   - **Roadmap**: product/task state, deferred work, or next-step sequencing.
   - **No artefact**: one-off fact, obvious implementation detail, or knowledge already covered
     clearly.
4. Prefer fewer artefacts. One focused skill plus one solution doc usually compounds better than
   several narrow documents.
5. When writing:
   - Keep skills procedural and short.
   - Keep solution docs concrete: context, problem, solution, verification, and when to apply.
   - Update existing artefacts instead of creating duplicates.
6. Verify:
   - `git diff --check`
   - Re-run the relevant repository gate if the artefact can affect generated docs, formatting, or
     lint.

Do not turn war stories into auto-loaded skills. Skills are for future action; solution docs are for
future recall.
