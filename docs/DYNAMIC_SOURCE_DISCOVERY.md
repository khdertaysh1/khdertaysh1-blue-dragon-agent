# Bounded literal dynamic-import discovery

`source_artifact discover` now includes JavaScript `import('./lazy.js')` candidates from the actual syntax tree, alongside static imports and re-exports. This helps collect a selected lazy-loaded module for local investigation without executing JavaScript or repeatedly fetching the parent file.

Workflow: fetch the authorized root once, discover its references, inspect the reference index and recorded state, then explicitly select one with `fetch_reference`. The selection is re-derived from the hash-verified source each time and passes the existing current read-scope and approval checks. No manifest-wide download is introduced. Duplicate resolved targets remain duplicates. Cached sources continue to be marked historical and verified when opened.

Only one nonempty plain quoted string argument without backslashes, up to 2000 bytes, is supported. Computed expressions, template strings, escaped strings, empty strings, options/additional arguments and other unsupported argument forms become `unresolved_dynamic_import` rows with no URL and no fetch capability. Their presence marks the analysis limited. A string containing import-like text or an object method named `import` is not a module reference. Literal candidates can still be conditional or unreachable; no load is claimed.

Static and dynamic references share the existing 80-reference collection cap and syntax traversal limits (60000 nodes and cooperative 200ms checks), with a two-MiB source limit in discovery. Results are sorted by recorded source offsets, but truncation is not guaranteed to retain the first references in source order. When limited, omitted totals remain unknown rather than invented. This metadata propagates through source discovery into `/audit-report`. An unresolved dynamic import also conservatively limits other syntax analyses using the same parser result.

The resolution model remains relative/absolute URL candidates only. Import maps, package resolution, runtime expressions, DOM insertion, workers, service-worker interception and actual browser fetching are not modeled. A literal candidate is neither a resolved runtime module nor an executed dataflow edge. No new dependencies are added.

## Validation

Run `python3 scripts/smoke_dynamic_discovery.py BINARY --lab PINNED_BETWEEN_BYTES_CHECKOUT`. It uses synthetic JavaScript response adapters in the unchanged local lab at `c396c90f2e7ed080f71368ce9edb5cd630098b09`, through the actual CLI and a scripted local model. The adapter source hashes, selected literal offset, expected unresolved/duplicate rows and server request list are independent fixture expectations.

Three scenarios passed: authorized selected fetch (two GETs), dependency outside scope (one root GET), and denied dependency approval (one root GET). Discovery and report export add zero GETs. Both computed dependencies remain unresolved; attempting to fetch one fails without a request. The report retains limited collection and the two unresolved references. Unit tests additionally cover escaped strings, fake import text/methods, static/dynamic mixing and 81-reference truncation. All 230 Rust tests, the release build and clippy with warnings denied passed.

Evidence: `validation/dynamic-source-discovery.json`. No graphical browser, JavaScript execution, physical Termux, Windows/macOS runtime or live provider was tested. The Python loopback audit guard does not OS-sandbox Rust. This independent local branch starts at `a8ebc93`; no upload, merge or release occurred.
