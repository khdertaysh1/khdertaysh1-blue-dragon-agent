# Current integrated candidate review inventory

This is an explicit inventory, not a claim of complete audit or Alpha parity. Historical reviews in other documents are not automatically counted as newly repeated. The release worker and independent reviewers retain their own scopes. No published tag/asset or version mapping changes are included.

| Area | Current evidence | Remaining gap |
|---|---|---|
| Native tool correction | Actual CLI plus native contracts and negative provenance controls | Independent review of the combined candidate |
| Local path policy | Protected canonical alias/read/write and approval-race tests | General hostile-writer TOCTOU; platform filesystem tests |
| Context | Repeated prose reconstructs losslessly; all originals saved | Unique high-information input still hits capacity; no broad reliability claim |
| Restart | Typed user/freshness/evidence references and unknown envelopes migrated | Actual Android process termination |
| Provider | Typed pre-stream 429 bounded same-route recovery, shared admission, cancellation | Live quota/load and queue UX on phone |
| Browser | Status contract inspected; prior fixture suites remain | Joined long real-DOM investigation and current browser suite |
| Jobs | 20 retained entries per manager, 2 concurrent, 300s/job | Automatic safe archival/recycling not implemented |
| Delegates | 8 retained children per parent session; shared 24 requests/64 tool calls/64KiB output/mail persisted under delegates/<parent-session-id>; 2 concurrent, 6 turns and 90s work deadline per child | Long-team capacity/reallocation needs a separately reviewed design |
| Alpha comparison | Local d49f883 compaction implementation inspected | Current remote-head verification and same-model live comparison not performed in this follow-up |

## Retention decision

Jobs belong to a runtime manager: CLI Agent uses its manager; the TUI shares an application manager across its workers. Completed/cancelled jobs remain among the 20 retained entries until that manager is dropped. A new manager does not recover or replay jobs. Reaching 20 completed jobs can block a 21st job despite zero active jobs; the regression explicitly tests this behavior and retained output. We keep the cap rather than silently deleting unread results or raising memory limits. Safe recycling needs durable bounded archives, explicit result-retention semantics and stable reference lookup; restarting merely resets in-memory capacity and loses unpersisted job output. This is a real constraint for a many-job workflow, not a failure of every long chat. Foreground approved commands do not consume this background-job registry.

## Repository folders

| Folder | Review status in this follow-up |
|---|---|
| src/ | Targeted paths below, not every function audited |
| scripts/ | Added regression/qualification scripts plus affected existing runners inspected |
| .github/workflows/ | CI regression wiring inspected; no remote CI execution here |
| docs/ | New change-specific evidence/limits documented; old claims not globally rewritten |
| tests/ | Included where project test runner exercises them; not separately audited exhaustively |
| browser-extension/ | No new implementation review in this follow-up |
| assets/ | Not reviewed; no edits |
| third_party/ | No imported implementation or license changes; not re-audited |
| validation/ | Historical artifacts not treated as new verification |
| Root build/install/version files | Existing 1.1.9.0/public vs 1.1.9/package mapping retained; no release edits |

## Source-file inventory

“Targeted inspection” means the relevant functions/call sites were read and/or exercised; it does not certify the entire file.

| Source | Current pass |
|---|---|
| src/agent.rs | Targeted inspection in this follow-up |
| src/anthropic.rs | No new dedicated code audit; project tests only where applicable |
| src/artifacts.rs | No new dedicated code audit; project tests only where applicable |
| src/audit.rs | No new dedicated code audit; project tests only where applicable |
| src/audit_engines.rs | No new dedicated code audit; project tests only where applicable |
| src/browser/bridge.rs | Targeted inspection in this follow-up |
| src/browser/cdp.rs | Targeted inspection in this follow-up |
| src/browser/inspection.rs | No new dedicated code audit; project tests only where applicable |
| src/browser/setup.rs | No new dedicated code audit; project tests only where applicable |
| src/browser.rs | Targeted inspection in this follow-up |
| src/config.rs | No new dedicated code audit; project tests only where applicable |
| src/continuity.rs | No new dedicated code audit; project tests only where applicable |
| src/delegates.rs | Targeted inspection in this follow-up |
| src/edit_proposals.rs | Targeted inspection in this follow-up |
| src/intelligence/checks.rs | Targeted inspection in this follow-up |
| src/intelligence/code.rs | Targeted inspection in this follow-up |
| src/intelligence/fresh.rs | Targeted inspection in this follow-up |
| src/intelligence/memory.rs | No new dedicated code audit; project tests only where applicable |
| src/intelligence/mod.rs | No new dedicated code audit; project tests only where applicable |
| src/jobs.rs | Targeted inspection in this follow-up |
| src/lab_report.rs | No new dedicated code audit; project tests only where applicable |
| src/lab_runner.rs | No new dedicated code audit; project tests only where applicable |
| src/local_files.rs | Targeted inspection in this follow-up |
| src/main.rs | No new dedicated code audit; project tests only where applicable |
| src/mcp.rs | No new dedicated code audit; project tests only where applicable |
| src/mcp_protocol.rs | No new dedicated code audit; project tests only where applicable |
| src/models.rs | No new dedicated code audit; project tests only where applicable |
| src/plan.rs | No new dedicated code audit; project tests only where applicable |
| src/project.rs | Targeted inspection in this follow-up |
| src/provider.rs | Targeted inspection in this follow-up |
| src/provider_budget.rs | Targeted inspection in this follow-up |
| src/provider_contract.rs | No new dedicated code audit; project tests only where applicable |
| src/provider_diagnostics.rs | No new dedicated code audit; project tests only where applicable |
| src/provider_traffic.rs | No new dedicated code audit; project tests only where applicable |
| src/reasoning/checkpoint.rs | Targeted inspection in this follow-up |
| src/reasoning/claims.rs | No new dedicated code audit; project tests only where applicable |
| src/reasoning/context.rs | Targeted inspection in this follow-up |
| src/reasoning/loops.rs | No new dedicated code audit; project tests only where applicable |
| src/reasoning/mod.rs | No new dedicated code audit; project tests only where applicable |
| src/rotation.rs | Targeted inspection in this follow-up |
| src/scope.rs | No new dedicated code audit; project tests only where applicable |
| src/session.rs | Targeted inspection in this follow-up |
| src/skills.rs | No new dedicated code audit; project tests only where applicable |
| src/source_discovery.rs | No new dedicated code audit; project tests only where applicable |
| src/source_report.rs | No new dedicated code audit; project tests only where applicable |
| src/source_trace.rs | No new dedicated code audit; project tests only where applicable |
| src/streaming.rs | No new dedicated code audit; project tests only where applicable |
| src/test_coverage.rs | No new dedicated code audit; project tests only where applicable |
| src/test_support.rs | No new dedicated code audit; project tests only where applicable |
| src/tool_arguments.rs | Targeted inspection in this follow-up |
| src/tool_selection.rs | No new dedicated code audit; project tests only where applicable |
| src/tools.rs | Targeted inspection in this follow-up |
| src/tui.rs | No new dedicated code audit; project tests only where applicable |
| src/vision.rs | No new dedicated code audit; project tests only where applicable |

## Environment and remaining acceptance blockers

Real Chromium 156.0.8078.79 is installed. An actual `smoke_url_inspection.mjs` attempt failed before debugger startup; a temporary-profile reproduction exits with `socket() failed: Operation not permitted` in process_singleton_posix. No workaround disabling this environment control was attempted. No real DOM/extension/Firefox success is claimed in this follow-up. This blocks the requested joined real-browser investigation in this execution environment, even though synthetic/browser-contract suites can run.

The distinct structured path/status workload now reaches 30 turns losslessly, while the separate 4339/5289-character high-information Arabic/English workload still completes only 6/13 turns before local context rejection. No arbitrary-content semantic compaction or live-model understanding of encoded history has been established. These limits, independent review, actual Termux testing and final platform CI remain outstanding; the candidate is not declared ready for publication.
