# lenslab — Decision Log

Lightweight ADRs. Each: decision, options weighed, rationale. All are **locked** for v0.1 unless
noted.

## D1 — Language: Rust

**Options:** Python (rich raw/imaging ecosystem) · Node · Go · Rust. **Decision:** Rust. **Why:**
Single statically-linked binary, no runtime package ecosystem (the explicit driver — Python's
packaging was a non-starter for the owner). Native raw decoders exist (rawler/rawloader/zenraw).
Strong numerics (`ndarray`, `rustfft`, `image`). Go has no credible native raw decoder (only
cgo→LibRaw, which forfeits the clean binary); Node's quality path is native LibRaw addons via
node-gyp (the same fragility we are avoiding).

## D2 — Decode backend: `rawler` behind a trait

**Options:** `rawloader` (≈200 cameras, Bayer only) · `rawler` (300+, X-Trans/CR3/JXL-DNG) ·
`zenraw` (safe Rust, scene-linear f32 output, swappable backends) · bind LibRaw. **Decision:**
`rawler`, accessed through a `Decoder` trait in `lenslab-decode`. **Why:** Broadest pure-Rust
coverage. The trait keeps the backend swappable (zenraw or a permissive fallback later) and
**confines the LGPL dependency to one crate**. zenraw's scene-linear f32 output is attractive and a
likely future backend.

## D3 — Licence: option (a), single static LGPL binary

**Options:**

- (a) Accept LGPL-2.1 on the distributed binary; statically link `rawler`; core stays MIT/Apache.
  **One static binary.**
- (b) Keep a permissive binary by splitting decode into a separate process or `cdylib`. **Not a
  single binary** (two executables, or binary + shared lib).
- (c) Permissive-only: own SOF3 decoder + `tiff` crate; DNG-lossless-JPEG + TIFF only, no
  proprietary raws.

**Decision:** (a). **Why:** For a fully open-source project the LGPL §6 relink obligation is
satisfied automatically — the complete buildable source is public, so any recipient can rebuild
against a modified `rawler`. (b) only matters when embedding the core in closed/permissive-only
software, which is not a goal ("deploy to multiple agents later" concerns plugin distribution, not
embedding the Rust core). (a) preserves single-binary ergonomics. The LGPL surface is still confined
to `lenslab-decode`, leaving (c) available to anyone who needs a fully-permissive build. **Note:**
The grey area (some lawyers dislike static-link LGPL even with full source) is the only reason to
revisit; revisit only if embedding-in-closed-software becomes a requirement.

## D4 — Name: `lenslab`

Chosen over optik / apertura / mtflab.

## D5 — Plugin first

**Decision:** Ship a Claude plugin (`plugin/`) first; keep the orchestration logic portable so other
agents can host it later. **Why:** Matches the owner's deployment path. The skill is thin
(orchestrate + narrate); portability comes free from keeping all real logic in the binary behind the
JSON contract.

## D6 — Division of labour: measurement in Rust, judgement in the plugin

**Decision:** The CLI does all deterministic measurement and emits versioned JSON. The plugin
coaches the shoot, runs the binary, interprets JSON into a verdict, and never re-measures. **Why:**
Determinism, testability and reproducibility belong in compiled code; shot coaching and narrative
judgement (verified/inferred framing, keep/return steer) are where an LLM adds value. The versioned
JSON schema is the contract between them.

## D7 — v0.1 scope: full measurement battery

**Decision:** v0.1 includes ingest/normalise (DNG+TIFF), `inspect`, `contact`, sharpness (MTF50 +
acutance), decentring, vignetting, **CA, distortion, field-curvature**. v0.2: HTML report,
focus-bracket support, additional decode backends. **Why:** Owner wants the whole battery usable
from the first release; the algorithms are already validated (see ALGORITHMS.md), so the risk is
implementation, not method.

## D8 — Measurement on uncorrected linear data

**Decision:** Always demosaic-free (single green plane) for sharpness, no WB/gamma/opcode
corrections; detect and refuse/warn on baked-in corrections (DNG opcodes / TIFF profile tags).
**Why:** Physical, reproducible numbers. Cooked input silently invalidates vignetting and sharpness.
This is non-negotiable and must be enforced at ingest.

## D9 — macOS cross-compile SDK: `phracker/MacOSX-SDKs`, pinned at 11.3

**Options:** (a) No macOS cross-compile — Apple Silicon/Intel Mac binaries only ship from a real
macOS runner. (b) `cargo-zigbuild` cross-linking from the Linux CI runner, sourcing a macOS SDK from
a community redistribution (headers + `.tbd` framework stubs extracted from Xcode; Apple provides no
official redistributable SDK). (c) Same, but self-host an SDK mirror.

**Decision:** (b), pinned to `phracker/MacOSX-SDKs`' `11.3` (Big Sur) release, checksum-verified in
CI. **Why:** `rawler` (via `chrono` → `iana-time-zone`) links `CoreFoundation` for the system
timezone on macOS; zig's bundled cross-linker has no macOS frameworks to satisfy that without a real
SDK. `phracker/MacOSX-SDKs` is the de facto standard for exactly this in the Rust/zig cross-compile
community — there is no more "official" alternative, since Apple's SDK licence does not permit a
sanctioned redistributable. SDK vintage only bounds which _new_ macOS APIs are available at build
time, not which macOS versions can run the result (macOS binaries are forward-compatible), so 11.3
is more than sufficient for the one framework call in play. Confirmed with the owner before landing,
since it is a new third-party dependency of the shared release pipeline (owner has an Apple
Developer Program membership for the separate, later code-signing/notarization step —
`docs/ROADMAP.md`).

## D10 — Real-camera fixture storage: GitHub Release asset, checksum-fetched

**Options:** (a) Commit the DNG(s) directly to the repository. (b) Git LFS. (c) External fetch —
host as a GitHub Release asset in this repo, downloaded by a script and verified against a pinned
SHA256, mirroring the macOS SDK fetch already in `.github/workflows/ci.yml`.

**Decision:** (c). **Why:** Real camera raws run tens of MB (`docs/ROADMAP.md` "Real DNG fixture").
(a) permanently bloats every clone — git never forgets a blob without a history rewrite — and every
CI job that checks out the repo pays for it even when it doesn't touch the fixture (`fmt`/`clippy`/
`deny`/`doc` all check out; only `test` would need it). GitHub also soft-warns above 50MB and
hard-blocks above 100MB per file. (b) keeps history small but adds a new required tool (`git-lfs`)
for every contributor and CI job, and leans on GitHub's account-wide LFS quota (10GB storage + 10GB
bandwidth/month, free tier) for something that doesn't need a dedicated large-file service. (c)
reuses a pattern already vetted in this repo rather than introducing one: `curl` + `sha256sum -c`,
cached via `actions/cache` in CI. Release assets go up to 2GiB with no separate quota, need no
credentials to fetch since the release is public, and keep repository history light regardless of
how many fixtures accumulate later (the next roadmap item, zone geometry, also wants real pixel
data). Rejected external hosts (S3, Dropbox, Bunny.net): each is a new account, credential, and
billing relationship to maintain indefinitely for a handful of files already sitting next to code
that lives on GitHub anyway — no durability or cost advantage over a Release asset in this same
repository. **Note:** kept off the version-tagged releases `release.yml`/ `scripts/release-prep.sh`
produce — those are owner-approval-gated shipped-binary releases; fixture assets live under a
separate, non-version tag (`fixtures-dng-v1`) so the two never entangle.

## Open questions (not blocking v0.1 start)

- Camera→pixel-pitch source: derive vs small bundled DB vs config override. (ALGORITHMS §MTF50.)
- Auto edge-detection quality for MTF50 vs requiring `--roi`.
- Frame-role auto-classification heuristic thresholds (target vs scene).
- Sidecar format for per-frame tags (role, focus-bracket) — filename convention vs TOML sidecar.
