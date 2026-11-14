# Agent control, context and evidence — 0.6 alpha

This revision improves the runtime around the selected language model. It does not ship new model weights, promise equal intelligence to another agent, or introduce a hidden paid model. Existing provider selection, approvals, browser setup and free-pool limits remain in force.

## Reference audit

AlphaCode was inspected at commit `815e169075e517f9cf729c7005e081d5f119e652`. Its task briefing, context projection/compaction and decision-loop components explain meaningful differences beyond the shared model gateway:

| Area | AlphaCode reference | Independent Blue Dragon implementation |
|---|---|---|
| Task guidance | `alphacode_base/prompt_enhance.rs` assesses task kind and verification hints | Fixed task-specific briefing for security/browser, diagnosis/fix and general engineering; direct questions need no tool loop |
| Context | `alphacode_app_core/agent/compaction.rs` and `agent/turn_loops.rs` manage provider context and proactive compaction | Budgeted request projection, full original transcript retained, exact bounded recall; no model-generated summary is treated as fact |
| Decision state | `alphacode_decision_core/agent_loop.rs` tracks recent tools/errors and continuation state | Canonical tool-argument signatures, unchanged-result detection, refusal/unknown-outcome repeat blocking, bounded stalled-loop stop |
| Verification | Reference prompt/runtime includes goal/verification guidance | Persisted observed-result IDs and typed finding citations; citation validity is checked, semantic truth is not automatically proven |
| Memory/indexing/orchestration | Reference has additional memory graph, retrieval, indexing and orchestration subsystems | These larger systems are not implemented by this revision |

Only architectural ideas were used. These new modules are independently written; no AlphaCode source was added. The existing small MIT protocol attribution remains unchanged.

## Context budget and exact recall

Default `--context-tokens 32768` reserves4096 reply tokens plus tool-schema and safety margins. Cached Kilo metadata can lower the limit for the active model without another catalog request. `/context` shows estimated usage and projection level. The estimate is a conservative character heuristic, not a model tokenizer; provider behavior can still differ.

When the request would exceed the budget, the engine first excerpts large observed outputs, then replaces older **complete** tool rounds with labeled historical digests. Recent tool-call/result envelopes remain paired. Original user requests and system instructions are not silently discarded. Only app-tagged automatic browser evidence is treated as excerptable user-role data; structured execution proof flags are retained. Untagged user messages are never mistaken for app evidence. Legacy sessions with untagged large browser packs may need a larger supported window or a fresh session. Large working notes are also bounded. The saved transcript is not rewritten or deleted by projection.

`recall_context` reads exact character ranges by original `message_index`, `offset` and `count` (at most12000 characters). It does not rerun commands, refetch sites or reuse stale output as a fresh observation. Compacted observations expose original indexes. No filesystem path or arbitrary other session can be requested through this tool.

Explicit pre-stream HTTP400/413/422 context-window codes allow **one** same-provider recovery per user turn with a smaller budget, down to8192. It is not free-model rotation and never replays completed tools. Interrupted streams, auth/shared-quota errors and generic request errors retain their existing stop policy. The learned smaller ceiling lasts for the current agent/model selection; it is not a guarantee that every provider context error is recognized.

If essential instructions or recent arguments still cannot fit, the task stops explicitly with the original history preserved. Larger windows require a model that supports them. Existing session loading remains bounded to16MB. This is not unlimited memory, a semantic memory graph or an embeddings search engine.

## Stalled loops and unknown outcomes

Tool arguments are compared as parsed JSON, so formatting/key-order differences do not bypass the guard. A denied or uncertain operation is not automatically attempted again in the same turn. Read/file/browser output is **not cached and returned as new evidence**: live reads can run again, but repeated identical observations prompt a different approach.

Three consecutive identical observations block another identical call. Three blocked attempts or eight repeated unchanged observations stop the turn after completing all tool-result envelopes. A new user turn resets the guard. This does not prevent every possible oscillation or semantically equivalent command; the existing step limit remains a final bound. It does not retry timed-out side effects merely because the model asks again.

An empty final response after tools receives one request to summarize the existing results; a second empty response fails explicitly. Tools are not rerun to manufacture a final answer. Plain questions do not get an automatic extra review/model call.

## Evidence and findings

Real tool outcomes receive IDs such as `E000003`, tied to the original transcript index. Automatic URL evidence is also registered. `/findings` shows recent observations and persisted findings. `record_finding` accepts:

- `hypothesis`: an idea not yet established;
- `supported`: a model interpretation citing at least one successful observed result;
- `blocked`: an unresolved issue, with limitations or failed/denied checks.

Invented evidence IDs and support consisting solely of failed results are rejected. Planning, finding storage and history recall cannot serve as fresh verification evidence. Up to40 findings are kept, with exact-summary updates supported. Findings and observation references survive resume; older session files load with empty default ledgers. The application-owned initial prompt is refreshed on resume, while user messages/skills stay intact.

**A valid citation is not proof that the interpretation is correct.** For example, reading a JavaScript sink does not establish exploitability, and a successful shell exit does not prove unrelated behavior. Browser execution canaries remain bounded and separately labeled; negative probes do not prove safety. Final reports include context/loop statistics, findings and unfinished plan counts, not an invented overall verification score.

## Verification

Offline integration fixtures exercise actual agent/provider/tool sequences: a denied write followed by an identical request, switching to a legitimate read, a long transcript with exact recall and retained user constraints, evidence citations surviving resume, empty-response continuation and same-model context-overflow recovery without repeating a completed tool. Existing URL-only browser fixtures and all platform CI checks remain release gates.

These tests establish runtime behavior, not a general model-quality benchmark. A fair model-quality comparison still requires repeated representative tasks with identical models/settings and independent scoring.
