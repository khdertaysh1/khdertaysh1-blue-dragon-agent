# Inspect a retained child result without replay

The root supervisor can now use `delegate_agents` with `action: "inspect"` to read a bounded portion of an existing child's native tool result. This fills the gap between collecting evidence IDs/hashes and inspecting the actual source result. It extends the existing tool instead of introducing another agent, session store or general transcript search.

Supply `agent_id`, `evidence_id` and `expected_result_sha256` from the collected child evidence. Optional `byte_offset` defaults to zero; `byte_count` defaults to 4096 and must be 1..8192. Follow `next_byte_offset` until `eof`. Offsets count UTF-8 bytes in the canonical serialized native-result JSON, not original source-file bytes. The returned full-result hash is not an excerpt hash.

The child must already belong to this manager's team and the evidence must be retained. Inspection reads only its bounded checkpoint (maximum 256 KiB), checks identity/workspace and both stored pins against the retained references, and invokes the existing audit pair validator. Missing or changed pins, ambiguous evidence IDs, unavailable/invalid checkpoints and incorrect expected hashes fail. Native result size is capped at 24 KiB. Non-boundary offsets, out-of-range offsets and windows too small for the next UTF-8 character fail rather than corrupting text or returning a non-progressing window.

The result is explicitly historical and untrusted data, with `recorded_outcome`, `actions_replayed: 0`, and no semantic/current-source verification claim. Validly pinned failed, denied and unknown results are available as diagnostics with those outcomes preserved; they are not promoted into successful evidence. This differs deliberately from the report evidence gate, which excludes those results from its accepted set.

No child model call, tool execution, source-file read, browser reconnection or automatic retry occurs. Calling the tool through the parent model naturally uses ordinary parent model turns; the inspection implementation itself performs checkpoint reads only. The child request/output budget is not charged again for reading a captured result. Ordinary parent turn/context limits remain in force. Children cannot invoke `delegate_agents`, inspect another team or widen their grants. The API does not return child prompts, summaries, tool arguments or arbitrary transcript messages.

Captured tool content may still include sensitive text allowed by the original grant. It is not automatically redacted, and an untrusted-content label is not prompt-injection immunity. Whole-store forgery and external filesystem races remain outside the consistency guarantee. Requiring matching pins does not prove a model conclusion or that the project still has those bytes.

## Validation and Alpha comparison

Rust tests reconstruct an Arabic/emoji result across small windows and verify pinned reconstruction, unchanged child quota, bad/missing pins, unknown team/evidence, invalid offsets/counts, oversized results, workspace mismatch and preserved diagnostic outcomes. Negative size/workspace tests assert the specific failure reason. The actual CLI fixture successfully inspects after deleting the original project source, rejects a wrong result pin and verifies no additional child provider requests. Existing report/tamper/recovery tests remain in the integrated suite.

Alpha reference `773e3ba2f3e30e47d09cd5b751d224a1592dc804` exposes broader past-session retrieval in `alphacode_app_core/tool/session_search.rs`, with query/limit/source controls. Blue already has parent `recall_context`, child checkpoints and source reports. This addition reuses those stores for the narrower missing operation: explicit, pinned inspection of known child evidence. It is not equivalent to cross-session search, semantic memory retrieval, arbitrary transcript browsing or a benchmark of Alpha's recall quality.

The reviewed report branch remains unchanged. No publication or new permissions are part of this extension. Independent review and real phone/browser/live-provider validation remain required.

## Recorded local qualification

Tested source `087bf01` on `improve/delegate-evidence-inspect`: 285 Rust tests, strict Clippy, offline locked release build and all 30 integrated qualification groups passed. The existing deterministic runtime regression passed 24/24 samples. All group-log hashes and the pinned binary hash were verified. [Structured results](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/blob/2b7dbc840c203c0b8093611eb281d8b09b037815/validation/delegate-inspection-readiness.json). Logs: `/tmp/blue-delegate-inspect-qualification`.

Pinned binary `blue-dragon-0.9.0-delegate-inspect-linux-x64`: 18,356,528 bytes, SHA256 `e4445922e5fe4940a3ff0a4b7c01c2fd9c7e8b64e42d58e7c4529b984d53313b`; retained in `/workspace/scratch/58a10643c600/candidate-builds`. The preceding report integration passed independent review; this inspection extension remains pending independent review. No remote publication occurred.
