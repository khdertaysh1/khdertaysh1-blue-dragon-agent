# Browser response boundaries and controlled redirect comparison

Local continuation of f74e016; no upload or release.

## Browser source and action results

CDP source reads now resolve the request ID to the latest observed native
response URL and check that URL against user scope before body retrieval.
Caller-provided URLs cannot override that provenance. Missing, evicted or
truncated response URLs fail closed. Extension source reads still validate their
explicit source URL. Tests cover a redirected request ID, caller override,
path boundaries, missing metadata and truncation.

Top-document and explicitly selected frame locations are checked before approval,
after approval, and after applicable actions. If the post-action location is
unavailable or outside scope, result content is withheld and the outcome is
reported as unknown. Screenshots are not retained from that failed postcondition.
Backend action failures are normalized to structured unknown outcomes without
forwarding untrusted page/transport error text. The existing per-turn guard
blocks an identical uncertain action rather than relying on error-string wording.

Authenticated local WebSocket fixtures prove these paths: one click is sent,
its response is lost, the connection is replaced, and only a fresh snapshot is
sent afterward. Identical click retry remains blocked by the turn guard. Separate
fixtures simulate a navigation redirect and an iframe leaving scope during a
snapshot; their private result content is withheld. These are real bridge
protocol tests with simulated extension responses, not browser-rendering tests.

This is not a network firewall or atomic DOM security boundary. Page requests
and redirects may already have occurred before the post-check; side effects
are not rolled back. A location change out and back between checks may escape
these checks. Login/session restoration and pending-action replay are not added.

## Three-observation redirect review

The opt-in redirect_controlled_comparison module requires baseline, variant and
repeated-control evidence in that transcript order, all distinct and paired to
native web_fetch results under current explicit scope. Baseline and repeat
normalized inputs must match. Each comparison against the variant must differ
by exactly one serialized argument field. Historical/denied/unknown results are
rejected. This bounded module requires three redirect responses with native
redacted Location metadata.

Only status and destination origin/path are compared. Inconsistent repeated
control metadata yields inconclusive_control_changed. Consistent metadata is
not proof of stable cookies/account/timing/server state or causality. Destination
query and fragment differences are not measured. No vulnerability is confirmed,
no active request is sent by this review module and no destination is followed.

The CLI loopback smoke now includes three separately approved user turns:
normal, variant, normal again. The repeated URL is fetched independently because
the per-turn cache is reset by the new user turn. It checks three native GETs,
seven scripted model requests, three distinct observations and zero followed
redirects. Existing allowed/denied cases continue to check one/zero GETs.

## Validation and remaining work

175 Rust tests passed on Linux. The final chronological-order refinement passed
all three focused engine tests. Clippy all-targets with warnings denied, release
build and all three CLI scenarios passed. Shared browser form, readiness, option
budget and same/cross-origin iframe contracts passed. Formatting/diff checks
passed. No real graphical browser, physical Termux, native Windows/macOS or live
vision provider was exercised for this branch. Independent review and authorized
cross-platform CI remain necessary before any merge consideration. Full active
multi-type scanners, semantic taint analysis and self-contained replay are still
unfinished; no Alpha parity claim is made.
