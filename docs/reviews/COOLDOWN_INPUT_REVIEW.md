# Interactive input during provider cooldown

The synchronous CLI owns stdin, including approval prompts. Retrying a rejected request while stdin is unread can ignore a newly entered stop/scope instruction. Interactive chat now persists the failed turn and returns to its prompt on HTTP 429 rather than retrying automatically. A subsequent turn still passes shared quota admission; this does not bypass Retry-After. One-shot and TUI bounded retry behavior remains available.

In the TUI, submitting a nonempty draft during work requests cancellation and retains the draft. After work stops, Enter submits it. A `/scope clear` therefore applies before any subsequent task. The draft is retained in the running UI, not guaranteed durable across process termination; the original turn and cancellation report remain saved. No unknown tool action is replayed.

`smoke_cooldown_steering.py` sends input only after the local HTTP server actually emits 429 (Retry-After 3), observes beyond the old retry deadline, and checks persisted scope and original instructions. It uses CLI pipes and a 40x24 POSIX PTY. Baseline 87063a4 makes two requests before processing the new scope. The corrected paths make one request; TUI records cancellation and accepts the retained draft without retyping. This is not Android or live-provider evidence.

`smoke_quota_resume.py` retains scope-specific, no-rotation, one-command, no-retry-for-400/default/long-delay checks. Its preloaded chat case now expects the second user instruction in the next request, not an automatic repeat of the old request. The actual delayed-input test supplies the stronger steering evidence.
