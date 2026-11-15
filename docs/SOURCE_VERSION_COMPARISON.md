# Bounded local source-version comparison

Local continuation from 58db912; no upload, merge or release.

source_artifact compare takes baseline_artifact_id and artifact_id. Both must
belong to this session and share a recorded source URL. Bodies are hash-verified
before comparison. The output contains hashes, lengths and one enclosing changed
byte span per version, with bounded read suggestions. It emits no raw source or
URL and makes zero network requests. It remains historical evidence.

The algorithm scans common prefix and suffix in linear time, without an edit
matrix or extra dependencies. Boundaries are adjusted to valid UTF-8 character
boundaries in both files. The artifact limits bound body reads to 8 MiB each.
Separate edits can enclose unchanged content: this is deliberately not a minimal
line diff, AST change explanation, semantic analysis or vulnerability conclusion.
Read suggestions cover at most 16000 bytes and expose when the changed span is
larger. Empty changed spans have no invalid zero-length read suggestion.

203 Rust tests passed. A reconstruction test exercises all pairs of strings with
empty/equal text, insertions, deletions, separate edits, Arabic, accented text and
emoji. A two-version fixture compares more than 4 MiB while returning under 2000
characters, reads the changed Arabic character by returned offset, and rejects
unrelated URLs, unknown artifacts and altered cached bodies.

The actual CLI source-refresh smoke now also compares versions and uses the
returned suggestion to read just the changed character. Three approved GETs still
produce two retained versions; local comparison/reads add no requests. Eight
scripted model calls complete the flow and the old version remains readable.
All 15 CLI scenarios passed on the final release binary, together with Clippy
warnings denied and format/diff checks. Results/hash are in
validation/source-comparison-smoke.jsonl.

The first final-build tool launch did not start because automatic permission
review timed out. Its one permitted retry succeeded; no blocker remains.

The local Alpha README was consulted for its emphasis on reviewing diffs and
verification. No Alpha execution benchmark or parity score was produced.
No physical Termux, native Windows/macOS, graphical browser or live model was
tested. URL associations are local index metadata, not authenticated proof of
server deployment identity or simultaneous capture.
