# Release Process

How to cut a `lenslab` release. This runbook covers prerequisites, versioning, the cut procedure,
post-release verification, hotfixes, failure recovery, and yank/rollback.

## Overview

Releases are triggered by pushing a `v*` tag from `main`. The
[`release.yml`](../.github/workflows/release.yml) workflow runs the project quality gates
(`just ci`), cross-compiles four binary targets via `cargo-zigbuild`, computes a `SHA256SUMS` file,
and publishes a GitHub Release with the matching CHANGELOG section as the body. The publish step is
gated by a `release` GitHub Environment that requires owner approval — push permission alone cannot
ship a release.

## Prerequisites

One-time setup, performed by the repository owner:

1. **GitHub Environment**. The `release` Environment with a required-reviewers list AND a deployment
   branches/tags policy is the security boundary that prevents push-only contributors from shipping
   a release. Both halves are required — reviewers gate the publish step, and the deployment policy
   gates which refs are allowed to request the gate at all.

   **Required reviewers**: Settings → Environments → New environment → name it `release` → enable
   "Required reviewers" → add the repository owner. The equivalent `gh` API call:

   ```sh
   USER_ID=$(gh api /users/attila --jq .id)
   gh api -X PUT /repos/attila/lenslab/environments/release \
     -F "reviewers[][type]=User" \
     -F "reviewers[][id]=$USER_ID"
   ```

   **Deployment branches and tags**: still in the `release` Environment, enable "Deployment branches
   and tags" → "Selected branches and tags" → add a rule with name `v*` and type `Tag`. Without
   this, the first tag push fails immediately with
   `Tag X is not allowed to deploy to release due
   to environment protection rules` because the
   GitHub default of "no rule" rejects tag-ref deployments. The default rule the UI suggests (`main`
   branch) does not apply — this project deploys from tag refs, not branches. The equivalent `gh`
   API calls:

   ```sh
   gh api -X PUT /repos/attila/lenslab/environments/release \
     -F "deployment_branch_policy[protected_branches]=false" \
     -F "deployment_branch_policy[custom_branch_policies]=true"
   gh api -X POST /repos/attila/lenslab/environments/release/deployment-branch-policies \
     -F "name=v*" \
     -F "type=tag"
   ```

   Verify both halves are in place:

   ```sh
   gh api /repos/attila/lenslab/environments/release \
     --jq '{reviewers: .protection_rules[] | select(.type=="required_reviewers").reviewers[].reviewer.login,
            deployment_policy: .deployment_branch_policy}'
   gh api /repos/attila/lenslab/environments/release/deployment-branch-policies \
     --jq '.branch_policies[] | {name, type}'
   ```

   Without reviewers, the publish job pauses indefinitely (recoverable: configure reviewers, then
   approve the still-pending deployment retroactively — no re-tag needed). Without the deployment
   tag rule, the publish job fails fast at the gate (recoverable: add the tag rule, then
   `gh run rerun <run-id> --failed` — safe in this specific failure mode because no
   `gh release create` ran, so there is no github.com state to collide with). Removing or emptying
   the reviewers list collapses the security boundary — do not change without an explicit security
   review.

   **Owner-confirmed as configured.** Re-run the verification commands above before the first real
   tag to confirm both halves are still in place — an unconfigured or partially-configured
   environment either fails `release.yml` at the gate or, worse, publishes without approval if
   GitHub auto-creates it unprotected on first reference.
2. **Local tooling**: `just`, `dprint`, `git-cliff`, and the GitHub CLI (`gh`) authenticated against
   the repo (`gh auth login`).
3. **Clean working tree** before starting any release procedure.

## Versioning rules pre-1.0

While the project is pre-1.0, semver constraints are intentionally loose.

| Version shape        | When to use                                                                   |
| -------------------- | ----------------------------------------------------------------------------- |
| `vX.Y.Z-alpha.N`     | Schema unstable, breaking changes expected. Default for early releases.       |
| `vX.Y.Z-beta.N`      | Feature-complete for the cycle, public testing welcome, no API guarantees.    |
| `vX.Y.Z-rc.N`        | No known blockers, last shake-out before stable.                              |
| `vX.Y.Z` (no suffix) | Stable release. Pre-1.0 still allows breaking changes between minor versions. |

**Bumping the minor (`v0.X.0`) vs the patch (`v0.0.Z`) pre-1.0:**

- **Minor** for any new user-facing capability or breaking JSON-schema change.
- **Patch** for fixes only.
- **Major** (`v1.0.0`) gates on a public stability commitment the project has not yet made.

## Cadence guidance

Solo-maintainer cadence — batch CHANGELOG entries to feature-completion boundaries, not per-PR.
Avoid a fixed cadence; cut a release when there is something worth releasing.

## Cutting a release

1. **Decide the version** per the table above.

2. **Curate `CHANGELOG.md`**. The `[Unreleased]` block accumulates entries from merged PRs.
   Hand-edit it before cutting:

   - Confirm the format matches existing entries (Keep a Changelog 1.1.0 — see
     <https://keepachangelog.com/en/1.1.0/>).
   - Write bullets to fit their section heading: under `Added`, prefer the feature or capability
     itself rather than repeating "Added ..." at the start of every entry.
   - Add or refine breaking notices and upgrade instructions, especially for the JSON output
     contract's `schema_version`.
   - **Do not run `just changelog`** — that recipe regenerates CHANGELOG from git-cliff and would
     clobber hand-curated breaking notices.

3. **Patch-vs-minor exception**: `release-prep` rotates the _entire_ `[Unreleased]` block. For a
   hotfix patch where `[Unreleased]` contains entries unrelated to the hotfix, after running
   `release-prep` hand-edit the rotated CHANGELOG to move non-hotfix entries back into a fresh
   `[Unreleased]` block before committing. Concretely:

   ```sh
   just release-prep 0.1.1
   # CHANGELOG now has: [Unreleased] (empty) + [0.1.1] - <today> with everything inside.
   $EDITOR CHANGELOG.md
   # Move non-hotfix bullets from [0.1.1] back up to [Unreleased].
   ```

4. **Run `release-prep`**:

   ```sh
   just release-prep 0.1.0-alpha.1
   ```

   This bumps the workspace version in `Cargo.toml`, rotates the CHANGELOG, runs `dprint fmt`, and
   runs `just ci` against the bumped tree before printing next-step instructions. Refuses on bad
   input (invalid semver, missing `[Unreleased]`, conflicting `[VERSION]`, or empty `[Unreleased]`
   block) before mutating any file.

5. **Open a release-prep PR**:

   ```sh
   git checkout -b ci/release-v0.1.0-alpha.1
   git commit -am 'chore(release): cut v0.1.0-alpha.1'
   git push --set-upstream origin HEAD
   gh pr create --draft --title 'chore(release): cut v0.1.0-alpha.1'
   ```

   Wait for CI green. Only the repository owner merges PRs and cuts releases.

6. **Tag from `main` only**:

   ```sh
   git checkout main
   git pull origin main
   git tag v0.1.0-alpha.1
   git push origin v0.1.0-alpha.1
   ```

   Tagging from any other branch is forbidden — see Failure Modes §1 for why.

7. **Approve the publish job**. The workflow runs `verify` and `build` automatically. When the
   matrix completes, `publish` pauses for owner approval at the `release` Environment gate. Open the
   workflow run in the GitHub Actions UI and click "Review deployments → Approve".

   The `gh` API equivalent (only members of the `release` Environment's required-reviewers list can
   use it — that is the security boundary):

   ```sh
   RUN_ID=<workflow-run-id>
   ENV_ID=$(gh api /repos/attila/lenslab/environments/release --jq .id)
   gh api -X POST "/repos/attila/lenslab/actions/runs/$RUN_ID/pending_deployments" \
     -F "environment_ids[]=$ENV_ID" \
     -F "state=approved" \
     -F "comment=approving v0.1.0-alpha.1"
   ```

## Post-release verification

After the workflow completes, verify the release end-to-end on at least one platform:

```sh
# Linux glibc example
VERSION=0.1.0-alpha.1
TARGET=x86_64-unknown-linux-gnu

curl -LO https://github.com/attila/lenslab/releases/download/v${VERSION}/lenslab-${VERSION}-${TARGET}.tar.gz
curl -LO https://github.com/attila/lenslab/releases/download/v${VERSION}/SHA256SUMS
sha256sum -c SHA256SUMS --ignore-missing   # macOS: shasum -a 256 -c SHA256SUMS --ignore-missing
tar xzf lenslab-${VERSION}-${TARGET}.tar.gz
./lenslab --version
```

Expected: SHA256 verification passes, binary executes, version string matches the tag.

## Hotfix path

Hotfixes follow the same merge-then-tag flow as regular releases, with one constraint: the hotfix
branches off the _tagged commit_ (not main HEAD), then merges to main, then is tagged from main.

```sh
git checkout v0.1.0
git checkout -b fix/critical-thing
# ... fix and commit ...
git push --set-upstream origin HEAD
gh pr create --draft --title 'fix: critical thing'
# Owner reviews, merges to main.
git checkout main && git pull
just release-prep 0.1.1
# (move non-hotfix entries back to [Unreleased] per §3 above)
git commit -am 'chore(release): cut v0.1.1'
# ... PR + merge + tag from main as usual ...
```

Never tag directly from a hotfix branch — every released SHA must be reachable from `main`. If the
hotfix conflicts with main HEAD beyond a clean cherry-pick, escalate to a regular minor bump rather
than forcing a hotfix.

## Prerelease promotion

When stabilising an alpha/beta/rc into a stable release (e.g. `v0.1.0` after `v0.1.0-rc.2`):

- Do **not** delete or demote the prior prerelease tags. They are the public record of the
  stabilisation arc.
- The `latest` pointer automatically jumps to the new stable release because GitHub filters
  prereleases out of `latest`.

## Failure modes

### 1. Tag pushed from a non-main commit

The workflow runs against the SHA the tag points at, not `main`. If the SHA is wrong, the release
will be cut from the wrong tree. Recovery follows the same procedure as §4 below.

### 2. `verify` fails on the tagged commit

`just ci` failed against the tagged commit. Do **not** retag the same version. Open a fix PR against
`main`, merge it, bump the patch (`vX.Y.Z+1`), and re-cut. Cleanup commands:

```sh
git push origin :refs/tags/v0.1.0-alpha.1   # delete remote tag
git tag -d v0.1.0-alpha.1                    # delete local tag
```

### 3. One build target fails (e.g. 3 of 4 green)

Two recovery options, with the trade-off named:

- **Ship a 3-target release**: edit the workflow `matrix.target` list on a follow-up commit to skip
  the broken target, document the gap in CHANGELOG (e.g. "macOS arm64 binary not available for
  v0.1.0, see v0.1.1"), bump to `vX.Y.Z+1` and re-cut. Affected users are gracefully steered to the
  next release.
- **Block the release until fixed**: delete the partial release + tag (commands below), fix the
  cross-compile in a PR, re-cut against the new patch version.

**Decision rule**: if the broken target is `aarch64-apple-darwin` (Apple Silicon Mac users are a
realistic install path), prefer blocking. Otherwise the 3-target ship is acceptable.

### 4. `gh release create` fails because release exists

A release with the tag already exists from a previous run (likely a partial-success retry). Delete
the release and tag in one step, bump version, re-cut:

```sh
gh release delete v0.1.0-alpha.1 --cleanup-tag --yes
# Then bump to v0.1.0-alpha.2 and re-cut.
```

If `--cleanup-tag` is unavailable on your gh version, fall back to:

```sh
gh release delete v0.1.0-alpha.1 --yes
git push origin :refs/tags/v0.1.0-alpha.1
git tag -d v0.1.0-alpha.1
```

**Never re-tag the same version.** The retag-fails-fast policy exists precisely because
retag-without-thinking is how broken artifacts ship.

### 5. Workflow re-run vs. retag

Do **not** re-run a failed `release.yml` workflow — neither via the GitHub UI's "re-run failed jobs"
button nor via `gh run rerun --failed`. The first run already created (or partially created) state
at github.com that the re-run will collide with. Always: delete release + tag, bump version, re-cut.
Re-running is safe for `ci.yml`; it is **unsafe** for `release.yml`.

## Yank / rollback

A release is "yanked" when a defect is discovered after publication. The mechanic is non-destructive
— artifacts stay on the release page (preserving checksum audit trail) but GitHub's `latest` pointer
skips them.

```sh
# 1. Demote the bad release from `latest`. Prerelease is the lighter touch than draft —
#    artifacts remain visible, but `latest` skips it.
gh release edit v0.1.0 --prerelease

# 2. Append a yank notice to the bad release body, pointing at the replacement.
$EDITOR /tmp/yanked-notice.md   # explain the defect; link to the fix release
gh release edit v0.1.0 --notes-file /tmp/yanked-notice.md

# 3. Cut a replacement release at the next patch version with the fix included.
#    (Follow the standard cut procedure above.)
```

After yanking, `releases/latest/download/...` URLs resolve to the previous good release
automatically. No README update required for a yank — only for the new fix release, which doesn't
change the snippet shape because it uses `releases/latest/`.

Update `CHANGELOG.md` retroactively only if the defect introduced a security or data safety risk
(rare). Otherwise the yank notice on the release page is sufficient.

## Auditability

Release writes via `GITHUB_TOKEN` are logged in repository audit logs. A suspicious release can be
traced to the workflow run that created it by cross-referencing the run ID with the release creation
timestamp.

## Why these choices

Single-runner zigbuild keeps the four-target matrix on one job type instead of maintaining native
macOS/Linux runners. No GPG signing: SHA256SUMS plus GitHub's own provenance is judged sufficient
for the project's threat model at this stage; revisit if that changes. No git-cliff round-trip
during release-prep: hand-curated CHANGELOG entries (breaking notices, upgrade steps) would be
silently clobbered by full regeneration, so `release-prep` rotates the existing block in place
instead. Retag-fails-fast and the owner-approval Environment gate are both deliberate frictions: a
release is a one-way door (an artifact, once downloaded, cannot be recalled), so the process trades
a small amount of manual ceremony for the inability to ship one by accident or via a compromised
non-maintainer token.
