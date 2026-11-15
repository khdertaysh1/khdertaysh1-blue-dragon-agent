# Historical source investigations in audit reports

`/audit-report` now includes `source_investigations`, a redacted index of recorded `source_artifact bridge_call` investigations. It helps a reviewer resume source inspection after restart without copying source excerpts or asking a model to reconstruct the investigation.

Each row links its evidence ID, caller/callee SHA256, selected call byte offset and local result message. Status is `blocked`, `syntactic_candidate`, or `limited_or_unknown_candidate`. A candidate requires explicit false limit flags in both the recorded import chain and callee flow to avoid the limited/unknown label. This still does not mean semantic dataflow is complete. Missing flags remain unknown. Block reasons use a fixed allowlist, so arbitrary recorded prose is not exported.

Export verifies the original tool request/result pairing and both recorded digests, checks the caller hash and exact call offset against the request, and rejects changed, reused/cached or incomplete provenance. Intrinsically historical local analysis (`fresh_evidence:false`) is allowed. Its observation success flag intentionally does not establish fresh native facts; the report instead requires a successful paired result and labels all output historical. It never promotes this record to a validated native assertion or security finding.

The report omits source symbols, expressions, excerpts and model prose. The original local result contains the chain, limitations and exact read suggestions for deliberate review. Export performs zero network requests, model calls or replays, and does not revalidate current cache files or authorization. A revoked scope does not erase historical records or authorize fetching again. Whole-checkpoint rewriting is outside digest integrity guarantees.

Bounds are the latest 100 observations, 20 exported rows, 128000 bytes per result and two MiB total scanned result content. Skipped, rejected and older-observation counts are visible. Rejected evidence may include another source operation encountered while verifying the bounded window; it is not a count of proven failed bridges. This is an investigation index, not a standalone exploit report.

## Validation

Run `python3 scripts/smoke_call_bridge.py BINARY --lab PINNED_BETWEEN_BYTES_CHECKOUT`. Six actual CLI workflows use synthetic module adapters in the unchanged pinned lab (`c396c90f2e7ed080f71368ce9edb5cd630098b09`). Positive and blocked outcomes are exported from a new process after scope revocation. Each fixture then changes its recorded mapping result without updating digests and exports again, requiring rejection. Server and scripted-model counters must remain unchanged during both exports. Initial source collection totals eighteen loopback GETs; exports add none.

Unit tests cover limited and absent limit metadata, changed results, missing request digests, all reused-evidence markers and private source/reason redaction. Validation results are recorded in `validation/source-investigation-report.json`. No actual JavaScript execution, graphical browser, physical Termux, Windows/macOS runtime or live provider is established. The loopback Python guard is not an OS sandbox for Rust.
