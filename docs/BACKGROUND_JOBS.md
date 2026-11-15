# Explicit background command jobs (experimental)

In an interactive session, ask the agent to run a longer approved command using
`start_job`. It returns a live job ID immediately. **Started is not completed.**
Use `/jobs` to list, `/jobs ID` to inspect state, or `/jobs cancel ID` to request
cancellation without needing another model response. The model can inspect
`job_status`, read `job_output`, list jobs, and cancel them. Ordinary
`run_command` remains unchanged.

## Limits and evidence

- Unix/macOS/Android only initially. Windows returns an explicit unsupported
  error; it does not silently provide weaker process cancellation.
- Two live commands at most, 20 retained jobs per application, and an explicit
  deadline of 1–300 seconds. Each start requires human approval. There is no
  automatic conversion of ordinary foreground commands to background work.
- Each output stream retains its latest 64 KiB. Readers keep draining earlier
  bytes to prevent deadlocks. Reads return byte cursors and dropped-byte counts.
  Text is decoded lossily; a cursor boundary may split a UTF-8 character.
  `complete` indicates the pipe closed; a terminal process status does not imply
  all pipe output has already arrived.
- `command_success` is true only after the direct child exits with code zero.
  It does not prove correctness, absence of security problems, or completion of
  independently detached processes. Running jobs invalidate acceptance checks.
- Status/output reads do not get cached or trip the repeated-tool guard. The
  normal task step budget remains in force; the agent should not busy-poll.

## Lifetime and cancellation

These are **application-owned jobs, not a daemon**. They can continue between
chat turns while the interactive application remains open. Graceful application
exit requests cancellation and waits for supervisors. Single-shot CLI exit also
ends its jobs. There is no disk persistence, automatic replay or reconnection to
old PIDs. Session transcript entries about old jobs are historical evidence only.

Each job uses a fresh Unix process group. Cancellation, deadline expiry and
normal leader exit kill remaining processes in that group. The leader is
observed without reaping before group cleanup, preventing reuse of its PID in
that cleanup interval. `cancel_job` only acknowledges the request; inspect status
for the terminal result.

This is **not a security sandbox**. Commands which detach with `setsid`, change
process group, daemonize, or escape normal process ownership are unsupported and
may outlive this supervision. OS force-kill, device shutdown and Android process
management are not graceful cleanup. File/network side effects are not undone.
Inspect state before retrying any interrupted command. Live Android behavior
still requires physical Termux testing.
