# Large local text files

`read_file` and `read_lines` retain their 128 KiB whole-file limit and existing editing hash contract. `search_files` is the small-file project-wide search; it skips larger files. Remote `source_artifact` remains the separate immutable source-cache workflow.

For a selected local file use:

- `read_file_window`: `path`, zero-based `byte_offset`, and `byte_count` (1–16384). Returns UTF-8 text, an exclusive end offset, file size, and `next_byte_offset` (null at EOF). An offset inside a UTF-8 character is rejected; a trailing partial character is deferred. A byte budget too small for the next character is rejected. `window_sha256` hashes only the returned bytes and cannot be used as the whole-file hash for editing.
- `search_file`: `path`, `query` (1–256 UTF-8 bytes), `byte_offset`, `scan_bytes` (1024–8388608), and `max_matches` (1–80). Streams exact, case-sensitive, non-overlapping matches with start/end byte offsets. No regex, structural analysis, or line-number claim. Use match offsets with `read_file_window`.

Always follow `next_byte_offset`, not `scanned_end_byte_offset`: search continuation can repeat a bounded suffix so a boundary-spanning match is not lost. `stop_reason` distinguishes EOF, scan budget, and match limit. An empty partial result does not prove absence. A match-limit result can conservatively require one extra request even at EOF. Each call uses bounded 16 KiB chunks with up to three UTF-8 lookahead bytes per chunk, independent of file and line length; it never builds a full-file or full-line buffer.

These are live-file reads, not cached or immutable versions. File size and modification time are checked during each call; changes detected during a call fail the result. This does not detect all same-size edits or make multiple calls one snapshot. Restart investigation after edits; do not combine offsets from different versions as stable evidence. UTF-8/NUL checks cover bytes read, not unseen parts of the file. Regular files only; existing workspace and secret-path guards apply, including resolved secret aliases. These guards are not an OS sandbox against concurrent adversarial filesystem replacement.

Tests cover large multilingual files, multi-megabyte lines, UTF-8 boundaries, chunk/page-spanning matches, independent whole-string search oracles, metadata changes, cancellation, invalid arguments/data, traversal, and symlink escape/secret aliases. Linux tests do not establish physical Termux or other-platform execution.
