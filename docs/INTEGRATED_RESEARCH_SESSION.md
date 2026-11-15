# One integrated research session across subsystem boundaries

`scripts/smoke_integrated_research.py` exercises the actual Blue CLI, parent and two independent child model/tool loops, scope enforcement, HTTP capture, browser bridge transport, report revalidation and restart in a single synthetic scenario. The model endpoint and browser peer are deterministic fixtures. This is integration evidence, not autonomous-reasoning or browser-rendering evidence.

## Executed scenario and independent checks

1. Start the normal browser bridge through the user command. A WebSocket client with the required extension-origin header but an incorrect token must be closed. Pair the controlled synthetic peer using the actual user-visible token. The token must never occur in model requests.
2. Grant the parent read scope for one loopback fixture URL. Spawn two children with separate explicit file grants. A synchronization barrier holds both child model requests until the parent browser-open request reaches the peer, proving overlapping independent child requests and parent browser dispatch.
3. Each child reads its allowed source outline, then attempts browser access and the other child's source. All four unauthorized calls must be denied. The peer independently observes exactly one authorized parent open and one snapshot, and the HTTP fixture counts exactly one GET. The continuity report contains one accepted local-source reference per child and counts their two rejected observations each.
4. Clear user scope. Scripted fetch/navigation attempts must return failure without increasing either the HTTP or bridge command counters. No new grants are supplied by the model.
5. Explicitly grant mutate scope for the same synthetic loopback URL, then approve one click. The peer records receipt and closes before replying. The native result must remain `execution_status: unknown`, with `automatic_retry: false`. This proves dispatch/uncertainty handling, not that a DOM click occurred.
6. Exit and resume the same parent session. Browser status must be not ready with reconnect pending. `/audit-report` and `/report` preserve accepted child evidence without contacting any provider or reconnecting the browser. While this resumed process stays open, remove a child's stored result pin; the next report must exclude it and increase the unverified count. Restore the checkpoint and verify acceptance returns. All provider/HTTP/bridge counters stay unchanged throughout reporting/revocation after restart, and the dispatched-click count remains one.

Approvals remain enabled. The fixture answers exactly the expected five read-phase action prompts and one explicitly authorized click prompt; any extra approval request fails the test. Children receive no browser, network, shell or write permission. Read scope and mutate scope are separate user commands, never a model-supplied widening.

The WebSocket fixture uses Python's standard library and actual masked client frames, handshake validation, required capability declarations and secret authentication. It does not patch product code, bypass the pairing gate or install dependencies. Request bodies, frame sizes, child waits and CLI waits are bounded. The scenario is included in normal cross-platform CI and local qualification, but a local pass is not evidence that remote platform CI has run.

## Browser preflight and unresolved limits

An actual existing `scripts/smoke_browser.mjs` preflight was attempted with the pinned binary. Chromium failed before `DevToolsActivePort` appeared: `socket() failed: Operation not permitted`, followed by a ptrace permission error. No restriction was bypassed, no installation was attempted, and the test was not counted as passing. Log: `/tmp/blue-integrated-browser-preflight.log`.

The synthetic browser peer returns controlled page/location data. It does not run Chromium, extension JavaScript, DOM navigation, page scripts, network subresources, managed-tab isolation or a physical device. It cannot prove actual click effects or visual correctness. Browser/user scope is not a network firewall, and an extension-origin header alone is not browser identity. The actual pairing token is still mandatory in this test.

The reference review consulted Alpha `773e3ba2f3e30e47d09cd5b751d224a1592dc804`, particularly session-owned tab pinning in `alphacode_app_core/tool/browser.rs` and coordinator/ancestry/report ownership in `alphacode_app_core/server/swarm.rs`. Blue's chosen boundary remains one parent browser owner with read-only local children. This scenario checks that existing boundary under concurrent work and failure; it neither implements Alpha's wider swarm semantics nor benchmarks equivalent multi-agent/browser behavior.

Rust product code is unchanged from the reviewed inspection extension. The added work is an integrated behavioral qualification covering interactions that separate unit/comparison fixtures did not establish. No broad parity, autonomous intelligence, OS isolation, real-browser success or live-provider quality is claimed. No push, merge, tag or release occurred.

## Recorded local result

Harness commit `3bfb51c` on `test/integrated-research-session`: all 31 local qualification groups passed, including the new integrated scenario and 24/24 existing deterministic runtime regression samples. All group-log hashes and the pinned binary hash were verified. The scenario proves two simultaneous child requests during parent browser dispatch, four child permission denials, exactly one GET, exactly one click dispatch with unknown outcome, scope-revocation prevention, live report tamper/recovery, and zero provider/network/bridge replay during resume/reporting.

[Structured evidence, including the blocked real-browser preflight](../validation/integrated-research-session-readiness.json). Logs: `/tmp/blue-integrated-research-qualification`. Binary remains `blue-dragon-0.9.0-delegate-inspect-linux-x64`, SHA256 `e4445922e5fe4940a3ff0a4b7c01c2fd9c7e8b64e42d58e7c4529b984d53313b`. Product Rust source is unchanged; no new Rust/Clippy run is claimed for this harness/CI/documentation-only change. The preceding inspection feature passed independent review; this combined fixture is pending review. The package is not fully platform/browser/device qualified and has not been published.
