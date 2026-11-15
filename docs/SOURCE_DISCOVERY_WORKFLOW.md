# Source discovery and selected collection

This adds a complete cached-page-to-source workflow to `source_artifact`:

1. `fetch` a user-approved page or JavaScript file into the existing hash-verified local cache.
2. `discover` its bounded static source references without sending requests.
3. `fetch_reference` for one chosen reference, subject to current explicit read scope and the existing download approval.
4. `discover` again to measure which unique references remain missing or match the cache index; read/search/trace the selected cached source as needed.

```json
{"action":"discover","artifact_id":"ROOT_SHA256"}
```

```json
{"action":"fetch_reference","artifact_id":"ROOT_SHA256","reference_index":0}
```

The fetch action re-reads and hash-verifies the root, reconstructs its references and checks current scope on every call. The caller cannot substitute an arbitrary target URL for the selected reference. An existing cached target is hash-verified and reused without a download. A missing target is fetched only through the existing separately approved single-source GET path; there is no recursive or bulk fetching. Scope is checked again by that fetch path.

## What the manifest measures

Each reference has a stable ordinal within this root version, source byte offset, kind, redacted target, specifier hash, cache-index association and collection state. Counts distinguish unique missing in-scope sources, index matches, out-of-scope references, unresolved/unsupported references and duplicates. A repeated URL points to the first reference and cannot trigger a separate reference fetch. Index matches explicitly say they have not been revalidated until opened/fetched; they are not fresh network evidence.

For HTML, a conservative lexical scanner identifies static script `src` attributes. It skips comments and script/style/textarea/title/xmp raw text, respects attribute quotes, handles case-insensitive attribute names, and decodes only the supported `&amp;` entity. Duplicate/malformed source attributes and unsupported entities are incomplete and are not treated as usable references or inline scripts. A detected base element conservatively prevents URL resolution rather than inventing browser base behavior. For JavaScript, existing AST import/re-export collection supplies static module references; package/import-map contexts remain unresolved.

Limits: one unambiguous root URL association, two MiB per root, eighty reference candidates, and a 4,096-byte HTML start-tag scan bound. Collector limits/malformed input propagate `limited`; the uncollected reference count is null when unknown. Counts describe emitted static candidates, not all resources in the application. Inline scripts, templates, dynamic imports/insertion, runtime API requests, browser loading, execution and security coverage are not resolved. The result always sets `runtime_coverage_complete: false` and `fresh_evidence: false`. Source URL query values and raw specifier strings are not emitted in the manifest. Existing source bodies remain local and available through explicit bounded reads.

This feature does not pretend to replace a browser. In particular, a syntactically present script tag can occur in an inert/template context, and an absent static reference does not prove absence at runtime. The recorded Chromium socket restriction remains a blocker; no attempt was made to bypass it.

## End-to-end local lab verification

```sh
python3 scripts/smoke_source_discovery.py BINARY --lab BETWEEN_BYTES_CHECKOUT
```

The script pins the unchanged lab to `c396c90f2e7ed080f71368ce9edb5cd630098b09`, starts its actual loopback server and runs Blue Dragon's real CLI with a scripted local model. Python HTMLParser independently extracts the expected script references from the pinned `academy.html`.

With authorized lab-source scope, Blue Dragon fetches `/academy`, discovers `/studio.js` and `/academy.js`, collects each separately, reuses the first once without downloading again, and reports missing counts 2 -> 1 -> 0 and index-match counts 0 -> 1 -> 2. Exactly three lab GETs occur. With page-only scope, exactly one GET fetches the page, both sources stay outside scope, and attempting to fetch a reference is refused before network activity. No browser load is claimed.

Validation: 215 Rust tests, release build, clippy with warnings denied and both real CLI lab scenarios. Unit checks cover comments/raw-text exclusion, malformed/duplicate attributes, unsupported entities, base detection, reference caps, query redaction, deduplication, scope rejection, approval denial and verified cached reuse. Results are in `validation/source-discovery-workflow.json`. Physical Termux, Windows/macOS runtime, live providers and graphical browser execution remain untested. The original lab checkout is unchanged; this is a local continuation from 5f252ab, without push, merge or release.
