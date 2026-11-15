# Explicit source refresh with retained versions

Local continuation from 4663ccd. No upload, merge or release.

source_artifact now has an explicit refresh action. Ordinary fetch still reuses
the latest cached copy without requesting approval or redownloading it. Refresh
requires an existing source for the exact URL in this session, current read scope,
verification of the previous cached body and a separate approval for one new GET.
It retains the existing HTTPS/loopback validation, no-redirect/no-cookie policy,
30-second request deadline, 10-second connection deadline and cancellation checks.

A changed UTF-8 body is stored under its SHA256 with the old body still available.
Refresh returns previous_artifact_id, content_changed and version_history_preserved;
it does not put the body into model context. An unchanged body returns newly
observed metadata without appending another cache record. Error statuses on
refresh do not replace the previous source or return the error body. Denial,
scope failure, cancellation, parse/size errors and refused redirects leave the
previous cache record intact. Hash changes are content differences only, never
vulnerability, semantic or deployment-identity proof.

Limits remain 8 MiB per source, 32 version records and a conservative 64 MiB sum
of recorded versions. A full cache refuses another request before downloading,
even if that request might have returned unchanged content. An unchanged refresh
does not rewrite the old cache record's metadata; that record remains explicitly
historical while the new tool observation records the current response metadata.

Default trace now selects the latest cached version per URL. An explicit
artifact_id can still inspect or trace an archived version. Cached import links
resolve to the latest cached target version and label that basis; there is no
claim that importing and imported files were captured simultaneously. Package,
import-map, runtime and semantic resolution remain unverified.

Example sequence (each network step requires its own approval):

    source_artifact {"action":"fetch","url":"https://example.test/app.js"}
    source_artifact {"action":"refresh","url":"https://example.test/app.js"}
    source_artifact {"action":"read","artifact_id":"<previous_artifact_id>","count":100}

## Validation

201 Rust tests passed. Local HTTP tests cover changed/unchanged bodies, no
refresh before initial fetch, retained old content, default latest tracing,
explicit historical tracing, scope rejection, denied refresh and a 503 response
without exposing its body. Requests are counted and bounded; no live target is
used. Existing large-source fetch-once/search/reload tests remain passing.

scripts/smoke_source_refresh.py runs the actual CLI with a scripted loopback model
and HTTP source server. It performs fetch, changed refresh, cached fetch,
unchanged refresh and reading the first version. Three approved GETs produce two
version records; six model calls complete the flow. Every returned result remains
under 1600 serialized characters. No raw source body is returned until the explicit
bounded read. The test is added to future CI but remote CI has not run this branch.

Clippy all targets with warnings denied, release build and formatting/diff checks
passed. The final binary passed 15 CLI scenarios: source refresh, coverage,
12 engine cases and continuity. Results/hash are recorded in
validation/source-refresh-smoke.jsonl.

No physical Termux, native Windows/macOS, graphical browser or live provider was
tested. Refresh does not reproduce an authenticated browser response, prove
server-state stability, perform semantic source comparison or complete the six
broader work areas. No Alpha parity claim is made.
