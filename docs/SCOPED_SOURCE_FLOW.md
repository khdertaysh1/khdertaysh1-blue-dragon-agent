# Scoped source-use workflow

`source_artifact flow` adds bounded same-function syntax-use traversal after a source has been cached. The practical sequence is fetch once, search for the relevant code, pass a UTF-8 byte offset selecting its function scope, then follow the returned exact read suggestions. The operation can connect a queried identifier through simple assignments to calls, returns and property writes without repeatedly sending the whole source to the model.

```json
{"action":"flow","artifact_id":"CACHED_SHA256","query":"input","offset":123}
```

The offset selects the nearest containing JavaScript function/arrow/method (or program scope); it is not a statement execution start point. Each returned step identifies its queried input symbol, syntax kind, target, input expression, function range, source range and exact `read` suggestion. Simple identifier assignment targets may be inspected next. Property writes, calls and returns are endpoints for review, not automatically proven sinks or validators. Nested function bodies are excluded when looking for input references, and other function scopes are excluded from the selected result.

A symbol with multiple syntactically visible writes is marked ambiguous and does not expand to additional aliases; its visible use sites may still be listed with the warning. This is deliberately conservative and does not model statement order. A call named `clean` or `valid` is not proof that data was cleaned or checked. Both `innerHTML` and `textContent` are reported as source syntax; their presence alone establishes neither a vulnerability nor protection.

The source body is verified against its cached hash and length. Bounds are one MiB per source, six examined symbols, twenty-four steps and a cooperative one-second traversal check between analyses, in addition to the existing parser/reference-scan limits. These are bounded syntax operations, not a hard wall-clock or filesystem-I/O deadline. Truncation, parse/reference limits, competing writes and pending symbols are visible. Oversized sources require narrower read/search inspection instead. No network requests are issued, and results are historical with `semantic_dataflow_verified: false`.

Unresolved limits include branches, execution order, block shadowing, property/object aliasing, closures, async effects, runtime dispatch, function return values, type resolution and sanitizer correctness. The analysis is JavaScript syntax based; parse errors limit conclusions. Source excerpts can contain private project data and remain untrusted. It is not an interprocedural taint engine or an execution oracle. Cross-file alias/re-export investigation remains available through the separate cached `trace_chain` action.

## Actual lab workflow

```sh
python3 scripts/smoke_scoped_flow.py BINARY --lab PINNED_BETWEEN_BYTES_CHECKOUT
```

The test uses the unchanged lab at `c396c90f2e7ed080f71368ce9edb5cd630098b09`. Through the actual Blue Dragon CLI it approves one GET of `/studio-target.js`, searches for `const r=`, scopes the analysis to the lab's async callback, identifies the response variable's use in `out.textContent`, and reads that exact source expression. The fetched body hash, anchor offset and final bytes are independently compared with the pinned lab's original `TARGET_JS`. It sends one GET total and zero analysis/read GETs. It does not submit the form or execute the fetched JavaScript.

Rust fixtures additionally exercise a transformation and a validator-named call followed by either `innerHTML` or `textContent`, excluding unrelated/nested functions; another fixture checks that competing writes prevent further alias expansion. Validation: 221 Rust tests, release build, clippy with warnings denied, and the actual lab CLI workflow. Machine-readable evidence is in `validation/scoped-source-flow.json`.

No graphical browser, physical Termux, Windows/macOS runtime or live model provider was tested. The browser socket restriction was not bypassed. This independent local continuation starts at 26926c5, with no upload, merge or release.
