---
name: lens-test
description: Interpret lenslab analyse JSON and coach lens-copy capture reshoots using the shared lenslab skill core.
---

# Lens Test Adapter

This Claude plugin skill is a thin adapter. The durable behaviour lives in the shared repo skill:

```text
${CLAUDE_PLUGIN_ROOT}/../agent-skills/lens-test/SKILL.md
```

Before acting, read that shared `SKILL.md` and its referenced files under:

```text
${CLAUDE_PLUGIN_ROOT}/../agent-skills/lens-test/references/
```

Do not duplicate or override the interpretation, reshoot, or capture-preflight rules here. Use this
adapter only to expose the shared `lens-test` core through the Claude plugin scaffold.
