# Import-binding completeness correction

The independent review identified a real producer-to-consumer gap: the AST binding collector stops at 32 retained bindings or 128 visited binding nodes and marks the import's `bindings_limited`, but the source-chain traversal could omit the queried binding and return `limited: false` with no blocked entry. Missing edges therefore looked complete despite incomplete input.

The regression was reproduced locally before changing production code: an actual forty-name import was parsed, a genuinely omitted name selected, and `trace_chain` incorrectly returned false for `limited`. The test failed with that exact assertion.

The source producer now promotes binding truncation to its overall `limited` flag. The chain also exposes `import_bindings_limited` and the number of incomplete import collections on each node. Before filtering imports by the surviving queried-name bindings, it records `import_binding_collection_incomplete`, source byte offset and retained count. The omitted count remains null: the bounded collector did not measure the total and must not invent it. Known candidate edges can still be returned, but absence of an edge is not complete coverage.

Validation: 213 Rust tests, warning-free clippy, release build, and two actual CLI source-chain cases. The complete alias case still produces three nodes, two edges and an exact implementation read. The 41-name CLI import retaining 32 names and omitting `clean` now produces one node, zero edges, `limited: true` and an explicit incomplete-binding blocker. Both cases use three approved loopback source downloads and zero additional GETs during analysis. See validation/source-binding-completeness.json. Reproduce with `python3 scripts/smoke_source_chain.py BINARY`.

This corrects completeness reporting; it does not add semantic dataflow or browser execution. The larger source-discovery expansion was deferred while fixing this review finding. Existing browser environment restrictions were not bypassed. This is a local-only follow-up to 438b802; no push, merge or release.
