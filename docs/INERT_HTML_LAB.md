# Opt-in inert HTML encoding experiment

The integrated `run_local_lab` workflow now accepts `scenario: "inert_html"` in addition to the default `literal_marker`. This changes the actual input sent to the local lab: each of the three fixed markers is wrapped in `<blue-probe>...</blue-probe>`. There are no scripts, attributes, events, URLs or executable payloads, and arbitrary probe text is not accepted. The approval shows the selected scenario and all fixed inputs. Existing interaction scope, exact loopback preview paths, three-request budget, five-second timeout, 64 KiB bound and persisted no-replay policy remain in force.

```json
{"action":"run","run_id":"fixed-encoding-01","scenario":"inert_html","url":"http://127.0.0.1:8000/preview/reflected-xss/fixed"}
```

Grant that exact path with `/scope allow URL interact` and inspect the approval before running. A saved run ID always refers to its existing historical report, even if different scenario arguments are later supplied. A deliberate new experiment needs a new ID and approval.

Unlike an alphanumeric marker, the fixed custom element lets the local weak and fixed implementations produce distinguishable responses. The runtime records presence of the exact raw sequence and its exact angle-bracket-escaped sequence in each response. With differential markers and stable baseline/repeated-control status, content type and body hashes:

- Raw-only sequences yield `raw_probe_sequence_observed`.
- Escaped-only sequences yield `escaped_probe_sequence_observed`.
- Both/neither/mixed observations yield `inconclusive_mixed_or_missing_encoding`.
- Changed repeated controls yield `inconclusive_control_changed` before any encoding interpretation.

These are byte-sequence observations. The runtime does not infer DOM context: raw sequences could occur in comments, script strings or other inert contexts; alternative entity spellings may not match the exact encoding. Neither observation proves browser interpretation, execution, exploitability or protection. A fixed endpoint label is a fixture expectation, not trusted evidence for a verdict. `vulnerability_confirmed`, `protection_verified` and `browser_execution_observed` remain false.

## Independent oracle and sensitivity

Run `scripts/smoke_lab_runner.py BINARY --lab CHECKOUT` against the pinned clean Between Bytes checkout described in INTEGRATED_LOCAL_LAB.md. All scenarios invoke the actual Blue Dragon tool through its CLI and scripted local provider. The server independently records emitted body/status/type hashes and lengths. Python's HTMLParser separately counts `blue-probe` start tags and decoded text containing the literal tag. The weak fixture has an element; the fixed fixture has text. This parser is a test oracle for the synthetic responses, not a browser or HTML5 rendering engine.

A test-only response adapter supplies mixed raw/escaped output and another changes the repeated control. A sensitivity adapter deliberately removes encoding from the fixed response. The fixed-case assertion must then reject the result with the specific encoding-mismatch failure, corroborated by the independent parser finding a raw element. Other failures do not count as a successful sensitivity check. The original lab source is never modified.

The seven original integrated cases remain, plus four inert scenarios (weak, fixed, unstable, mixed), and one deliberate sensitivity failure. Eleven normal cases use 23 GETs; sensitivity uses another three, for 26 total. Ten saved-run cases reopen the same session in another process and issue zero repeat GETs; read-scope rejection creates no run. The sensitivity case intentionally fails before the restart check. No request follows a redirect or leaves the reviewed loopback target.

Validation: 209 Rust tests, clippy with warnings denied, release build, and the complete integrated matrix with sensitivity. Machine-readable results are in validation/inert-html-lab.json. The earlier validation/integrated-local-lab.json remains evidence of the original seven-case checkpoint, not the new binary. The graphical browser remains blocked by the previously recorded environment restriction. No browser execution, physical Termux, Windows/macOS runtime, live provider or external target was tested. This branch is local only.
