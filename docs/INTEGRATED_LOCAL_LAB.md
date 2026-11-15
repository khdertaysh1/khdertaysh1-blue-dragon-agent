# Integrated local marker workflow

`run_local_lab` adds an opt-in production workflow, not just another classifier. It executes baseline, variant and repeated-control GETs against one Between Bytes preview endpoint, persists progress before every dispatch, and returns a bounded report. It requires explicit user-managed **interact** scope and a separate approval displaying the exact target, budget, limits and no-retry rule.

## Use

Start a separately reviewed Between Bytes lab on loopback. This feature accepts only `http://127.0.0.1:PORT/preview/reflected-xss/vulnerable` or `/fixed`, with no credentials, query or fragment. It does not start or authenticate a server and cannot prove which program owns that port. Verify the local lab before approving.

In Blue Dragon, grant only the intended preview path:

```text
/scope allow http://127.0.0.1:8000/preview/reflected-xss/fixed interact
```

Ask the agent to invoke:

```json
{"action":"run","run_id":"fixed-marker-01","url":"http://127.0.0.1:8000/preview/reflected-xss/fixed"}
```

Tool name: `run_local_lab`. The approval covers exactly three harmless GETs with fixed ASCII markers and the lab's synthetic all-zero preview token. It sends no cookie/authentication headers, bypasses proxies, never follows redirects, explicitly disables client retries, uses a five-second timeout per request, and captures at most 64 KiB per response. No payload execution is attempted. At most twenty run checkpoints are admitted per session.

To inspect a saved run without network activity:

```json
{"action":"status","run_id":"fixed-marker-01"}
```

A repeated `run` with the same ID also returns the historical report, including after restarting Blue Dragon with `--resume SESSION_ID`. No completed, denied, interrupted, corrupt or outcome-unknown run is replayed. Corrupt reservations fail closed. A new run requires a new ID and approval; request repetition should follow a new user instruction. Partial runs remain available for inspection but are not automatically continued.

## Evidence and failure semantics

Checkpoints live under `sessions/lab-runs/SESSION_ID/RUN_ID.json`. An exclusive reservation precedes approval; each dispatch has a persisted unknown-outcome marker before the network call. Responses replace it with observed status, captured length, UTF-8/completeness flags, content-type/body SHA256, and presence of the two harmless markers. No response body, policy value, cookie or credential is saved by this workflow. The report contains the exact target, method, fixed inputs, sequence and limits needed to rerun deliberately; each step has a stable index inside the tool result, which receives the ordinary native evidence ID in the session.

The server may receive a request even when response reading fails. Such runs remain `request_outcome_unknown` and are never retried. A non-2xx, oversized or non-UTF-8 response stops the workflow with partial evidence. Cancellation/scope checks occur before the next request; an in-flight synchronous request is bounded by the five-second timeout. Native checkpoints survive the tested ordinary process restart; this is not a power-loss durability guarantee or protection against local file deletion/tampering. Hashes verify consistency, not authenticity.

The result is `differential_literal_marker_only`, `inconclusive_control_changed`, or `inconclusive_marker_not_differential`. Both weak and fixed lab pages produce the first result for harmless alphanumeric markers. That is expected: marker reflection does not discriminate escaped rendering from script execution. HTTP success, a completed workflow and matching repeat controls prove neither exploitability nor protection. Browser execution, account identity, browser cookie policy and server-state stability are unverified. The workflow neither injects synthetic browser evidence nor adds these substeps as fabricated standalone transcript tool calls.

## Feasibility and validation

The existing Chromium feasibility attempt failed before rendering (`socket() failed: Operation not permitted`, process exit -6). This continuation does not bypass that restriction or claim a browser run. It delivers the integrated HTTP portion with scope, approval, budget, progress, bounded evidence, partial reporting and persisted no-replay behavior. Genuine browser rendering/execution remains the blocker to the complete interactive browser path.

```sh
python3 scripts/smoke_lab_runner.py /absolute/path/to/blue-dragon --lab /absolute/path/to/Between-Bytes-Web-Labs
```

The test requires clean tracked lab code pinned to `c396c90f2e7ed080f71368ce9edb5cd630098b09`. It imports that separate checkout without modifying it and runs the actual Blue Dragon CLI/tool path with a scripted local provider. Test-only HTTP handler adapters inject a changed third response, a truncated body, or a connection close; these are labeled synthetic fault conditions, not properties of the unmodified application.

Seven cases passed: weak, fixed, unstable control, disconnect, truncation, read-only scope and denied approval. Total: eleven lab GETs; six cases reopen the saved session in a second process with zero repeated GETs (read-scope refusal creates no run). The independent server-side oracle records the actual emitted response status, bounded body hash, length, completeness and content-type hash and checks them against stored observations. It is independent of the Rust summary implementation; it is not a browser execution oracle. The Python socket guard does not sandbox the Rust subprocess; the production tool itself restricts the target, disables proxy/redirects/retries and counts attempts.

Validation: 208 Rust tests, clippy with warnings denied, release build, and the seven integrated CLI cases with server-oracle and process-restart checks. See `validation/integrated-local-lab.json`. No physical Termux, Windows/macOS runtime, live provider or graphical browser was tested. No external site or other person's account was used. No upload, merge or release is included.
