# Release notes

## 1.1.14.0

Coding and defensive QA release. Open a public URL for its current page, request metadata and console. Automatic dependency discovery and cross-file tracing operate on local development sources. Historical lab results remain readable. The lab runner and its separate training suite are excluded from this release.

Install and launch from a terminal, from the project folder you want to work on.

**Linux x64, Apple Silicon macOS and Android ARM64 (Termux with curl)**

```bash
curl -fsSL https://raw.githubusercontent.com/khdertaysh1/khdertaysh1-blue-dragon-agent/v1.1.14.0/scripts/start.sh | bash
```

**Windows x64 (PowerShell)**

```powershell
& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing 'https://raw.githubusercontent.com/khdertaysh1/khdertaysh1-blue-dragon-agent/v1.1.14.0/scripts/start.ps1').Content))
```


- Run two local code-review delegates with `/swarm PATH | TASK`. The implementation and test reviews run concurrently and their results are collected with each agent's evidence.
- Use `/httpflow EVIDENCE_ID [OFFSET]` to page through previously recorded browser requests and outcomes. Use the existing `/websearch WORDS` with a configured JSON-enabled SearXNG endpoint.
- Use `/desktop open PATH` to launch a workspace text file in Notepad, TextEdit, or an installed Mousepad, Gedit or Kate editor (including Termux:X11).
- Compact fetch receipts retain bounded complete URL values verified against the original result hashes. Older records with only hashes remain identifiable.

- Verify exact script URL arguments for supported literal curl downloads against ordered HTML references. Preserve bounded file receipts across context projection.
- Verify the saved three-column SHA-256 TSV against named local JavaScript files after reading the report back.
- Advertise full-file reads for named source files, offer the missing read schema on the next eligible request, and keep detailed evidence in the session report.

- Use `run --json TASK` for a single result or `run --ndjson TASK` for live events and the final result. Approval prompts go to stderr, with the same stdin approval rules.
- Use `/compact` in chat or TUI to show a local recap and keep a compact projection for later requests, including after resuming the session.
- Open the TUI model picker with Ctrl+T while idle.
- Load project conventions from a workspace-root UTF-8 `AGENTS.md` of up to 16 KiB on session open or resume.
- Read-only tasks default to 48 steps. Set `--max-steps` or `BLUE_DRAGON_MAX_STEPS` (1–100) to override the normal turn limit.

- Preserve file references and conversation context across follow-ups.
- Improve Arabic and English byte-range requests for local files.
- Retain browser network observations during approved response saves.
- Record bounded stream diagnostics when a provider returns an empty completion.

## 1.1.12.0 — cumulative R2–R11 update

- Improve Termux input, including multiline paste, cancellation and Arabic rendering.
- Preserve native command outcomes and distinguish denied, cancelled and unknown results.
- Save selected browser responses with approval and resume local analysis using verified file fingerprints.
- Review large local files with byte coverage and verified quotation ranges. Repair rejected report formatting once from recorded evidence.
- Continue unfinished reviews after restarting using `/resume-review`.
- Support exact file writes, including an explicitly requested trailing newline.
- Collect passive browser evidence with explicit scope and preserve approvals.
- Retain the published baseline's argument diagnostics, verification and provider recovery safeguards.

The release workflow builds Linux, Windows, macOS and Android ARM64 after CI passes.
Android is cross-compiled. Physical-device verification and live-provider reliability remain separate.

## 1.1.9.0 — unreleased candidate

Review and final platform/browser CI are pending. The published baseline remains
[0.9.0-alpha.6](https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/releases/tag/v0.9.0-alpha.6).
The installer does not deliver these changes until a release is published.

### Changes

- Inspect saved tool calls locally without sending another model request or
  replaying an action. Diagnostics expose bounded argument shapes, field paths
  and error codes rather than arbitrary values or potentially sensitive prose.
- Give each tool a bounded argument-correction allowance. Discovery cannot clear
  a failed command; approvals, step limits and protection against replay remain.
- Recognize successful native file reads even when their results lack `ok=true`.
  Validate hashes and window metadata; reject failed, denied, stale, cached,
  uncertain or malformed results instead of reporting false success.
- Use public version **1.1.9.0** from `VERSION` with internal Cargo SemVer
  **1.1.9**. The installer verifies the version manifest and executable before
  replacing a binary. Existing releases and rollback remain available.
- Simplify the project overview and release documentation while preserving
  attribution and the distinction between demonstrated behavior and open limits.

### Validation and limits

A live-model local `curl` trial on diagnostic commit `2450357` completed one
approved HEAD request, with verified native headers and exit code 0. It used
three provider requests in 17.64 seconds with no observed 429. That commit later
needed the read-correction fix `9691a34`, so this trial is not qualification of
the final release, a phone test, an external-site test or proof of long-session
reliability. The original phone call arguments were unavailable; discovery-list
hypotheses must not be presented as reproduced phone inputs.

See [diagnostic recovery evidence](COMMAND_DIAGNOSTIC_RECOVERY.md),
[version mapping and upgrade checks](PUBLIC_VERSION.md), and
[acceptance requirements](CANDIDATE_TRIAL_PLAN.md). Publication requires review,
final CI and a previously unused tag. Version numbering does not establish
AlphaCode equivalence or unlimited provider capacity.

## Previous releases

These summaries describe published changes. Linked engineering reports retain
their original test scope and limitations; historical local results are not
substitutes for validation of a newer release.

### 0.9.0-alpha.6

Fresh-source acceptance and bounded continuity checkpoints; validated command
arguments; explicit command failure outcomes; local result inspection; bounded
discovery-name correction. Original history and approval boundaries remain.

The implementation candidate passed 353 Rust tests, 15 command CLI/PTY cases and
35 local qualification groups. A separate live print task completed in three
requests, but an earlier trial stalled at discovery. These are not controlled
performance comparisons or long-session proof. See [continuity evidence](CONTINUITY_EVIDENCE_AUDIT.md),
[command repair](COMMAND_ARGUMENT_REPAIR.md) and [discovery repair](DISCOVERY_COMMAND_REPAIR.md).

### 0.9.0-alpha.5

Separated soft traffic targets from context bounds, preserved tool-call pairs,
bounded discovery repetition, exposed failed projection separately from the last
successful request, and added opt-in free-model fallback without shared-quota
bypass. One-shot URL inspection now fails when inspection is denied or fails.

Scripted local acceptance reached 30/30 fresh-source exchanges versus 7/30 on
alpha.4. The live trial passed two exchanges and failed the third; it did not
prove 20–30-turn live stability. See [context continuity evidence](context-continuity-review.md).

### 0.9.0-alpha.4

Pinned gateway automatic routing, shared catalog caching, bounded provider
traffic accounting and coordinated admission/cooldowns. Historical controlled
measurements reduced estimated input from 158,822 to 40,300 tokens. Ten live
requests showed no 429, but only three of four rounds performed the requested
fresh read. See [capacity review](provider-capacity-review.md).

### 0.9.0-alpha.3

Redacted provider diagnostics, complete-argument validation, shared cooldowns,
long-session constraint preservation and supervised multi-file edit proposals.
Approval diffs explicitly represent CRLF and reject bare carriage returns.
See [provider reliability](PROVIDER_RELIABILITY.md), [boundary fixes](PROVIDER_BOUNDARY_FIXES.md)
and [edit proposal limits](SUPERVISED_EDIT_PROPOSALS.md).

### 0.9.0-alpha.2

Abyss visual identity, README animation and optional terminal motion. The GIF is
decorative; ordinary Termux text terminals do not display the bitmap mascot.
See [identity and motion controls](ABYSS_IDENTITY.md).

### 0.9.0-alpha.1

Bounded large-file reading/search, scoped historical memory, independently
sessioned read-only agents, evidence-linked reports and retests, approved MCP
resources, syntactic outlines and bounded browser scrolling.
See [integrated comparison](INTEGRATED_ALPHA_COMPARISON.md),
[delegated agents](DELEGATED_AGENTS.md), [coverage ledger](TEST_COVERAGE_LEDGER.md)
and [file windows](LOCAL_FILE_WINDOWS.md).

## Scope of evidence

Physical Android testing and sustained live-provider reliability remain
unverified. Scripted fixtures, cross-builds, historical captures and illustrative
screenshots do not establish those results. Provider quotas remain external;
there is no guarantee of avoiding HTTP 429 or of equivalence to AlphaCode.
