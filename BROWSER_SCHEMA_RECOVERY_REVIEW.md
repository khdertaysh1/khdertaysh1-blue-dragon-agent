# Advertised-tool recovery and browser diagnostics

Baseline: 8a174607c710f3703439877bbc35ce17c3742d8b. No external website was contacted.

## Confirmed defect

A call rejected by `tool_arguments::prepare` because its schema was absent was
stored by `reasoning::loops::Guard::after` in the same blocked class as denial or
unknown execution. After successful discovery, the identical now-advertised,
schema-valid arguments were still blocked before dispatch. Reproduced through
CLI for web_fetch and browser. This is not evidence of a network/site failure.

The guard now clears ONLY this precise never-dispatched schema rejection when
`prepare` succeeds against the next request's advertised definitions. The call
still follows original scope, approval and dispatch. Discovery alone does not
clear unresolved arguments; native success of the intended tool is required.
Actual denial, malformed arguments and unknown execution remain blocked,
including when they follow a corrected unadvertised call. No limits were raised.

## Browser meaning and privacy

Status returns a connection observation, not page evidence. Its result and UI
explain readiness and a safe next step subject to existing approval/scope; no
automatic navigation or unknown-action replay is added. Partial reports distinguish
native status readiness from page/DOM inspection. Unchanged status remains bounded
by the existing guard, with an explicit blocker rather than implied page success.

`/tool-result` preserves the original session and makes no provider or tool call.
It now shows `native_action` only for an exact browser schema enum value and fixed
native status fields/types: backend none/cdp/extension, ready, port, connection
epoch, reconnect_pending, and relevant boolean connection facts. It reconstructs
status guidance; raw setup/error/page strings, URLs, selectors, pairing tokens
and unknown field names remain redacted. This is historical local diagnostics,
not proof that a page/login is currently valid or protection from complete local
store forgery. Existing request/result integrity checks still apply.

## Phone evidence boundary

E000019 reports invalid_arguments / not_executed / expected advertised tool.
That proves no fetch dispatch for that call; provider_requests=0 in local
inspection does not count earlier model requests. The redacted E000021 string
lengths do NOT identify its action, backend or page result. We have not established
that the phone followed the reproduced discovery-transition sequence.

Minimum further evidence: public version/build hash; sanitized `/tool-result`
for E000019, E000021 and E000023 (native_action, category, execution_status,
fixed error code/type/path, backend/ready and numeric limits); ordered discovery
calls and their registered active_schema_names; turn/step indices and approval
outcome. Exact URLs, tokens, response bodies and the full private session are not
needed. Unknown native error strings need local review/redaction, not blind export.

## Regression checks

- `smoke_schema_recovery.py`: unadvertised → discovery → same input; invalid URL
  remains a failure, browser status can succeed while ready=false and page_inspected=false.
- `smoke_discovered_fetch.py`: real CLI + loopback GET, zero requests before schema
  advertisement, one GET only after explicit approval, repeated GET reuses old
  evidence; rejection produces zero GETs and cannot be cleared by discovery.
- Rust controls: denied/unknown/malformed remain blocked after schema validation;
  later denial does not inherit the old schema exemption; fixed diagnostic values
  do not reveal injected strings; repeated status reports a connection blocker.

Scripted providers and an authenticated mock browser are not a real browser or
physical Android test. Zero findings never establishes site safety.
