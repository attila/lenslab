---
name: github-pr-ci-status
description: Use when checking whether GitHub Actions CI has run or passed on a pull request via the GitHub MCP tools (`mcp__github__pull_request_read`) — before claiming CI is green, red, or hasn't run yet.
---

# Checking PR CI status: `get_status` vs `get_check_runs`

## Symptom

`mcp__github__pull_request_read` with `method: "get_status"` returns `"total_count": 0` on a PR that
has GitHub Actions workflows configured and has visibly run them (confirmed separately via
`mcp__github__actions_list` or the Actions UI). This reads as "no CI has run against this commit,"
and is wrong.

## Root cause

`get_status` calls GitHub's legacy Combined Commit Status API (`GET
/commits/{sha}/status`) — the
API older third-party CI integrations (Travis, CircleCI, pre-Checks-API Jenkins) use to post a
status directly onto a commit. GitHub Actions does **not** use this API. It reports through the
separate Checks API instead. A repository whose only CI is GitHub Actions will **always** return
`total_count: 0` from `get_status`, regardless of whether any workflow has run, is running, or
failed — the field is simply not populated by that integration path.

## Fix

Use `method: "get_check_runs"` instead — it calls the Checks API and returns the actual GitHub
Actions job list (name, status, conclusion) for the PR's head commit. This is the only one of the
two methods that reflects Actions results.

If you need run-level detail (timestamps, the workflow run/job URLs, commit message per run), use
`mcp__github__actions_list` (`method: "list_workflow_runs"`) instead or in addition.

## Don't be misled

`get_status`'s zero is not "pending" or "no data yet" in the way a fresh/uncompleted run would show
under `get_check_runs` (there you'd see `"status": "in_progress"` or `"queued"` entries, not an
empty list). A `get_status` zero tells you nothing about Actions; treat it as answering a different
question ("are there any legacy commit statuses"), not "has CI run."
