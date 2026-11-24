# Reliability review and reproducible validation

This change builds on Blue Dragon `237bb56c2c0fe9771f46ff022e8c1b0c9d44d505`
(v0.7.0-alpha.2). AlphaCode was inspected at
`773e3ba2f3e30e47d09cd5b751d224a1592dc804`. This is a bounded engineering
review and improvement set, **not full AlphaCode parity or a bug-free claim**.
No version bump, merge, or release is part of this change.

## Source comparison and decisions

| Area | Inspected reference | Blue Dragon decision / remaining limit |
| --- | --- | --- |
| Context | Alpha `src/alphacode_compaction_core/mod.rs`, `src/alphacode_app_core/agent/messages.rs` track compaction, recent turns and image budgets | Keep bounded, non-destructive projection and exact recall. Synthetic long-context tests retain tool arguments and user instructions. No semantic summarizer, model-quality comparison or unlimited context claim. |
| Memory/search | Alpha `src/alphacode_app_core/tool/session_search_index.rs` uses a cached token prefilter with revalidation; `src/alphacode_base/memory.rs` provides richer memory management | Keep small workspace/origin-scoped lexical notes and content-hashed source parses. Skip line tokenization for files with no requested symbol/reference. No embeddings, cross-product transcript indexing or type-resolution claim. |
| Persistence | Alpha `src/alphacode_base/session.rs` separates journal, crash recovery and persistence modules | Bound checkpoints to the same 16 MB limit as resume; preserve the prior checkpoint on error; validate filename/identity; reject symlinks; lock an active session against concurrent writers; buffer writes and sync before atomic replacement. No journal migration or background-job persistence. |
| Evidence | Blue `src/intelligence/checks.rs`, `src/reasoning/claims.rs` and native-tool tests | Preserve exact native result assertions and unknown-status recovery. Model prose, screenshots and exit zero do not establish semantic task correctness. |
| Browser/vision | Alpha README browser/desktop capabilities; Blue `src/browser.rs`, CDP and extension transports | Integrate one-shot JPEG interpretation with separate destination-specific consent, explicit model allowlist, bounded response and pinned derived history. No silent provider fallback or project-memory export from image sessions. No desktop automation, generic image-file reader or live vision quality claim. |
| Providers/tools | Blue `src/provider.rs`, `src/models.rs`, MCP and job regressions | Preserve credential isolation, explicit approvals and no retry of partially streamed actions. Loopback tests exercise image serialization, failure status, response bounds and truncation. Unknown vision routes fail closed. |
| TUI/platforms | Alpha `AGENTS.md` terminal guidance; Blue buffer and PTY tests | Keep compact phone layout and bounded event delivery; add nonblocking package-install progress. Do not import desktop GUI/swarm overhead into the phone workflow. Windows background jobs remain explicitly unsupported; ordinary commands remain available. |
| Installation | Blue installer safety/upgrade fixtures | Show each download and verification phase; TTY progress bar; connect/stall/attempt/retry limits; checksum and executable-version checks before replacement; preserve rollback. Browser package setup reports progress every five seconds without streaming raw package logs into the model. |

## Reproduce the runtime comparison

Build both revisions with the same Rust compiler and release profile. Save the
baseline executable before building the candidate. Then run:

```sh
python scripts/benchmark_runtime.py --candidate /path/to/candidate/blue-dragon --baseline /path/to/baseline/blue-dragon --repeats 3 --output runtime-comparison.json
```

The harness uses temporary workspaces, synthetic data, a local scripted HTTP
provider and an environment without provider credentials. It verifies exact
file bytes, native exit status, cache metadata, preserved arguments and absence
of replay. It does not delegate the pass/fail decision to a model. A deliberately
malformed checkpoint only points inside its disposable fixture directory.
This is not a network sandbox; the tested executable must be trusted.

Recorded Linux x86_64 run: Rust 1.100.0, release builds, three fresh runs per
case. Full binary/harness hashes and samples are in
[`validation/runtime-comparison.json`](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/blob/2b7dbc840c203c0b8093611eb281d8b09b037815/docs/validation/runtime-comparison.json).

| Case | Baseline passed/executed | Candidate passed/executed |
| --- | --- | --- |
| Atomic multi-edit | 3/3 | 3/3 |
| Invalid second edit preserves original | 3/3 | 3/3 |
| Denied write preserves original | 3/3 | 3/3 |
| Exit 7 despite success-looking stdout | 3/3 | 3/3 |
| Content-hashed symbol cache | 3/3 | 3/3 |
| Interrupted tool resume without replay | 3/3 | 3/3 |
| Archived arguments with assistant text | 3/3 | 3/3 |
| Stored session identity mismatch rejected before write/request | 0/3 | 3/3 |

Timings are local measurements, not performance promises: only three samples,
no CPU pinning, and checkpoint durability adds I/O. Do not infer overall speed,
model quality or parity percentages. Alpha has no built executable/validated
runtime adapter here and is explicitly **not run**, not counted as failing.
The same fixture protocol could be adapted to Alpha later without implementing
missing product behavior in an adapter.

## Validation performed

- 145 Rust tests pass, including consent, pin/reload, image payload/HTTP bounds,
  checkpoint capacity, identity, lock lifetime, corrupt-latest fallback,
  Unicode context, memory scope, evidence and background-job cleanup.
- `cargo clippy --offline --locked --all-targets -- -D warnings`, formatting,
  release build and `git diff --check` pass locally.
- Offline model selection/key isolation, actual PTY model picker/help/exit and
  terminal restoration, and PTY exit during an active job/provider wait pass.
- Installer fresh install/update/rollback, source-upgrade preservation and
  release-payload validation pass. Download timeout flags are checked by mocked
  curl; real mobile-network timing is not measured.
- Browser form/option-budget contract tests pass. Real local Chromium launch was
  attempted but blocked by the execution environment: `socket() failed:
  Operation not permitted` in Chromium process-singleton setup. CDP/extension/
  URL integration for this revision therefore needs CI or a permitted machine.
- CI adds the runtime regression suite on Linux, Windows and macOS, retains
  Android ARM64 cross-build and real Chromium/Firefox tests, and runs form and
  active-job shutdown regressions. Configuring CI is not a passing result.

## Remaining limits and follow-up acceptance criteria

1. Run the exact PR head in native Windows/macOS CI and the Android NDK build.
   Physical Termux installation, TLS/network interruptions, keyboard input,
   lifecycle/background restrictions and browser startup still need a device.
2. Run real Chromium/Firefox integration where OS sockets are allowed. Verify
   browser package progress on actual Termux; the package manager still has its
   existing 15-minute per-package deadline, and package installation is not an
   atomic rollback transaction.
3. With separately authorized provider credentials, measure vision perception
   against labeled screenshots. The current allowlist is an adapter boundary,
   not a guarantee of model availability. Image approval remains mandatory.
4. Run common model-driven coding tasks against both products with equal model,
   token/tool budgets, repetitions and independent workspace oracles. No live
   model performance or Alpha execution was measured here.
5. Session JSON remains bounded and fully read for resume; metadata listing
   avoids deserializing message trees but still scans files. Locks are advisory,
   apply to this application and depend on filesystem support. Concurrent hostile
   filesystem replacement is not an OS sandbox. Directory-entry crash durability
   and power-loss recovery are not guaranteed by syncing the temporary file.
6. Historical notes remain lexical and scoped; source snapshots intentionally
   exclude secrets, ignored/large/binary files. Successful checks are bounded
   evidence, not proof of complete coverage or security.

## Additional scoped-audit acceptance work

The later eleven-priority request is mapped explicitly in
[SCOPE_AND_AUDIT.md](SCOPE_AND_AUDIT.md), including implemented mechanisms and
remaining gaps. Added regressions cover large-source reuse after server closure,
cache tamper/session isolation, scope-before-network denial, path boundaries,
redacted/deduplicated coverage, one-field comparison, hypothesis requirements,
AST candidates, report privacy, and bounded readiness/frame behavior.

The earlier macOS PTY exit timeout is not declared fixed by a later passing CI
run. A concrete test-harness risk was reproduced: waiting for child exit without
draining its PTY blocks a child that writes enough final output. The smoke test
now drains while retaining the same five-second deadline and records exit timing
and a bounded terminal tail on failure. The independent backpressure regression
passed; ten consecutive Linux runs of the real PTY scenario passed. macOS
causation and native timing still require the exact-head CI result.
