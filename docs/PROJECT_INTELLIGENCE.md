# Project intelligence — 0.7 alpha

This is an implementation comparison, not a claim of equal model intelligence. The selected model's weights, context limits and tool-calling ability still matter. No percentage of AlphaCode parity is asserted.

## Reference inspected
AlphaCode main at [`2e1db4018f65c12429c8d113aa469617836e5873`](https://github.com/dragonked2/alphacode/tree/2e1db4018f65c12429c8d113aa469617836e5873) was inspected, including `src/alphacode_task_types/goal_contract.rs`, `src/alphacode_app_core/tool/session_search_index.rs`, `src/alphacode_app_core/memory_manager/persistence.rs`, memory-graph/project-analysis modules and verification quality logic. The findings motivated these three independent additions. No new AlphaCode code was copied; the pre-existing small MIT JSON-RPC envelope remains credited in third_party/NOTICE.md.

| Gap addressed | Blue Dragon implementation | Remaining limit |
|---|---|---|
| Finding relevant code | `code_search`: ranked local text/path retrieval, Tree-sitter declarations, scopes, imports and syntactic references | No embeddings, resolved symbol binding, semantic cross-file graph or runtime call graph |
| Remembering previous investigations | `project_memory`: bounded local notes, current-session evidence references qualified by source session, project/target isolation | Historical model notes can be wrong or stale; new observations must revalidate them |
| Checking a completion claim | `set_task_checks`: exact tool/input/native metadata, source freshness, stale evidence invalidation and one review continuation | The model can choose weak criteria; a matching exit code is not semantic proof |

## Search and coverage
Supported structure parsers are Rust, Python, JavaScript/JSX and TypeScript/TSX, using the public MIT Tree-sitter grammars. Other eligible text files use lexical retrieval. Comment text may be a code-text search hit but is never fabricated as an AST declaration. Camel-case and underscore identifier queries match exact names in symbol mode; natural-language queries use token ranking.

The walker honors ignore rules, excludes hidden/secret files and generated dependency/build directories, does not follow links, and bounds work to 500 files, 4,000 walker entries, depth 12, 128KB per file and 12MB total. A refresh checks a three-second deadline between entries; each parser has a 70ms cancellation deadline, 30,000 AST nodes and 2,000 extracted items. This is bounded best effort, not a hard real-time deadline. Results expose parser errors, structural truncation and file coverage. Hashes detect equal-length edits without depending on modification timestamps. Deleted files leave the cache. No source execution, remote embedding API or index file inside the repository is required.

The freshness hash covers the eligible text snapshot, not ignored files, binary assets, services, runtime state or the entire filesystem. A matching snapshot does not prove every task dependency stayed unchanged. Concurrent hostile filesystem changes are outside the trusted-workspace boundary.

## Historical project notes
Notes live under application data in `project-memory/<sha256-canonical-root>.json`. The canonical workspace and explicit HTTP(S) origin scope must match. URL targets embedded in text are recognized; follow-up turns and resumed sessions retain that target, while multiple explicit origins have a separate combined scope. A new session without a URL uses project scope. A model navigating outside the explicit task scope does not change that scope automatically; scope is provenance metadata, not a browser sandbox.

Only accepted findings from the current turn are automatically queued; the tool may also explicitly save a note. Findings/lessons require real current-session source references in the matching scope; decisions/open questions may have no references. Notes are model-authored historical guidance, never current evidence or permission. A saved source is `{session,evidence_id,message_index}`; an old `E...` is not promoted into this session. Up to 200 notes, 2,000 bytes each and eight references each are allowed. Full capacity causes an explicit error, not silent deletion. Malformed/oversized files fail explicitly. Writes take a bounded cross-process lock and replace the file atomically; Unix files use private permissions. This protects ordinary concurrent writers, not a malicious local user with equal access.

`/memory [query]` searches the current scope; `/memory URL` looks up that origin; `/memory forget ID` deletes a note. `--no-project-memory` disables note retrieval and saving for that process. Sessions still persist normally. Relevant retrieved notes can be sent with the conversation to the selected model provider, like other project context. Search is lexical, not semantic. No arbitrary search across all private session files is added.

## Acceptance checks and finalization
`set_task_checks` declares at most six exact checks before their tools run. `run_command` supports native `/exit_code == 0`; `file_info` supports `/bytes`, `/directory` and `/readonly`. Stdout banners, fetched content, memory and model assertions cannot satisfy these fields. A declaration never runs a tool or grants approval. Changing the contract clears previous proof. Only later native results with matching tool and exact arguments qualify. Failed, denied, unknown or repetition-blocked outputs cannot qualify.

A source mutation clears previous observations. Source hashes are refreshed before guarded tool calls and finalization; resume clears old proof. An unobserved/stale contract, or edits without a contract, causes one extra completion-review request. If still unresolved, the engine emits `needs_verification` and labels the answer accordingly. There is no automatic replay of tools or denied commands. Responses during this verification phase are buffered so the provisional final answer is not streamed as verified completion.

`/checks` shows the latest observed snapshot; `/report` refreshes acceptance state at task completion. `finished:true` records a normally returned turn, not acceptance. `task_status` distinguishes `declared_checks_observed`, `needs_verification`, `answer_returned_no_acceptance_contract` and `interrupted`. Matching the declared checks does not establish overall task correctness: model criteria may be too weak, syntactic checks do not prove behavior, and broader user acceptance still requires review.

## Still missing
A resolved cross-file code graph, semantic embeddings, broader historical session search, isolated patch worktrees, coordinated workers, richer structured test/coverage evaluation, stronger crash recovery, wider MCP transports, extension signing and physical Android validation remain outside this release. The free-model router and browser are retained; this upgrade improves orchestration around the model rather than replacing it with a stronger model.
