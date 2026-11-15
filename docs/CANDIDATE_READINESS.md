> Historical 0.8 candidate preparation record. The integrated 0.9 candidate is documented in [INTEGRATED_09_READINESS.md](INTEGRATED_09_READINESS.md); do not use the old binary hash or CI claims below to qualify it.

# One delivery candidate: 0.8.0-alpha.1

Branch: `candidate/professional-preview`. This is the single unpublished delivery branch. It contains the complete reviewed linear chain from `237bb56` through `cdb9406`, followed only by candidate versioning, release notes, delivery audit and an integrated CI gate. No feature branch needs a separate merge to try this candidate. Do not substitute Draft PR #13's older binary.

## What is ready to try

| Area | Working capability and evidence | Remaining boundary |
| --- | --- | --- |
| Large sources | Fetch once, immutable SHA256 versions, UTF-8 search/read, explicit refresh and saved-version comparison. Full suite tests a 50,000-line multilingual source, reuse with the server gone, tail read and tamper rejection. | 8 MiB/source, 32 versions/64 MiB/session; discovery 2 MiB, flow 1 MiB. Larger analysis is rejected, not claimed complete. |
| Discovery and code investigation | Static/literal dynamic references; selected scoped dependency fetch; bounded cached import/re-export chains, simple argument/parameter bridge and same-function uses. | Unsupported/computed imports, shadowing and semantic execution remain unresolved. Explicit caps/omissions are surfaced. |
| Scope and selective engines | Persistent read/interact/change scope; individual form, response-policy, marker/reflection and redirect reviews; controlled comparison with repeated control. | Scope is not an OS firewall. Policy headers and reflection do not prove protection or exploitation. |
| Integrated local lab | Approved fixed harmless three-request experiments against the pinned lab, durable no-replay IDs, weak/fixed controls and independent response/parser oracles. | HTTP and parser evidence, not graphical browser execution or full exploit demonstration. |
| Reports and evidence | Native request/result pairing and digests, historical coverage/source investigations, canonical lab recipes and recomputed outcomes. Export after restart and scope revocation; tampered evidence rejected, zero report requests. | Bounded recent-observation windows; no authenticity guarantee against rewriting the entire checkpoint. Raw bodies/model prose omitted. |
| Task/session continuity | Durable sessions, bounded recall, background progress/cancellation, unknown interrupted actions not blindly repeated. | Windows background jobs unsupported; escaped Unix process groups and power-loss guarantees remain bounded. |
| Browser | CDP/extension actions, frames/readiness checks, scope checks, omission counters and connection-generation dispatch. Mocked producer contracts pass locally. | Current graphical browser run blocked by environment socket policy; current real-browser CI still required. No unconditional React-ready claim. |
| Models and vision | Offline model selection passes; destination-specific consent, one-shot screenshot transmission, image-history pinning and memory-export restrictions. | No live-provider run. Free/unknown vision gateways are rejected; availability and pricing are not guaranteed. |
| Termux/install | ARM64 selection, visible package/download progress, bounded network waits, hash/version checks and safe replacement/rollback. Offline installer fixtures pass. | Current Android cross-build and physical phone test remain outstanding. Browser packages need official Termux repositories and separate consent. |

## Independent review corrections included

All corrections below are ancestors of this candidate and retained in the full qualification. Parent-reported independent review found no remaining blocker for the corresponding fixed batches; that is not a claim of no undiscovered bugs.

- Stale/reused evidence and automatic URL provenance: `438b127`, `f74e016`, `4a597ba` (paired digests, no invented original requests).
- Form field/form truncation at 61/21: `446233a` (producer-to-engine completeness).
- CDP network omission after 300: `4663ccd` (producer counter and unknown legacy metadata).
- Truncated import bindings: `5f252ab` (limited chain even when omitted symbol is absent).
- Malformed duplicate `src` and custom-element prefix: `d28dcd1`.
- Compound/update writes: `1a3c1b6` (no unsafe alias expansion).
- Nested/unexported/reassigned call targets: `5bfd436` (direct named export subset and competing-write rejection).
- Integrated historical source/lab reports: `a8ebc93`, `bace56b` (restart, revoked scope and modified evidence tests).

## Repository and build audit

The pre-delivery tracked tree has no file over 1 MiB; largest is a 301605-byte existing GIF, with about 2.11 MB total tracked data. The source cache is runtime session data, not committed downloads. Cargo.toml dependencies and all third-party Cargo.lock entries are unchanged from main; only the root package version changes to `0.8.0-alpha.1`. No dependency upgrade or vulnerability-database audit is claimed. A targeted secret-pattern scan found no matching private key/token patterns in source/scripts/docs/validation/workflows; this is not an exhaustive secret scan.

The combined Linux qualification passed 21/21 groups and 24/24 deterministic runtime samples, with 233 Rust tests, clippy and formatting. The versioned candidate is rebuilt and rerun before handoff; its final binary hash and exact results are recorded in `validation/candidate-readiness.json`. Package versioning changes the binary identity; old qualification hashes must not be used for the new preview.

## CI truth and delivery gate

Read-only GitHub inspection confirmed Draft PR #13 is still draft/open at `de2f613d82be3a5e83629c7ec4bf7f4a0ec6ca97`. Its run [43662580002](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/actions/runs/43662580002) passed Linux/Windows/macOS, Android ARM64 cross-build, installer and real-browser jobs; publication was skipped. **Those results cover the old draft, not this candidate.** Main remained the PR base `237bb56` when inspected.

The candidate has not been uploaded and has no candidate CI result. Its workflow now additionally checks out the reviewed lab at an exact commit and runs the integrated matrix on Linux. Publication depends on that job as well as the existing platform, Android, installer and real-browser jobs. CI may discover issues on unavailable platforms; do not describe local passage as cross-platform qualification.

## Concrete path to a user-testable build

1. Obtain authorization to upload this one candidate branch and open/update a Draft PR. No merge or release is needed to get CI artifacts.
2. Push this branch without force-replacing unrelated work, and ensure the PR head exactly matches the candidate commit. Run the configured PR checks.
3. After candidate CI passes, share its Android ARM64 artifact/checksum for a private-storage Termux smoke test, and its matching Linux/Windows/macOS artifacts as applicable. Do not offer the Linux binary as an Android binary. Keep the existing installed version available for rollback.
4. On the device: check `--version`, open a disposable workspace/session, verify help/model selection, test one local authorized task and restart. Browser package installation and image transmission each require their separate consent. Record actual errors and resource use; do not assume the phone test passed.
5. Only after separate merge/release authorization: land the candidate, require all publish dependencies to pass, and publish the distinct `v0.8.0-alpha.1` prerelease with complete assets/checksums/notices. The installer then supports `--version v0.8.0-alpha.1`. That tag is not currently published and the normal installer still selects the existing release.

There is no need to delay a useful preview for full Alpha parity. The current gate is authorization to upload and run candidate CI, followed by a truthful device trial—not completion of every speculative feature. No push, merge, tag or release was performed during preparation.

## Final handoff correction

The user deferred device trial until the complete update is released; no immediate source/binary trial package is being distributed. The separately versioned `0.8.0-alpha.1` binary has now passed a fresh 21-group qualification, 24 deterministic runtime samples and 233 Rust tests. Its SHA256 differs intentionally from the earlier `cdb9406`/0.7.0-alpha.2 build. `validation/candidate-readiness.json` ties the new hash to both fresh qualification and runtime results, and identifies a fixed local copy outside the mutable Cargo target directory. Production `src/` is unchanged from the reviewed base; version metadata and delivery/CI documentation are the candidate-preparation changes. The `cdb9406` worktree remains clean and unchanged.

Next action for coordination: authorize upload of this exact candidate branch and a Draft PR to run candidate CI. That authorization does not imply permission to merge or publish. After successful candidate CI, obtain the separately scoped merge/release authorization; then distribute the actual release and arrange the first Termux device trial. No preliminary Linux artifact should be presented as an Android build.
