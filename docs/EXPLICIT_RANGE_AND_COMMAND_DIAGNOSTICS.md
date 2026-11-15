# Explicit source ranges and command argument diagnosis

Historical diagnosis at `68c06f6`. The command behavior described below is superseded by the implemented [command argument repair](COMMAND_ARGUMENT_REPAIR.md); the original evidence and diagnosis are retained for review.

Isolated follow-up to reviewed candidate `66c926a8959c107c9066421501c0dd6e98d081bc`; that worktree remains unchanged. No remote push or release action.

## Confirmed acceptance defect and fix

A real CLI fixture reproduced the candidate accepting a 0..512 native window for `Read bounded window of app.js bytes 1000..1100 again`. The detector left `expected_window=None`, so the native capture was valid but unrelated to the requested interval.

`src/intelligence/fresh.rs::{explicit_range,detect,status}` now persists an explicit half-open byte interval and requires a native window covering it. Explicit bounds are not silently shortened to EOF. `bytes 1000..1100`, `byte range 1000..1100` and Arabic `بايت ١٠٠٠..١١٠٠` are supported; Arabic/Persian digits are normalized only for parsing the bounds. Ambiguous/unsupported range syntax, reversed/empty/multiple ranges and inclusive qualifiers are unresolved, never accepted as an arbitrary nonempty read. A syntactically supported interval must still be within tool permissions and resource limits. This is not general natural-language comprehension or a union-of-many-windows proof.

Tests include the reported mismatch, a valid 1000..1100 read, restart followed by Arabic `كمل`, negative range syntax, explicit bounds beyond EOF, and all previous native freshness cases. `../outside.js` is not mistaken for range syntax. Existing file permission checks still reject the out-of-workspace read.

## run_command diagnosis (no commands executed)

The current advertised schema in `src/tools.rs::definitions` requires `program: string`, `args: array<string>` and optional integer `timeout_seconds` in 1..300. `src/tools.rs::invoke` checks these before command approval or process creation. No string-to-array or string-to-integer coercion currently occurs.

Controlled read-only dispatch tests show:

| Input shape | Current outcome |
|---|---|
| Object with `program="curl"`, `args` a serialized array string, timeout string | `Missing args array` |
| Object with valid program/args array, timeout `"30"` | `timeout_seconds must be an integer` |
| Entire arguments value is a JSON string rather than an object | Direct dispatcher: `Missing or invalid program`; current provider completion validation rejects it earlier as malformed |

Therefore stringified nested args alone does NOT explain the reported `Missing or invalid program`. That message indicates the dispatcher did not find a string `program` at the top level (missing, null, wrong nesting/type, or whole-value encoding). The actual phone build and sanitized wire shape remain unknown. Current `src/provider_contract.rs::{identity,validate_completion}` rejects whole-string/double-encoded or truncated top-level arguments before tool execution, but does not enforce each advertised property's type; tool-specific validation does that later.

No claim is made that the Termux bug is reproduced exactly, nor that strings should be silently coerced. The next evidence needed is the exact installed version/commit and type-only argument shape at the provider/dispatch boundary, without URLs, headers, keys, or private conversation bodies. A future compatibility fix should provide field-specific `not_executed` feedback and bounded correction before approval, not decode shell text, bypass approval, or retry an unknown command. This diagnosis is language-independent; Arabic and English source requests remain supported.
