# Security model (developer preview)

Blue Dragon requires explicit approval for filesystem writes and command execution, and denies both in read-only mode. Human approval is the security boundary, **not** the model. The user must inspect proposed changes and commands before approving them.

## Threats and limitations

- A tool-calling model may be manipulated by untrusted code or files (prompt injection). Project contents are treated as data, not instructions, in the system prompt, but this is not a guarantee.
- Path validation blocks `..`, absolute paths, common secrets, and out-of-workspace symlinks. It cannot prevent race conditions when another process changes symlinks during a write. Do not use on adversarial shared filesystems.
- Explicitly approved `run_command` is not sandboxed and may execute arbitrary programs. Its timeout does not necessarily kill descendant processes. Only run on a trusted machine or inside a container/VM.
- The agent keeps local session transcripts and sends relevant code snippets to your chosen model endpoint. Check the endpoint's privacy practices.
- File writes are limited to 128 KiB of UTF-8 text at existing parent paths, tool output is bounded, and providers require HTTPS except localhost.
- No active internet scanning, credential collection, arbitrary remote deployment, or unattended command execution is enabled by default.

## Reporting

Please open a security advisory in the GitHub repository instead of publicly publishing unpatched exploit details.
