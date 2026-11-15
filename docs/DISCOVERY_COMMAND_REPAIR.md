# Quoted tool discovery repair

Follow-up to reviewed `de1fa3ff6f56a91237c56668004fb4d3459f6907`; that commit and its saved binary are unchanged. No version bump, publication or published asset replacement.

## Original failure and cause

The prior live command trial sent `discover_tools` with `{"names":["\"run_command\""]}`. The name contained an additional JSON string encoding. `src/tool_selection.rs::discover` required exact registry identity and rejected it. Subsequent calls paged offsets 0, 20, 40, then 60 and 80 despite `next_offset:null`, interspersed with recall. Five provider dispatches exhausted the unchanged five-step allowance before any command approval. No 429 or provider failure was observed. This is a tool argument/recovery failure, not evidence of a streaming defect or exhausted service quota.

## Change

`discover` accepts one bounded JSON-string decoding only when it resolves to an exact registered name. It records original/effective names separately in `name_normalizations`; original assistant arguments remain unchanged. Malformed, nested, unknown, whitespace-adjusted names and mixed invalid selections are rejected without changing selection. No alias guessing, execution or permission change occurs.

`discover_once` keys repeated discovery by resulting selection and catalog offset, so equivalent name encodings cannot evade unchanged-phase detection. Phase transitions remain allowed. Out-of-bounds pagination now explains the end condition. Existing approvals, command normalization, schema validation and step limits remain in force.

## Evidence and limits

- 353 Rust tests and all-target Clippy passed; five discovery unit tests include single decoding, rejection, atomic selection and equivalent-name repetition.
- Real CLI against a local scripted provider: 15 cases passed, including the exact quoted-name payload followed by normalized command arguments, approval with exactly one marker append, denial with no file, original argument preservation, streaming/truncation, and 40x24 PTY approval/denial. Maximum steps is five (previous harness used six). The marker-writing case causes an existing completion-only review: four provider requests versus three for denial. This is not an extra tool execution.
- Existing discovery phase-cycle CLI passed, including A/B/A/B restoration, unchanged-repeat block and three-step termination. The preserved pre-fix binary reproduces rejection of the quoted name in the same CLI fixture.
- One subsequent live free-alias trial completed with one specifically approved local print, native stdout `BLUE_COMMAND_LIVE\n`, exit 0, three provider dispatches, no observed 429, and 14.48 seconds elapsed. It emitted correctly typed names and command arguments; **it did not exercise the malformed-name fix live**. The deterministic test establishes that repair. The preceding failed live trial used five dispatches and 30.239 seconds; model output differed, so these two runs are not a controlled performance comparison.
- Same live prompt, free route, no rotation, five-step maximum, exact local-command approval allowlist. No external curl request was executed. No physical Termux, Windows/macOS, real browser, long-session or capacity guarantee.

Measurements are in `docs/measurements/discovery-command/`. This remains a local candidate pending independent review and parent-coordinated release; no Termux update command is advertised for an unpublished fix.
