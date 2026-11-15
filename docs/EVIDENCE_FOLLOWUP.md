# Local evidence and tracing follow-up

This follow-up is separate from the published draft #13 batch. The published head
`de2f613d82be3a5e83629c7ec4bf7f4a0ec6ca97` has the exact tree of local
`57d03006a0fc73d01c88f08413fbd147a8d33c40`:
`21881ab69e8d6486954fd68c4866f5b430721500`.
GitHub Actions run 43662580002 completed successfully: Linux, Windows, macOS,
installer checks, Android ARM64 NDK build, and real-browser integration.
Release publication was skipped. This does not test a physical Android phone.

## Reproduced review findings

Regression tests added to the original audit implementation failed in three cases:
explicitly stale source reads accepted as a controlled pair; those reads counted
as coverage; automatic URL-inspection evidence lacking a paired tool request
reported without an explicit unavailable-input status. Existing three audit tests
continued to pass. The corrected implementation passes all six tests.

The first local correction commit, `2f1ebdd`, rejects fresh_evidence:false,
cached/cached_result, reused_local_copy, reused_evidence_id and
original_evidence_id on either side of a comparison and in coverage recording.
Historical data remain useful context but do not constitute fresh observations.
Automatic-inspection inputs are explicitly unavailable: the implementation does
not parse user-message prose into trusted provenance or invent a replay request.
Paired input hashes are labelled normalized_arguments_json, not exact wire hashes.

## Further local corrections

- Reject duplicate evidence IDs, duplicate matching tool-call IDs, and mismatched
  observation/tool-call names rather than taking the first match.
- Compare individual array elements; distinguish missing object keys from null.
  Multiple changed array inputs no longer count as one changed serialized field.
- Match source-trace identifiers/member-expression syntax rather than arbitrary
  substrings. String literals, longer identifiers and object property keys do not
  become references merely because they contain the query.
- Resolve only explicit relative/root/HTTP imports. Bare package specifiers and
  aliases require package/import-map context; do not invent cached URL links.
  Even explicit URL candidates are labelled import-map-unverified.

## Verification and limits

152 Rust tests passed locally on Linux after all corrections. cargo fmt, clippy
(all targets, warnings denied), and git diff --check passed. Tests cover stale
markers on both sides, unavailable unpaired inputs and secret omission, ambiguous
provenance, arrays/null, identifier false positives, and import resolution.
These follow-up changes have not been uploaded or run in cross-platform CI.

Source tracing is still bounded syntactic assistance: shadowing, aliases,
interprocedural semantics, import maps, sanitizer correctness, execution order
and exploitability remain unresolved. Dotted queries use exact source spelling;
computed access/whitespace variants can be missed. A comparison cannot establish
stable login/server state or causality. The report remains a redacted evidence
index, not a complete standalone replay script. Automatic-inspection original
requests remain unavailable in this format. The historical macOS timeout is not
proven fixed by one successful CI run. Physical Termux and live model-provider
vision tests remain unavailable; no AlphaCode parity claim is made.
