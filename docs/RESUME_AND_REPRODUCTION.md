# Local recovery and reproduction follow-up

Based on local follow-up 84fb5c4, not uploaded to GitHub.

## Changes

Recovery now remaps observation positions and acceptance-check positions when
missing tool envelopes are inserted before later saved messages. Evidence IDs
remain stable; unknown actions are never executed by recovery. Invalid positions
are not rebound to unrelated future evidence. The new regression test fails on
84fb5c4 (stored index 3 points at a request instead of the expected result at 4)
and passes after the correction. Repeated recovery is idempotent.

Report schema 2 adds per-entry original-argument hashes, request/response message
references and a six-step manual reproduction checklist. No original argument
values or response bodies are exported by this addition. Automatic replay is
always false; stale/denied/unknown evidence and missing paired requests receive
explicit blocked states. This is a local-evidence-assisted manual workflow, not
a self-contained exploit reproducer. Existing exported URL paths may contain
sensitive data and still require human review before sharing.

Report native facts are extracted again from paired results. Stored metadata
mismatches are exposed. Structured finding assertions are checked again against
these facts, with historical verification labels separated; prose and security
impact remain unverified. Export is bounded to 100 latest evidence records and
100 latest findings, with omitted counts. A hash detects differences relative
to this saved transcript, not malicious alteration of the entire checkpoint.

Source tracing keys candidates by start/end/kind, preserving nested calls such
as factory(input)(other). Context compaction retains explicit cached/freshness
and source-evidence markers so a shortened response does not hide its age.

## Remaining limits

No background process is resurrected after restart, no browser login/session is
recreated, and no unfinished side effect is retried automatically. Recovery
updates authoritative observation/check indexes; embedded historical text and
past tool payloads remain unchanged. Use current observation indexes rather than
old textual position hints after repaired history. Automatic URL inspection
still lacks a paired original request and is labelled unavailable. Cross-file
tracing remains syntactic rather than interprocedural taint analysis. These
changes require separate cross-platform CI if subsequently authorized for upload.

## Local validation

156 Rust tests passed with cargo test --offline --locked --all-targets on Linux.
Clippy passed for all targets with warnings denied; formatting and diff checks
passed. The baseline recovery failure log is retained outside the repository at
/workspace/scratch/58a10643c600/resume-index-before-fix.log. No new provider or
physical-device tests were performed in this follow-up.
