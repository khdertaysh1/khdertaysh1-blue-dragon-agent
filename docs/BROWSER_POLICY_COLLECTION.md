# Browser policy indicators without raw header retention

Local continuation from e806f5d. No upload, merge, release or extension installation.

## Collection changes

The extension now requests responseHeaders on its existing informational
onCompleted listener, and immediately reduces them to boolean policy indicators
for managed tabs only. It does not add request-body, cookie, blocking, extraHeaders
or new manifest permissions. CDP now produces the same indicator shape instead
of retaining selected raw header values. Even allowed CSP values can contain
nonces or private report destinations, so those values are not serialized.

Selected indicator inputs are CSP, CSP report-only, X-Frame-Options and
X-Content-Type-Options. At most 200 headers and 2000 UTF-8 bytes per selected
value are examined; oversized names are ignored. Missing header data, duplicate
selected names, non-string values, newlines or excessive size mark the summary
incomplete. Raw names/values outside the allowlist never enter the output.

response_policy_review consumes native_boolean_summary only when available,
complete and correctly typed. Otherwise it emits incomplete/unavailable, never
missing protection. Legacy saved security_headers remain readable for backward
compatibility; this change does not erase old checkpoints. Report-only remains
separate and every output retains protection_verified/vulnerability_confirmed
false. Syntax indicators are not browser enforcement or impact evidence.

## Extension network bounds

The extension retains its 300-event ring and exports a recent window in
chronological order within a 180000 JSON-character entry budget. It reports
entries_omitted and ring_entries_omitted; the latter applies to all managed tabs
since the worker started. These are capture limits, not proof of complete traffic.
Policy review exposes upstream omissions. Long URL metadata is marked truncated,
and source collection refuses a truncated URL as proof of prior observation.

## Validation

198 Rust tests passed, including shared header-boundary cases and consumption of
the exact extension-produced network fixture. scripts/test_browser_policy.mjs
runs the actual background.js with mocked browser APIs: checks responseHeaders
registration, unmanaged-tab exclusion, cookie/authorization/CSP nonce omission,
missing/binary/duplicate/large/multiline headers, URL truncation and network output
budget/ring omissions. The same fixture is read by the Rust review engine.
Existing JS form, collector and readiness contracts also passed.

Clippy all targets with warnings denied, release build, formatting/diff and JS
syntax checks passed. Both unsigned extension archives were packaged locally.
Coverage, all 12 engine CLI scenarios and continuity passed on the final binary;
results are in validation/browser-policy-smoke.jsonl.

The future graphical URL smoke now checks a complete frame-directive indicator
instead of a raw CSP value. That graphical test was not run here. No extension
was installed in a real Firefox/Chromium session, no physical Termux or native
Windows/macOS test was run, and no live provider was contacted. API contracts and
mocked events do not prove end-to-end browser support or policy enforcement.

## API references checked

- Chrome webRequest OnCompletedOptions responseHeaders:
  https://developer.chrome.com/docs/extensions/reference/api/webRequest
- Mozilla informational onCompleted responseHeaders option:
  https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/webRequest/onCompleted

These references establish API availability, not that this implementation was
exercised in a real browser. Broader semantic dataflow, recovery and Alpha parity
remain unproven.
