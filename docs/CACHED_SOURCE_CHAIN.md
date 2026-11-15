# Bounded cached source chains

`source_artifact` now accepts `action: "trace_chain"`, with a root `artifact_id` and identifier/member `query`. It follows explicit cached import aliases and named re-exports in one bounded operation. This closes the manual handoff between individual `trace` calls; missing sources are reported rather than downloaded.

```json
{"action":"trace_chain","artifact_id":"SHA256_FROM_FETCH","query":"clean"}
```

For an application importing `sanitize as clean`, a bridge re-exporting `decode as sanitize`, and a cached implementation declaring `decode`, the result contains three nodes and two candidate edges. Nodes retain artifact IDs, queried symbols, source byte ranges and bounded excerpts, so the agent can immediately request an exact `source_artifact read` range. The test exercises that final read through the real CLI.

The traversal admits at most six nodes, depth three, two MiB of cumulative source bytes and eight candidates per node. It checks a one-second cooperative traversal deadline between nodes; existing parse deadlines remain in force. This is not a hard wall-clock/I/O deadline. Cached bodies are length/hash verified before analysis. Cycles and repeated candidates are not revisited. Missing package/import-map context, unsupported bindings, missing cached targets, byte/depth/work limits and ambiguous content hashes associated with multiple source URLs are explicit. It refuses to choose an arbitrary source origin for a shared content hash. Invalid/tampered bodies fail the operation instead of emitting an apparently valid chain.

No network request is issued. Output is marked historical with `fresh_evidence: false`, `network_requests: 0` and `semantic_dataflow_verified: false`. A link means that the recorded syntax and cached URL resolution suggest a next place to inspect. It does not prove runtime call dispatch, input propagation, sanitizer correctness, account/session behavior or exploitability. Shadowing, aliases, default/wildcard exports, types, packages and import maps are not resolved semantically. Imported targets use the latest cached version and are not guaranteed to belong to the root's capture. Source excerpts remain untrusted project data.

## Validation

```sh
python3 scripts/smoke_source_chain.py /absolute/path/to/blue-dragon
```

The actual CLI approves three GETs from a synthetic loopback server, traces the cached `clean -> sanitize -> decode` chain, and reads exactly the implementation's function byte range. The trace and read add zero GETs. It uses temporary workspace/session directories and a scripted local provider; no external site or live model is involved. The machine-readable result is `validation/cached-source-chain.json`.

Rust regressions cover alias/re-export traversal, cycle termination, tampered bodies, missing packages, ambiguous URL associations, the byte budget and the depth limit. Full verification: 212 Rust tests, release build, clippy with warnings denied and the real CLI source-chain test.

The browser blocker remains unchanged: the previously recorded Chromium failure occurred before rendering because a socket operation was forbidden. No attempt was made to bypass that restriction, and no graphical browser claim is made. This work instead advances the requested bounded large-file and cross-file investigation workflow. Physical Termux, Windows/macOS runtime and live providers remain untested locally. It is a local continuation from `294d210`, with no push, merge or release.
