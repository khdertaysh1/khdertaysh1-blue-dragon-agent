# Blue Dragon 0.9.0-alpha.6 candidate and trial plan

The package version is `0.9.0-alpha.6`, the prospective tag is `v0.9.0-alpha.6`, and the display
name is **Blue Dragon 0.9.0-alpha.6**. This candidate is unpublished. The version is a
product naming decision, not evidence of equivalence, stability or device tests.
Existing published releases must not be changed or deleted.

## Scope of this reliability candidate

This release preparation starts from frozen implementation commit `1a41627`,
a descendant of published alpha.5 main `7da0c186`. The release-only difference
updates package/lock versions, installer test version and release documentation;
it adds no runtime features. The integrated changes cover continuity checkpoints,
fresh native source reads and explicit byte ranges, command argument validation
and bounded normalization, local redacted tool-result inspection, and exact
quoted-name discovery repair. Original calls, approvals, user changes, licenses
and published assets remain intact. See [release notes](RELEASE_NOTES.md),
[command evidence](COMMAND_ARGUMENT_REPAIR.md) and
[discovery evidence](DISCOVERY_COMMAND_REPAIR.md).

The implementation candidate passed 353 Rust tests and 35 local qualification
groups. A single live free-route local print completed with one explicit approval,
three provider requests and no observed 429. It emitted correctly typed arguments;
malformed input repair is supported by deterministic CLI tests, not that live run.
An earlier live run exhausted five steps in discovery. These runs do not establish
20–30 successful live exchanges, a reliable hour-long session, sustainable quota,
or future absence of 429. Earlier scripted continuity exchanges are not live
model reasoning evidence. Physical Android and final platform/browser CI remain
separate, explicitly unverified gates for this release candidate.

## Implemented capability and remaining gaps

| Area | Implemented and locally exercised | Remaining limits / verification |
| --- | --- | --- |
| Files | Bounded UTF-8 windows, streaming search, source pins, syntactic outlines | No semantic repository graph; external filesystem races are outside guarantees |
| Agents | Independent model loops and sessions, concurrency two, shared durable budgets, messaging, cancellation, pinned evidence reports and bounded inspection | Bounded child proposals; parent-only apply/rollback require approval and version checks. No direct child writes, browser ownership, nested delegation or isolated mutable workspaces |
| Research | Explicit coverage cells, competing hypotheses, request comparisons, retest history and evidence versions | Text/status comparisons do not prove causality, identity or current truth; hypotheses remain unresolved without evidence |
| Browser | Root-owned CDP/extension, scope/approval checks, forms/scroll and uncertain-outcome handling | Final candidate must pass real Chromium/Firefox CI; integrated protocol fixture is a mock, not DOM validation |
| Memory and integrations | Scoped lexical memory with source revalidation; approved stdio MCP tools/resources | No semantic graph, remote MCP transports or broad service integrations; excerpts are not automatically secret-redacted |
| Continuity | Interrupted/unknown recovery and no automatic replay of uncertain operations; reports reread pinned history | Full-store forgery and external races excluded; Android system termination/background limits need actual device trials |
| Comparison | Real Blue Dragon and AlphaCode CLIs exercised with independent fixture checks for read/edit contracts | Scripted deterministic providers, different build profiles; no live-model reasoning, speed or general equivalence benchmark |

This is an evidence-based grouping, not a claim that an unseen numbered list of
requirements has been fully closed. See `INTEGRATED_ALPHA_COMPARISON.md` for the
code comparison and feature-specific evidence. Direct child writes, broader live-model capability validation and remote
integrations remain outside this release.

## One candidate, then CI, trial and publication

1. Freeze one source commit containing the integrated package and this release
   preparation. Submit that branch as one reviewable PR. Do not merge just to
   obtain binaries: pull-request CI builds artifacts without publishing.
2. Require Linux/Windows/macOS tests, Android ARM64 cross-build, installer and
   complete payload checks, pinned local-lab qualification, and **real browser**
   jobs on the exact candidate. Record run URL, commit and artifact hashes.
   Local Chromium startup was blocked by socket permissions; that is not a pass.
3. Download `blue-dragon-Android-ARM64` and `blue-dragon-trial-kit` from that same
   successful run. Check the Android artifact's SHA256SUMS before execution.
   Keep the trial binary separate from the user's installed binary. The Python
   kit is optional diagnostics, not an installer dependency. It requires Python
   3.11+; do not install packages automatically.
4. Extract the kit into a fresh folder in Termux's private home (shared Android
   storage may prohibit executing binaries). Run, using the actual absolute
   paths to the downloaded executable and a **new** result directory:

   ```sh
   chmod +x "$HOME/blue-dragon-trial/blue-dragon"
   python scripts/device_trial.py --binary "$HOME/blue-dragon-trial/blue-dragon" \
     --output "$HOME/blue-dragon-trial-results-1"
   ```

   The runner uses six existing deterministic CLI fixtures, isolated homes and
   loopback services. It records version, source/kit/binary hashes, per-group
   status and log hashes. It needs neither Rust nor Git at runtime. The manifest
   detects inconsistent transport, not malicious replacement of the whole kit.
   Keep the downloaded CI artifact provenance. Review logs before sharing.
5. Record actual device/model/Android/Termux versions and user-observed results.
   Also check the cyan BLUE/white DRAGON wordmark at narrow phone
   widths; run the TUI with `BLUE_DRAGON_MOTION=1`, then with
   `BLUE_DRAGON_REDUCED_MOTION=1`, and verify approval controls stay readable.
   The animated README dragon is not expected to render inside a text terminal.
   Separately exercise browser pairing, approval denial/scope withdrawal, and
   system termination during an action. Restart must show interrupted/unknown
   and must not replay an uncertain action. The kit alone does not prove these
   device behaviors, real browser behavior, or live-provider performance.
6. Resolve CI/device failures and repeat the affected checks on the final commit.
   The user authorized publication after review and required CI; the coordinating
   parent handles GitHub, while unavailable physical-device testing remains
   explicitly untested. Publication behavior: **a push/merge to
   main automatically publishes after all required jobs pass**. The workflow
   packages the complete platform/extension set and creates `v0.9.0-alpha.6`. A published
   same-tag release is immutable; later fixes need a new version.
7. After publication, the existing installer accepts `--version v0.9.0-alpha.6`, verifies
   SHA256 and executable version and keeps the previous binary for rollback.
   Do not invoke that tag before it exists. There has been no release from this
   local preparation work.

Release metadata deliberately retains `physical_android_tested: false` until
verified device evidence is explicitly incorporated. Cross-compilation and host
metadata are insufficient. No mock success may be labeled a Chromium success.

## Short supervised trial prompt

```text
Inspect this local project. Preserve my files and scope. Read large files in bounded
windows, cite native evidence, and propose a small two-file improvement. Show the
full diff and wait for my approval before applying. Verify the resulting files.
If interrupted or a provider fails, report partial progress and never replay an
action whose outcome is unknown.
```
