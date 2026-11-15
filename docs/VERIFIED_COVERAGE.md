# Verified bounded coverage view

Local continuation from 2876f27; no upload, merge or release.

The coverage command, audit_coverage tool and reproduction report now rebuild
coverage from verified recent native observations instead of trusting persisted
ledger counters. The old ledger remains readable for checkpoint compatibility,
but its counters are not accepted as current evidence by these user-facing views.
Original request/result pairing and available stored digests are rechecked;
historical/cached evidence and mismatches are skipped and counted explicitly.
Legacy pairs without digests retain their previously documented weaker guarantee.

Reconstruction uses the most recent 100 observations, at most 512 KiB per result
message and a 4 MiB result-message budget. It exposes older observations omitted,
checked candidates, invalid/unavailable evidence, historical records and skipped
oversized/budget-limited messages. This is a recent bounded view, not exhaustive
session coverage. The discovery walk retains its existing row/node/depth limits.

Rows distinguish unknown outcomes, blocked/not-executed results and successful
collection; partial capture is separately counted. Nested collector truncation,
known omissions and legacy form/field arrays at their cap mark a capture partial.
The partial scan itself is bounded to 5000 nodes/depth 12 and conservatively marks
unexamined data partial. Successful collection still does not mean a security
test passed. Each output row explicitly has security_test_completed false and
automatic_retry_allowed false.

Rows annotate current read scope, independently of their historical location.
If no scope was configured, that annotation is null rather than an authorization
claim. No request or retry is made. Query values are redacted before reconstruction
and existing redaction applies to discoveries. Paths and query names may still
need human review before sharing. Scope remains an action policy, not a firewall.

## Validation

196 Rust tests passed. New cases verify rejection of altered evidence, exclusion
of invented saved counters, reclassification after scope changes, separate unknown,
partial and blocked counts, and visible skips for historical/oversized inputs.

scripts/smoke_coverage.py resumes a synthetic checkpoint through the real CLI
with a loopback scripted provider. It checks two resulting locations: one partial
in-scope observation and one unknown out-of-scope outcome. A third mismatched
digest is rejected. No collector requests or approval prompts occur, raw query
values are not exported, and the model makes two calls. The smoke is added to the
future CI runtime step; this local branch has not been uploaded to run remote CI.

Clippy all targets with warnings denied, formatting/diff checks and release build
passed. The final binary passed coverage smoke, all 12 engine CLI scenarios and
continuity smoke. Results and SHA256 are in validation/verified-coverage-smoke.jsonl.

No physical Termux, native Windows/macOS, graphical browser or live model/vision
provider was tested. There is no automatic recovery or whole-session replay,
semantic trace completion, full coverage or Alpha parity claim.
