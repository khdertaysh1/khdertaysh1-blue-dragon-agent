# Controlled literal-marker comparison

Local continuation from 446233a; no upload, merge or release.

review_web_evidence now offers reflection_controlled_comparison. Arguments are
engine, evidence_id (baseline), variant_evidence, repeated_control_evidence,
marker (baseline), and variant_marker. This is a passive, opt-in review of three
existing paired web_fetch observations; it sends zero requests itself.

The engine requires distinct chronological observations with verified pairing,
recorded integrity metadata where available, usable freshness and current
explicit read scope for each source. The only changed normalized argument must
replace one query value equal to marker with variant_marker. Repeat control
must exactly retain baseline normalized input. Markers use the existing harmless
ASCII restrictions and cannot equal or contain one another. Wrong-factor changes,
reused evidence or duplicate references are rejected.

For classification, captures must be complete successful 2xx bodies with available
content-type metadata. Baseline/repeat body, status and content type must match;
variant status and content type must also match. The matrix then checks baseline
marker alone in baseline/repeat and variant marker alone in variant. Its strongest
classification is differential_literal_marker_only. Static dual-marker content,
changed control, changed response kind and incomplete/non-success metadata remain
inconclusive. This conservative rule may reject useful but dynamic pages rather
than attributing unrelated changes to the test.

The review exports evidence references, marker hashes and boolean observations,
not raw markers, response bodies or URLs. Existing literal scan budgets apply.
It never parses HTML, renders a DOM or executes JavaScript. All outcomes retain
execution_observed false, causality_proven false and vulnerability_confirmed
false. Repeated body equality does not prove stable session/account/server state.
Differential escaped text or JSON is not an XSS finding. Separately authorized
browser behavior and attributable impact still require observation and review.

## Validation

193 Rust tests passed. New cases cover escaped differential output, static text,
unstable repeated control, truncated captures, wrong changed factor, duplicate
references, historical results, and changed/missing response metadata. Altered
fixture responses are recorded as new matching observations to test comparison
logic without bypassing evidence integrity.

The real CLI smoke was expanded with three local HTTP fixtures: escaped text
that changes with the marker, constant text containing both markers, and a
changing repeated control. Each uses three separately approved GETs and seven
scripted model calls, without following redirects or reusing the baseline cache.
The module sends no additional request. It reports respectively differential
literal marker only, inconclusive marker pattern and inconclusive control.

All 12 engine scenarios plus continuity pass on the final release binary; the
latter replays zero commands. Results and binary SHA256 are recorded in
validation/controlled-marker-smoke.jsonl. Clippy all targets with warnings denied,
release build, formatting and diff checks passed.

No physical Termux, native Windows/macOS, graphical browser or live model/vision
provider was used. The loopback model is scripted. The broader browser, semantic
source tracing, standalone replay and provider work remains incomplete; this is
not an Alpha parity measurement or proof of vulnerability.
