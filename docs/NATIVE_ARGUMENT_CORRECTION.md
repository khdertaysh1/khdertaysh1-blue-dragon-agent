# Native argument correction contracts

The regression reproduced on `3e298d5`: `search_file(path=42)` fails validation, then the same tool with a valid path searches successfully without an `ok` field. The root agent previously retained the unresolved error and exited 1. The earlier read-only fix did not cover other native contracts.

## Runtime change and trust boundary

`Agent` now carries an in-memory `CorrectionOrigin` from the actual dispatch. Invalid input, cancellation before dispatch, loop-guard/cached replies and native `Err` cannot claim native completion. `tools::invoke_result` preserves the native Rust `Result`; the existing `invoke` JSON adapter remains available to other callers. Model text and persisted JSON cannot set this origin. `correction_succeeded_from_dispatch` then checks the tool's original contract and rejects explicit failure, denial, unknown execution, stale or historical/cached results. Removing a pending error remains keyed to that exact tool name.

This is successful execution of corrected arguments, not proof of complete investigation, vulnerability, current source identity after the call, or model reasoning quality. Partial bounded searches can successfully correct arguments while still explicitly reporting incomplete coverage. No tool is replayed to validate this predicate.

| Native tool | Contract checked after actual successful dispatch | Implementation |
|---|---|---|
| read_file | content digest and source path | tools.rs / read_file |
| read_file_window | content digest, input path/offset, UTF-8 byte length, window/EOF/continuation | local_files.rs / window |
| search_file | input path/offset, ordered exact query-byte match ranges, scan and match limits, completion/continuation consistency | local_files.rs / search |
| list_files | bounded sorted string list and truncation flag | tools.rs / list_files |
| read_lines | input path/start, sequential line numbers, bounded text rows | tools.rs / read_lines |
| file_info | input path, byte count, directory/readonly booleans | tools.rs / file_info |
| search_files | bounded path/line/preview rows and truncation flag | tools.rs / search_files |
| git_status | checked subprocess exit, native text payload | tools.rs / git_status |
| git_diff | checked subprocess exit, native diff text | tools.rs / git_diff |
| project_map | bounded counts, language totals, paths and truncation metadata | intelligence project map via tools.rs |
| file_outline | source digest format, source bound, syntax metadata and pagination | intelligence/code.rs / outline |
| browser(action=status) | native backend-specific status contract; ready:false is a valid status observation, never DOM inspection or browser readiness | browser.rs / status |
| list_jobs | bounded retained job list with native job identities | jobs.rs via tools.rs |

`git_diff` previously represented subprocess failure as successful text. It now returns `Err`, matching `git_status`. Git status text is payload, not an execution-status enum: a filename containing `failed` must not invalidate successful status execution.

Other native action contracts normally supply explicit `ok`. Historical reports (including `audit_coverage`, `task_continuity`, memory and web-evidence reports marked `fresh_evidence:false`) are deliberately not promoted to fresh corrective evidence. External MCP responses still require explicit success; their JSON cannot impersonate native completion.

## Verification and limits

`scripts/smoke_native_correction_contracts.py` uses a scripted loopback provider and the actual CLI/native tools. It verifies all thirteen successful corrections, wrong-tool isolation, missing/denied paths and failed Git subprocesses. It independently inspects file contents, match offsets, line contents, digests and results; assistant prose is not an oracle. `--expect-regression` reproduces the old search failure. Rust controls use actual native results, then inject failure/cache/status metadata or corrupt contracts; they also test UTF-8 match limits and partial scans.

Directory/project traversal remains bounded and may skip inaccessible descendants under the existing policy; successful execution does not certify exhaustive coverage. Lexical/parser output is not runtime dataflow. No filesystem race isolation is claimed. The user's newer original Termux session payload is unavailable in this workspace. These fixtures are not a live model, real phone, Windows/macOS or real-browser qualification. The published version and release assets are unchanged; runtime changes require independent review before integration.
