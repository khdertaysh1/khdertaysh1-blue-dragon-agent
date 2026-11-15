Blue Dragon 0.7.0-alpha.2 improves reliability, browser interaction and the terminal workbench.

- Bounds memory queries, reduces oversized tool context, reuses fetched content and preserves partial reports on interruption.
- Restores browser connection preferences with explicit setup consent; adds select and checkbox actions. Page event handlers may have side effects.
- Adds supervised background jobs on Unix-like systems, with bounded output and cancellation. Windows background jobs are not supported.
- Adds exact native HTTP-status and command-exit assertions; these do not establish exploitability or verify all prose.
- Adds staged single-file multi-edit with conflict checks and a refreshed terminal interface.
- Fixes recall of tool-call details when assistant messages also contain text.

This is an alpha release. Android ARM64 is cross-built, not verified on a physical phone. Browser extensions are unsigned. Very long accumulated instructions can still exceed a model context window. Background jobs cannot guarantee cleanup of processes that deliberately escape their process group.
