# Private command diagnostics and tool-specific recovery

Based on published alpha.6 main `eb53e0aa5367c6bab8c2e78cbaa4b27acca5b5ff`. No version/tag/asset change or publication is part of this repair.

## Established causes and unknowns

The reported phone sequence is run_command invalid arguments, successful discovery, then discovery names size rejection. The original phone session is not available in this workspace or the accessible message history. Neither an empty array nor an oversized array is asserted as its actual cause. The previous raw curl payload supplied in conversation is a different earlier observation, not evidence of the new call's contents. Why this live model ignored the supplied payload or chose discovery cannot be established from the three outcome lines.

Two code defects are reproducible: private_view redacts every diagnostic string, and Agent::run used one invalid-argument counter for every tool in the turn. A command error followed by a discovery error exhausted this shared counter before either tool had its own corrective opportunity. Successful discovery already did not remove a pending run_command error; that property is retained and tested explicitly.

## Repair

- tool_arguments::diagnostic_view exposes only fixed diagnostic vocabulary, registered native field paths, and numeric bounds/counts. Arbitrary error and retry strings are never echoed; readable text is reconstructed. Unknown fields/types remain redacted. Native discovery has its own schema and is included explicitly.
- Array bound violations record actual_count, minItems and maxItems. No count is inferred from legacy error prose. original_argument_shape independently reports types and array cardinality directly from the saved original call, including old sessions; it does not execute or call a provider. Original argument scalar values, including numbers, remain hidden. Result numeric metadata retains the established display policy.
- Correction counters are per tool per turn, at most two invalid calls per tool, with explicit attempt/limit metadata. Unchanged invalid calls and the existing overall step limit remain bounded. Only a successful uncached known result of that same tool clears its pending error. Discovery alone cannot turn a failed command into success; denial still executes nothing.
- The original calls remain unchanged. Approval sees the effective command as before; normalization grants no consent. No shell guessing, model rotation or larger production step limit.

## Verification and limits

`scripts/smoke_diagnostic_recovery.py` reproduces the shared-counter stop against the saved published-source alpha.6 binary using two explicitly hypothetical fixtures (zero and nine names). The repaired five-step CLI sequence corrects both tools, obtains approval, executes real curl HEAD to a loopback server exactly once, and verifies native headers/exit status. Separate denial and discovery-only cases establish no HTTP request and no false completion. Saved-result inspection is tested without provider calls or replay. The script is included in CI and local qualification.

Unit tests check counts, JSON types, schema paths, arbitrary/forged error text, secret field names and original numeric values. Existing command tests cover normalization, unchanged malformed repeats, streaming, truncated calls and narrow PTY approval/denial. An early unit test exposed an omitted discovery schema in the diagnostic renderer; it was corrected and CLI assertions strengthened to require actual readable strings rather than mere field presence.

`scripts/live_curl_trial.py` separately runs an opt-in free-model trial with an exact allowlist for one local curl HEAD request, a five-step limit, no rotation and no paid fallback. The local server independently counts HEAD requests and emits a fixture header. Native success requires completed, uncached execution, HTTP/1.0 200, that header and exit code zero. Only this local command can be approved. Recorded live results do not establish the inaccessible phone failure is fixed, external website behavior, or sustained model/service reliability. No physical Termux/Windows/macOS or real-browser run is claimed here. Final commit/binary/test identities are recorded separately for review.

## Independent review correction: native reads without ok

Review of 2450357 reproduced path=42 followed by a successful app.js read but final failure: read_file/read_file_window return native content/hash contracts without an ok flag. The correction predicate now validates those original native contracts (hash, and window offsets/length/EOF where applicable) rather than requiring ok=true. It still rejects errors, denied/failed/stale/cancelled results, unknown/noncompleted execution, cached/historical references and malformed or corrupted read contracts even with ok=true. Other tools without a recognized contract are not inferred successful from an arbitrary object.

The same-tool unresolved set remains independent: a successful read_file cannot resolve an invalid read_file_window and vice versa. Eight real CLI cases cover both successful corrected reads and wrong-tool, missing-file and denied-secret-path negatives; the original failure is reproduced against the saved 2450357 binary. Direct Rust negative controls cover stale, unknown, cached and altered-hash/window metadata without a production result-injection hook. These tests do not establish a file remains unchanged after its native capture.
