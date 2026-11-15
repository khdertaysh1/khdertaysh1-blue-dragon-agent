# Local coverage, module links and selected review engines

This branch builds on 9471e4f and is not uploaded. No dependencies were added.

## Review correction

The reported stale-assertion inconsistency was reproduced: the report accepted
reused_local_copy:true while marking the result historical. A single shared
is_historical policy now governs audit comparisons, coverage, native-fact
extraction, and newly created observation success flags. Regression tests cover
cached, cached_result, fresh_evidence:false, reused_local_copy,
reused_evidence_id and original_evidence_id. Historical HTTP status cannot
revalidate the saved assertion in the exported report.

## Source and coverage

Import links now include bounded named/default/namespace bindings and re-export
aliases, source offsets and a location hash. A three-file local fixture resolves
an explicit relative import, while a bare package and absent file remain
unresolved. Escaped specifiers are not guessed. Limits: 80 module edges per
analysis, 32 bindings per edge, 128 visited binding nodes. Existing overall
parse/traversal/source byte bounds remain. These are syntax and cache links,
not proven interprocedural flow; import maps, shadowing and sanitizer behavior
remain unverified.

Coverage records retain successful and blocked observation counts, avoid
recounting the last eight observation IDs per location, and mark array/node
truncation. Discovery walks have a 5,000-node budget in addition to existing
entry/depth bounds. Older checkpoint rows initialize the new counters at zero;
those counters describe newly recorded history, not reconstructed older totals.
URL/query-value collapse and the eight-ID window remain explicit limitations.

## Selective modules

review_web_evidence requires an explicit current user scope and one engine plus
one paired evidence ID. It refuses denied/unknown/historical evidence. It makes
zero requests and performs no mutations. It does not run all modules by default.

- form_csrf_review: examines at most 20 forms and 60 field names per form from a
  browser snapshot. Out-of-scope actions are skipped. A token-like name is only
  an indicator; missing names do not prove missing protection. Values and field
  names are not exported. Server state, SameSite and origin checks are untested.
- redirect_review: examines native web_fetch status and redacted redirect
  metadata. web_fetch still follows no redirects. Status or cross-origin
  redirection alone is not proof of controllable destination or security impact.

Each module includes the evidence ID/hash, bounds, hypothesis, next controlled
check, and confirming/refuting conditions. These are passive hypothesis-review
modules, not complete active vulnerability scanners. Active comparison steps
remain manual and require appropriate scope and separate approval.

The existing DOM canary now additionally requires explicit enabled interact
scope and separate approval for the selected bounded check. At most two variant
navigations and one restoration are allowed by this code path. A visible stable
body criterion replaces a fixed sleep; this is not React completion. Proof is
not accepted after a changed document URL or out-of-scope location. Duplicate
query parameters change only one occurrence. Page-generated subrequests are not
network-firewalled or counted by the navigation bound.

## Validation

165 Rust tests passed on Linux. Clippy all-targets with warnings denied, formatting,
diff checks, shared browser form/readiness/frame contracts and JavaScript syntax
check passed. The real loopback HTTP redirect test verifies no follow and query/
fragment redaction. Consent tests have no browser connected and prove rejection
occurs before any browser action. The full real-browser smoke fixture was updated
for the extra approval but has not run for this branch in this environment.

Release build passed. The unchanged eight-case runtime harness ran three repeats:
candidate 24/24, preserved main baseline 21/24; the baseline identity-validation
case fails three times. This measures those eight runtime cases only, not model
quality or Alpha parity. Machine result: docs/validation/coverage-flow-runtime.json.
Candidate binary SHA256: 566476de8e7f5e8168dfc4fa5881254906c11290f790ed6c90906c8b67555b31.

## Reference comparison and remaining work

The inspected Alpha reference includes a research observation checkpoint in
src/alphacode_app_core/agent/prompting.rs that asks for uncertainty, evidence and
a discriminating next observation. The selected modules use explicit versions
of those questions. Its bundled async_fuzz.py includes concurrent URL checks,
a status/grep hit heuristic, automatic redirects and disabled TLS verification;
those choices were not copied into this lightweight review workflow. These
source observations do not constitute a benchmark of Alpha behavior.

No physical Termux, native Windows/macOS, live-provider vision or new remote CI
has been run for this branch. Full active multi-engine testing, semantic dataflow,
a self-contained replay report and browser-session restoration remain unfinished.
