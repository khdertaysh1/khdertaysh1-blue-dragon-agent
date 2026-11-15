# Opt-in local web lab matrix

This matrix exercises a separate checkout of `khdertaysh1/Between-Bytes-Web-Labs` pinned to `c396c90f2e7ed080f71368ce9edb5cd630098b09`. It does not clone, edit, push, or publish that repository. Run only against this reviewed synthetic lab revision. The tracked checkout must be clean.

```sh
python3 scripts/lab_matrix.py /absolute/path/to/blue-dragon --lab /absolute/path/to/Between-Bytes-Web-Labs
```

The application server and its sidecar bind to ephemeral loopback ports. The Python harness denies non-loopback socket binds/connections, disables HTTP proxy use and redirect following, bounds response size and request counts, and shuts down the servers. Blue Dragon runs in temporary state/workspace directories with a scripted loopback model, exact preview-path read scope, explicit approval input, and a 30-second process deadline. Its three GET observations per marker case and one GET per policy case are independently paired with native evidence IDs. Synthetic cookie/token values stay in memory or temporary sessions; the saved summary contains no raw credentials or response bodies. Response hashes use canonical JSON serialization, not wire bytes.

The Python audit hook is not an operating-system network sandbox and does not govern the Rust subprocess. Only reviewed loopback URLs are supplied to that subprocess; the external-looking redirect destination is a policy-model string and is never visited. External-request count is an observation of this harness, not packet-capture proof. This is not a template for testing arbitrary sites.

## Measured results

The committed `validation/local-lab-matrix.json` records 12 selected rows and 37 lab HTTP requests: 22 application requests, 12 Blue Dragon collector GETs, and 3 independent HTTP oracle GETs. These are selected-case coverage counts, not whole-site or source-code coverage.

| Cases | Measured behavior | Limit |
| --- | --- | --- |
| CSRF weak/fixed | Removing the token changes the note and revision only in the weak case; independent state reads and normal controls check the result | Manually supplied headers and cookies; no browser exploit or SameSite bypass claim |
| CORS weak/fixed | Credentialed cross-origin grant headers differ, although both HTTP clients receive a 200 response and synthetic body | HTTP client access does not prove browser readability |
| Redirect weak/fixed | Closed policy endpoint accepts/rejects the destination string | No redirect navigation or external request |
| Marker weak/fixed | Three native observations yield `differential_literal_marker_only` for both | Harmless alphanumeric markers do not distinguish raw from escaped rendering or prove execution |
| Native response policy weak/fixed | Four boolean header indicators match a separate HTTP-client observation | Same indicators on both lab variants; neither browser enforcement nor exploitability follows |
| Native response policy invalid preview | HTTP 400 evidence is rejected even when response headers exist | Rejection is inconclusive, not protection |
| Invalid preview | Three native HTTP 400 observations are rejected as unusable comparison evidence | Missing usable evidence is not a negative vulnerability test |

There were zero unsupported definitive confirmations in the fixed/invalid marker cases. The rejected case may omit the confirmation field; the summary preserves that as null rather than inventing a successful negative result. No exploit precision/recall is calculated because browser execution has no oracle here. The six application rows validate the lab behavior through the Python harness; the three marker and three policy rows exercise Blue Dragon end to end. Policy oracles issue separate GETs and inspect headers directly without calling the Rust summary helper; these deterministic lab comparisons do not establish stability on a general server. Evidence IDs are session-local, and full temporary sessions are deleted; the recipe supports rerunning, while summary hashes alone cannot reconstruct responses.

## Validation and remaining limits

- The expanded matrix passed against the changed native-policy release binary. Each policy case checks result provenance through the real CLI, native GET and passive review; any true vulnerability confirmation fails the harness. The named confirmation metric remains limited to fixed/invalid marker cases.
- The lab's Python quality suite passed 77 tests. Its sensitivity suite rejected all three deliberately weakened disposable copies; the checked-out lab remained unchanged.
- The native-policy continuation passed 205 Rust tests, clippy with warnings denied, release build, and 12 existing review-engine CLI smoke cases. The production change adds a bounded allowlisted header summary to native GET results and permits their passive policy review. Unit regressions cover duplicate, binary, oversized and missing metadata, report-only distinction, scope, and privacy.
- Chromium failed before rendering with `socket() failed: Operation not permitted` and process exit -6. Browser execution, actual cookie enforcement, and graphical integration therefore remain untested. No attempt was made to bypass that environment restriction.
- No physical Termux phone, Windows/macOS execution, live model provider, or external website was tested in this matrix. It makes no Alpha parity claim.
