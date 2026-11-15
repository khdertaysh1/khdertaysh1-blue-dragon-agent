# Evidence-checked source coverage in the reproduction report

`/audit-report` now includes a `source_discovery` section. The workflow is: discover from a verified cached page, explicitly collect selected in-scope references, rediscover remaining sources, then export a report linking the recorded coverage to native request/result evidence and a concrete local reinspection action. Exporting never fetches sources or invokes the model.

For each root artifact, the section chooses the latest accepted discovery result within the bounded observation window. It requires a correctly paired `source_artifact discover` call, matching root artifact ID and recorded request/result digests. It verifies those digests again through the existing evidence resolver. Legacy pairs without both digests, altered results, mismatched roots and reused/cached result envelopes are excluded and counted. Direct local discovery is intrinsically historical (`fresh_evidence: false`) and is accepted explicitly as recorded analysis, never as fresh website evidence.

The report recounts missing/indexed/outside/unresolved/duplicate states from the recorded reference list rather than copying its summary counters. It exports root/evidence/message IDs, collector limitations, unknown omitted-reference counts, source byte offsets, redacted targets and the exact `source_artifact discover` arguments needed to reinspect the root. Each reference checks current read scope anew; all automatic-fetch flags remain false. Cache-index matches are not revalidated by reporting, and stale cached files may need separate inspection before an approved fetch.

Raw source text, specifier values, model prose and query values are not copied into this section. URLs are redacted again even when the producer already redacted them. Paths can still require manual redaction. Reinspection first re-reads the hash-verified root; selected fetching still needs current scope and explicit approval. Hashes provide consistency checks, not authenticity against rewriting the whole local checkpoint.

Bounds: latest 100 observations; at most ten roots and twenty exported reference rows per root; 128,000 bytes per considered tool result and two MiB of total considered result text. Counts cover up to the producer's eighty recorded references; omitted export rows, rejected evidence, skipped-budget entries and older observations are reported separately. This is not whole-site coverage or a standalone exploit proof. If a newer result is rejected, an older verified result may be shown, identified by its own evidence/message reference and recorded-analysis basis.

## Actual CLI and lab verification

```sh
python3 scripts/smoke_source_discovery.py BINARY --lab PINNED_BETWEEN_BYTES_CHECKOUT
```

All three scenarios use the pinned local lab and real Blue Dragon CLI from discovery through `/audit-report`. The normal case performs three GETs and reports two source index matches after collection; page-only scope performs one GET and retains two outside-scope references; the malformed response adapter performs one GET with no dependency fetch and incomplete collection. The original lab remains unchanged.

For every scenario the test then starts a new CLI process, clears scope, exports again, and checks that formerly allowed references are no longer reported authorized. It then deliberately modifies recorded discovery tool results without changing their native digests and confirms that a subsequent report excludes them. The exports, restarts and tampering checks perform zero model calls and zero GETs. The normal page's source set is independently checked with Python HTMLParser; the malformed case uses the previously documented conservative collector contract.

Validation: 219 Rust tests, release build, clippy with warnings denied, and all three extended end-to-end report scenarios. Unit regressions also verify count recomputation from intentionally incorrect summary counts, query redaction, current-scope changes, integrity rejection, legacy digest requirements and reused-envelope rejection. Results are in `validation/source-report-workflow.json`.

No graphical browser execution, physical Termux, Windows/macOS runtime or live provider was tested. The earlier browser socket restriction was not bypassed; static source coverage does not prove browser loading, script execution, security coverage or exploitability. This independent branch starts from d28dcd1; no upload, merge or release is included.
