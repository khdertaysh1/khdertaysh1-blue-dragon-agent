# Bounded supervised multi-file proposals (development)

Independent children can propose, never directly apply, changes to 1–4 granted
existing regular UTF-8 files. Each file is limited to 4096 bytes and combined old
and new text to 16 KiB. Proposals pin base and replacement hashes, preserve separate
child evidence, and reject duplicate/out-of-scope paths, symlinks, hard links on
Unix, control characters and oversized diffs. Bare carriage returns are rejected.
CRLF bytes remain supported and unchanged in files; approval diffs encode CR as
visible `\r` and double literal backslashes, preventing cursor movement and
distinguishing control bytes from literal text. Diff bounds apply after encoding.

Parent review uses the pinned native proposal result, not a model summary. Apply
and rollback each require the existing parent approval mechanism with the full
bounded diff. All files are re-read after approval, versions checked, and an
exclusive workspace lock and durable unknown-intent journal created before any
replacement. Every resulting file is independently read again before success.

An interrupted or uncertain operation is never automatically replayed. Conditional
rollback only replaces a version still matching this transaction's own output;
external changes are preserved. Uncertainty retains the lock for manual
reconciliation. Explicit rollback requires the committed journal, original pinned
proposal and matching current versions. Task/session recovery is not file rollback.

Limits: multi-file replacement is not filesystem-atomic; other programs, commands,
MCP servers and external writers do not honor the cooperative native-file lock.
Filesystem races and full-store forgery are outside the guarantee. Windows hard
link detection is not implemented. No new files, deletions, nested agents or
isolated mutable workspaces. Journals contain private before/after source text.
No phone, live-provider or cross-platform validation claimed by local tests.

Provider adaptation also preserves system/user/assistant text-block histories when
translating to Anthropic. Unsupported blocks reject before transmission instead of
silently dropping constraints. This is structural compatibility, not confirmation
of any live model's tool capability, catalog accuracy or context capacity.

Reference inspected: AlphaCode `d49f883`,
`src/alphacode_app_core/tool/apply_patch.rs` (patch argument extraction, workspace
path guidance and patch input validation). This implementation uses an intentionally
smaller complete-replacement proposal format with explicit native evidence pins;
no Alpha code was copied, and existing notices/licenses were preserved. No runtime
comparison or general equivalence with Alpha was measured in this change.
