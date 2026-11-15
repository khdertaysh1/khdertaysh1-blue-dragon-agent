# Task continuity and selected response-policy review

Local continuation from a851112, with no remote upload or release.

## Task continuity

The read-only task_continuity tool distinguishes unknown historical outcomes,
references to prior jobs, and jobs actually present in the current manager.
It never restarts a job or replays a command. A missing live job means only that
the current runtime cannot track it; it does not prove success, failure, no side
effects, or that every old process has terminated. Prior normal return and
model-reported plan completion remain separate from task correctness.

Work is bounded to the latest 300 transcript messages and observations, at most
20 unknown-result rows and 20 job rows. Tool messages above 64 KB are skipped
and counted. Unknown-row omissions and older-message count are exposed. Raw
commands, program arguments and output text are not included in the overview.
Original message indexes identify where details can be recovered.

scripts/smoke_continuity.py loads a synthetic interrupted checkpoint through the
real CLI and a loopback scripted model. It contains a historical running job and
an unfinished command that would create a marker file if replayed. The test
verifies the recovered unknown result index, no live job, no marker file, no
approval prompt, zero replayed commands, one open plan step and two model calls.
The smoke is wired into the future cross-platform CI runtime step, but this
branch has not been uploaded or run in remote CI.

## Selected response-policy module

response_policy_review is an additional explicit choice for review_web_evidence.
It consumes verified browser network observations or verified automatic browser
inspection, under the current user-managed read scope. It makes no requests.
It reviews at most 100 entries and 20 responses, skips outside-scope locations,
and reports scan limits. Raw URLs and header values are not exported.

The module distinguishes CSP frame-directive indicators from report-only
indicators, recognizes common X-Frame-Options values and nosniff, and labels
missing header metadata as unavailable rather than missing protection. These
are syntax/value indicators only: no full CSP parser, browser enforcement,
framing impact or MIME execution is inferred. The current extension backend
may not supply selected headers; those entries correctly remain unavailable.

Synthetic tests cover report-only policies, absent metadata, known values,
outside-scope entries and secret omission. Existing provenance/freshness checks
and hypothesis/next-test/confirm/refute output conventions are retained.

## Validation

179 Rust tests passed on Linux. Clippy all targets with warnings denied, release
build, formatting and diff checks passed. The final binary passed the continuity
CLI smoke plus the existing allowed, denied and repeated-control engine smokes
(1, 0 and 3 native GETs respectively; zero followed redirects).

Final binary SHA256:
d3c22aebe89f94aa12a0a7626a67e45485214003d44e9e065f62fcf3ef664b2c

No physical Termux, native Windows/macOS, live vision provider or graphical
browser enforcement test was run for this branch. Historical process recovery,
complete standalone replay, active multi-type scanners and semantic dataflow
remain unfinished. These results are not an Alpha parity measurement.
