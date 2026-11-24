# Bounded history projection and retrieval

Original session messages remain unchanged on disk. The current user turn and system messages remain verbatim. Two user turns stay verbatim except at the final emergency tier, where the previous turn may use the same declared historical representation. Projection adds two bounded representations for older user content:

* Exact shared sentence dictionaries for long messages; ordered segments reconstruct every original character. Short instructions remain verbatim.
* Explicitly lossy omission of long opaque hexadecimal values in labelled observation sentences, retaining the surrounding prose, original SHA256/message index and original character ranges. Equal-width values use cumulative offset deltas. Directive-bearing sentences matched by the conservative guard stay intact. This is not a general natural-language constraint classifier, semantic summary, or permission grant. Unknown prose stays intact and can still exhaust context. A required value must be retrieved from the original before citation.

The UI announces omission. The model-facing representation says it is historical data, not fresh evidence or authorization. Recall never reruns a tool. A bounded latest tool round stays paired; small latest outputs (up to 1024 characters) remain exact for the next decision rather than immediately being archived. Larger outputs retain the existing explicit excerpt behavior.

The long-history harness independently reconstructs every encoded message or its declared masked text and compares it to the original. The distinct fixture must complete all 30 turns, restart after turn 15, preserve every original message and source file, and retrieve an omitted turn-zero value through `recall_context` before its final answer. Ten negative oracle controls reject altered ranges, forged text, stale/cached/denied/unknown results and replay. A scripted model selecting the right recall is a transport/evidence test, not proof a live model will do so.

The independent `/tmp/review_heterogeneous.py` is run unchanged; it contains twelve different natural-language sentences shared across thirty messages. Its result must not be described as arbitrary nonrepeating natural-language capacity. The separate distinct fixture contains different opaque values in every observation. Neither proves unrestricted recall, a one-hour session, real-browser behavior, physical Android use, or Alpha equivalence.

Publication remains blocked pending independent review and the required integrated/live acceptance, especially for uncompressible prose and reliable model-driven retrieval. README/branding cleanup is separately coordinated for the same release after implementation review.

## Exact nonnumeric text differences

A further tier uses a bounded Unicode-character delta against one explicit historical user-text dictionary. Literal strings and `[offset,count]` copies reconstruct the original exactly; both the base and each reconstructed message carry hashes. Changed words, negations, filenames and markers are retained, not normalized away. Numeric-only differences keep their existing representation. Diff work has a 10 ms budget per candidate, a 12,000-character input bound and 128-operation cap; a nonbeneficial delta is rejected rather than dropping text. This adds no model request and does not infer which constraints are important.

The independent marked-natural-text reproducer is kept unchanged. The bilingual marked harness independently decodes every projected delta, compares all text with the original, restarts at turn 15 and requires an original `recall_context` result before reporting an old detail. Neither makes unrelated arbitrary prose compressible or establishes semantic model memory. The previous opaque-hex omission remains separately labelled lossy; it must not be described as a lossless delta.

The overall `projection_is_lossless` flag is true only for unprojected history. Exact user-text encodings do not imply that assistant reasoning/tool excerpts elsewhere were retained in full.

## Live recall-loop follow-up

The 333d8ce live trial completed four turns; turn five performed a correct fresh read but exhausted its configured six steps in repeated historical recalls. This was not HTTP 429 or successful investigation completion. A local contributing defect was that large recall results could immediately be projected again, hiding the original message/offset and pointing at a recall-result wrapper.

A bounded latest recall page (up to 4096 estimated tokens) now remains intact for the next decision. When a larger/historical recall result is projected, the excerpt preserves its original message index and offset and advances next_offset by exactly the visible character count. It does not redirect the model into a chain of recall wrappers. Original saved results are unchanged. The six-step and eight-page limits are not raised.

`smoke_recall_projection.py` reproduces the old failure through the CLI: one native file window and a default recall, with independent comparison of the page actually sent back to the model. Baseline CLI completion is not accepted as proof of delivery. The corrected test requires the exact original recall page, one source read, unchanged file and constraints, within the same six-step cap. A live model may still choose unnecessary pages; a deterministic pass alone does not establish live completion.
