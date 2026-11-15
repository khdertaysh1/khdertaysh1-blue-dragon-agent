# Cached cross-file call investigation

`source_artifact bridge_call` connects one selected module-level named call through cached imports/re-exports to a unique named function declaration, maps a simple identifier argument to its positional parameter, and returns that parameter's bounded scoped source-use analysis. It reads verified local source versions; it downloads nothing and executes no JavaScript.

```json
{"action":"bridge_call","artifact_id":"CALLER_SHA256","query":"render","offset":61,"argument_index":0}
```

First fetch the authorized sources once, search the caller for the call, then use its exact UTF-8 byte start. The response includes the caller and callee hashes, selected call offset, argument/parameter names, import-chain candidates, scoped use sites and exact read suggestions. Follow those suggestions to inspect actual source bytes. The offset is not an execution start point.

The supported subset is deliberately narrow: module-level calls, named imports/re-exports, one cross-file directly exported named module-level function declaration, up to eight identifier arguments and parameters with equal arity. Calls inside functions, expression arguments, defaults, rest/destructuring, incomplete import chains, ambiguous definitions and detected local reassignment/shadowing or writes to any selected import-chain binding are rejected with a reason. A nested or unexported declaration is not accepted as the exported binding. Default exports and separately declared local exports (`function show(...) {}; export {show}`) are currently unsupported and explicitly rejected, even when valid JavaScript. Writes are rejected conservatively regardless of statement order or execution reachability. No missing source is automatically fetched. Scope and approval continue to apply to the separate fetch operation.

`mapping_available: true` means a syntactic positional candidate exists, not that runtime dispatch or dataflow has been resolved. `semantic_dataflow_verified` remains false and `fresh_evidence` remains false. The returned callee flow can itself be limited; inspect its limits. Block-level bindings, closures, branches, execution order, object aliases, returns, asynchronous effects and sanitizer correctness remain unresolved. Sources can have been captured at different times. An `innerHTML` or `textContent` source occurrence proves neither vulnerability nor protection.

The caller is limited to one MiB and one cached URL association. Existing per-stage bounds apply: the import chain allows six nodes, depth three and two MiB, and callee flow allows one MiB, six symbols and twenty-four steps. Traversals use cooperative one-second checks; there is no single hard wall-clock deadline across all stages. Cached file hashes and lengths are verified. Source excerpts remain untrusted and may contain private project data.

## Reproducible CLI validation

```sh
python3 scripts/smoke_call_bridge.py BINARY --lab PINNED_BETWEEN_BYTES_CHECKOUT
```

The unchanged lab checkout is pinned to `c396c90f2e7ed080f71368ce9edb5cd630098b09`. The test installs three **synthetic module response adapters** in the local server; these are not original lab vulnerability cases. Each case uses the actual Blue Dragon CLI, a scripted local model, explicit read scope and three explicit fetch approvals. The independent fixture oracle verifies all source SHA256 hashes, the import/re-export chain, positional mapping and exact final source bytes.

- Weak-labeled fixture: `input` maps through `render`/`show` to `value`, followed to `node.innerHTML`.
- Fixed-labeled fixture: the same correspondence ends at `node.textContent`.
- Ambiguous fixture: reassignment of `render` prevents mapping.

Three additional negative cases cover a reassigned exported function, a nested function with the same name as an exported value, and an unexported function. All six cases passed: eighteen loopback GETs total, zero analysis/read GETs. Labels describe fixture source differences, not a measured exploit or protection outcome. The Python loopback audit hook does not OS-sandbox the Rust subprocess. No graphical browser, actual Termux device, Windows/macOS runtime or live model provider was tested. The previously observed browser socket restriction was not bypassed.

Validation: 227 Rust tests passed, release build succeeded, clippy passed with warnings denied, and all six CLI cases passed. Machine-readable evidence is in `validation/cached-call-bridge.json`. This local independent continuation starts at `695ac86`; no upload, merge or release was performed.

## Binding-integrity regression

Both reported false mappings were reproduced before the fix with failing cached-source tests. The parser now records whether a declaration is a direct named export in the program AST. The bridge additionally rejects assignments, compound assignments and updates to selected names throughout the cached import chain. Tests exercise `=`, `+=` and `++`, nested/unexported/default/local-clause declarations, and retain positive import/re-export controls. This is conservative syntax filtering, not full ECMAScript module or lexical-binding resolution. The negative CLI fixtures use the same actual fetch/search/bridge pipeline as the positive controls.
