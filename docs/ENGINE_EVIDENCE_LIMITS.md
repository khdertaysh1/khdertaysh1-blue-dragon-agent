# Selected marker review and incomplete form metadata

Local continuation from 4a597ba; no remote upload, merge or release.

## Explicit reflection review

review_web_evidence now offers reflection_review with evidence_id and marker.
It reviews an already paired, integrity-checked, nonhistorical web_fetch result
under current explicit read scope. The module makes no requests, submits no
forms and executes no JavaScript. Collection still uses the existing scoped,
separately approved web_fetch path; selecting the review does not approve a GET.

The marker must be 8-64 ASCII alphanumeric/underscore/hyphen bytes and equal a
decoded query value from the original request. Arbitrary response-only strings
and executable syntax are rejected. The scan is literal and case-sensitive,
bounded to 64000 UTF-8 bytes and 100 counted matches. Output contains a marker
hash and evidence pointer, not raw marker, URL, response excerpts or MIME value.

States distinguish literal marker observed, marker not observed in captured
text, and inconclusive incomplete body. Missing truncation metadata is treated
as incomplete. Missing body metadata rejects the review. HTML MIME is only an
indicator. No execution or input control is inferred, even if the marker appears
in apparent script text. The shared outer verdict remains hypothesis_review_only,
vulnerability_confirmed false and fresh_evidence false.

This is a bounded investigation aid, not an XSS detector or active scanner.
It does not decode HTML, build a DOM, parse JavaScript, trace semantic dataflow,
verify session/cookie context, or observe browser execution. A baseline and
variant with repeated control and separately verified browser behavior are
still needed before interpreting causality or impact. Marker absence is not a
claim of safety.

## Form metadata boundaries

form_csrf_review now exposes fields_scanned, fields_omitted and
 token_name_assessment for each in-scope form. Missing field metadata or a
60-field bound without a token match yields an inconclusive assessment. Complete
supplied metadata without a match says only not_observed_in_supplied_fields.
The token at position 61 regression test verifies that a bounded scan does not
imply absent protection. Input values remain unexported.

## Repeatable local validation

Run the existing offline Rust all-target test suite and:

    python scripts/smoke_review_engines.py /absolute/path/to/blue-dragon
    python scripts/smoke_continuity.py /absolute/path/to/blue-dragon

The engine smoke uses the real CLI, scripted loopback provider and local HTTP
server. Five new allowed cases return escaped HTML, JSON, static text, a complete
body without the marker, and a body truncated before its marker. The server
intentionally returns fixed bodies independent of query input: even coincident
marker presence must not claim reflection causality. Each allowed case makes
one approved GET. An additional denied case makes zero GETs. No redirect is
followed, and the module makes zero extra requests. All native observation
request/result digests are checked against saved paired messages.

188 Rust tests, Clippy all targets with warnings denied, formatting/diff checks
and release build passed on Linux. Nine engine CLI scenarios (three existing,
six new) and the continuity smoke passed. Machine-readable results and binary
SHA256 are in validation/engine-evidence-limits-smoke.jsonl.

No physical Termux, native Windows/macOS, graphical browser or live model/vision
provider was tested. CI already invokes the expanded engine smoke, but this
branch has not been uploaded and remote CI has not run. The six broader work
areas remain incomplete; these tests do not measure parity with Alpha.
