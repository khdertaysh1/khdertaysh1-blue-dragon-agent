# Restoration R1 — new recovery baseline, not 393ffb0

Original input: blue-dragon-provider-capacity-source.tar.gz, SHA256
85078fff323aa68f04d4967e5ca31ae1e37f393df9d85b02f4cb3b47d5f3418f.
Archive provenance commit 8c870f85779406cfa5b03de6ebefe47e814b6c36;
extracted tree 8e577e6f1f796092ac756313dc47ab06d8ebe090.
Local baseline commit 670b4a91ac8f52048f68edcd347d58b057f62087 has that tree.
The original archive remains unchanged. Cargo package version remains 0.9.0-alpha.3;
the restoration build ID and source commit in BUILD-MANIFEST.json identify this build.
This is NOT the missing 393ffb0 source, binary, or an equivalent restoration.

## Confirmed findings

- tool_selection::core did not advertise run_command in traffic-budget mode.
  The archived runtime did not contain the user's reported strict validation
  errors: its native dispatcher accepted a known command even when absent from
  the request schemas, and silently ignored extra fields.
- The phone's original arguments are unavailable. The offending field is UNKNOWN.
  `cwd` in our negative fixture is synthetic, not recovered phone evidence.
- intelligence::checks::Contract::needs_review treated changed files with no
  declared checks as failure. File creation/read and HTML/header retrieval each
  required an unnecessary extra model response and ended needs_verification.
- tools::resolve checked input path components but not canonical target names.
  A synthetic alias to .env was readable; the new regression failed before the
  fix. Canonical components now preserve the existing sensitive-path policy.
  Allowed symlinks remain readable. This is not an OS sandbox or TOCTOU guarantee.

## Changes

run_command is included in core schemas. tool_contract validates run_command and
 discovery against the exact schemas sent in that provider request, before
approval or dispatch. Native command dispatch checks the same schema definition.
Extra fields are rejected with a bounded field path and allowed fields; values
are not echoed. Array bound errors include actual_count/minItems/maxItems.
There is no guessed program, field alias, shell conversion, or broken-JSON repair.
Other tools retain native validation; this is not a general MCP schema validator.
Original arguments remain in session history. Successful execution closes only
that tool's correction incident. Discovery alone cannot clear a command error.
Two errors within one incident or eight total argument errors bound recovery.
Existing step, permission and model defaults are unchanged.

Empty checks now mean not_declared, never proof of completion. Declared missing
or stale checks and active background jobs still require review. Search schemas
and results state literal semantics. Optional compact browser snapshots return
bounded URL/title/h1/elements; the original full snapshot shape is retained.

## Actual validation

- Untouched baseline: 328 Rust tests passed.
- Final code: 332 Rust tests, clippy --all-targets -D warnings, fmt and diff check.
- scripts/accept_restoration.py: eight CLI scenarios with a local scripted provider:
  approved printf (waits for fresh approval; exact stdout/exit); rejection (zero
  command execution); create/read; local curl HTML + separate headers/read both;
  literal search/read; bad-good-bad-good; repeated bad; discovery-only correction.
- Baseline command schema absent; two synthetic extra-field calls executed in the
  repeated-bad case. Final repeated-bad executes zero. Bad-good-bad-good executes
  only the two valid calls. File/fetch requests decreased 5 to 4 with no false
  declared-check success claim. These are script/request counts, not live capacity.
- Browser form/readiness/frame/scroll, policy, transport and compact snapshot
  JavaScript contracts passed. They use mock DOM/APIs.
- Unix PTY smoke: model picker/help/exit/terminal restoration passed. It is NOT
  a full TUI approval acceptance test or a physical mobile screen test.
- Real Chromium local-page acceptance could not start: socket() Operation not
  permitted. No sandbox workaround attempted. Browser navigation acceptance
  remains UNVERIFIED. The script is retained for an environment permitting Chrome.

## Limits

No live model, phone, Windows, macOS, or Android execution was tested. Android
cross-compilation is not a Termux test. Historical fixes absent from this archive
are not silently claimed restored: long-paste/TUI approval changes, long-session
compaction and the soft-target/hard-context separation need separate validation.
Default provider/model/rotation and permissions were not redesigned or changed.
No GitHub writes, release, external target research, or external workflows added.
No complete source of 393ffb0 was found. Store this exact source with its binary.
