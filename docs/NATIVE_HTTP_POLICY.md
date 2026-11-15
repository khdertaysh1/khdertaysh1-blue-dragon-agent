# Native HTTP policy evidence

A user-approved, in-scope `web_fetch` now summarizes four response headers: CSP, CSP report-only, X-Frame-Options and X-Content-Type-Options. `review_web_evidence` with `response_policy_review` can inspect that paired native observation without a browser, another request, or additional dependencies. Existing browser evidence remains supported.

Only boolean indicators and availability/completeness flags are added to saved tool results. The helper selects exactly four header names, limits copied values to 2,000 bytes each, and rejects duplicate, binary or oversized selected values as incomplete. Raw policy values, cookie headers and nonce/report-endpoint strings are not included in the summary. This does not sanitize the existing fetched response body.

The review still requires current explicit read scope and verified original evidence pairing, integrity and freshness. It identifies the collection as `single_native_GET_without_browser_session` and points to `/security_header_indicators`. A single captured response is not a browser network log; missing legacy metadata produces an unknown row, not a claim that protection is absent. The one-entry capture counters describe the response envelope, separately from header metadata completeness. HTTP error observations remain unusable for this review under the existing evidence policy.

No effective CSP interpretation, script execution, framing test, cookie behavior, session continuity or protection is claimed. Report-only remains distinct from enforcing-header presence. The same safe header indicators can occur on both weak and fixed lab pages.

Validation: 205 Rust tests, warning-free clippy, release build, 12 existing CLI review scenarios, and the 12-row pinned loopback lab matrix. Six lab rows now use the actual Blue Dragon CLI/tool/evidence/review flow; the other six remain direct application tests. Browser startup remains blocked by the previously observed environment restriction; no graphical test, physical Termux or live provider was performed. See LOCAL_LAB_MATRIX.md and validation/local-lab-matrix.json for reproduction and exact counts.
