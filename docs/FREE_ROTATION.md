# Blue Free automatic model rotation

Select `/model blue-free` (also `/model free` or `/model kilo`) in chat/TUI, or `blue-dragon model free` before launch. The displayed lane is **Blue Free**, backed by Kilo's public `kilo-auto/free` route plus its discovered zero-priced, tool-capable models. It does not contain embedded API keys or model weights. Existing saved providers are preserved; new keyless installs default to this lane.

`/models` opens the searchable free-model picker. Selecting a concrete ID **pins it**, so Blue Dragon never silently changes that choice. Select Blue Free again to enable automatic routing. `--no-rotation` disables client rotation for the process. The public gateway can itself route its automatic alias independently.

## Failure policy

After a request fails **before a response stream begins**, Blue Dragon may switch within the same validated Kilo free catalog:

- HTTP429 only when structured error metadata explicitly identifies a model/upstream-model quota or rate limit.
- HTTP404 only for explicit model-not-found/unavailable codes.
- HTTP502/503 for pre-stream service unavailability, unless metadata identifies a shared/global scope.

Unknown 429, per-IP, anonymous, account, organization, gateway or global quotas stop the turn. HTTP401/402/403, invalid requests, network/TLS failures, cancellation and interrupted/malformed streams do not rotate. No account cycling, key cycling, paid models, purchases, cross-provider fallback or attempts to bypass a shared service limit are performed.

At most **three switches per user turn** are allowed; each candidate is tried at most once in that turn. A failed model receives an in-memory cooldown from `Retry-After`/structured retry seconds, bounded to 1–86400 seconds (429 default300, other default60). Candidate selection skips cooling models. Cooldowns last for the current agent process, not across restarts. An active model can be probed at the beginning of a later turn; rotation does not wait in a retry loop.

Only a failed provider request is sent again with the same current message history. Completed tool actions are **never executed again** by the rotation engine. Once streaming starts, an error stops; there is no automatic replay of partial model output. The session preserves completed tool results. Every switch emits a visible notice and updates the displayed active model; automatic selection stays saved as Blue Free for later launches.

Use `/rotation` for active model, remaining budget, cooldowns and switch history. The last task report includes routing history. If no eligible model remains or the budget is exhausted, the task stops with an explicit error. Free access is external and limited; rotation does not promise unlimited usage or a working model at all times.

## Privacy and verification

All switches remain on the same public Kilo endpoint. Prompts, workspace excerpts and tool results needed for the task go to that service; its upstream model can change. Do not use public free services for confidential data without reviewing their policies. No unrelated provider API key is forwarded.

Offline tests verify scoped versus shared errors, paid/auth refusals, cooldown/budget rules and a full agent sequence with a completed tool followed by a quota error: only the provider request changes model; the completed tool is not repeated. Browser tests and regular CI never consume live free-model quotas. This is an independently written implementation based on public service behavior, not copied AlphaCode routing code. See [MODELS.md](MODELS.md) for catalog filtering and provider setup.
