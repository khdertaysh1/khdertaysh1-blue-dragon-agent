# Provider diagnostic and stop-boundary review fixes

Base under independent review remains `24587e0`, unchanged. This separate branch
addresses two independently reproduced blockers; it is not a release or completion
claim for the reliability roadmap.

Error fields are extracted from the already bounded complete HTTP error body
without truncation, then scrubbed against the configured synthetic/real credential
and request context, then clipped for display. Previously clipping code at 128
characters could leave a secret prefix that no longer matched the complete key.
No raw body or request logging was added. Generic prose-secret detection remains
limited; this fixes exact-known-secret truncation, not universal redaction.

Streamed OpenAI-compatible and Anthropic message stops use the same classifier as
nonstream responses. Context-limit, pause, refusal, length, content-filter and
unknown explicit reasons reject the assembled tool batch. Normal stop/tool-call
reasons remain allowed. Existing fixtures lacking a stop reason are still accepted;
this change does not promise complete protocol validation.

Validation:
- 306 Rust tests and Clippy with warnings denied.
- 18 real CLI loopback cases: six diagnostic key/private-request boundary cases,
  and twelve OpenAI-compatible streamed/nonstream stop cases. Each checks one
  provider request, no native observations, unchanged sentinel file and interrupted
  report. Privacy checks inspect stdout, stderr and the saved last_report; original
  private user messages intentionally remain in the private session transcript.
- Native Anthropic transport test exercises twelve HTTP/SSE stop cases with a
  complete tool batch and verifies rejection before it becomes executable.
  Test-only endpoint wiring is used; production routing/authentication is unchanged.
  This is not Anthropic CLI or live-provider qualification.
- The CLI oracle rejects the previous pinned binary for both the secret-prefix leak
  and the SSE follow-up request, demonstrating sensitivity to the original bugs.

No secrets from real accounts, external requests, additional scanner engines,
GitHub writes, release or physical-device run were used.
