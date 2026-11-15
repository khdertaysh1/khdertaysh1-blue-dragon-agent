//! Explicit, runtime-owned command jobs. No daemon, restart recovery or replay.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
#[cfg(unix)]
use std::io::Read;
use std::path::Path;
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(unix)]
use std::time::Duration;
use std::time::Instant;

#[cfg(any(unix, test))]
const CAP: usize = 64 * 1024;
#[cfg(unix)]
const MAX_JOBS: usize = 20;
#[derive(Default)]
struct Capture {
    bytes: VecDeque<u8>,
    total: u64,
    complete: bool,
    read_failed: bool,
}
impl Capture {
    #[cfg(any(unix, test))]
    fn append(&mut self, data: &[u8]) {
        self.total += data.len() as u64;
        for byte in data {
            if self.bytes.len() == CAP {
                self.bytes.pop_front();
            }
            self.bytes.push_back(*byte);
        }
    }
    fn output(&self, cursor: u64, limit: usize) -> Value {
        let first = self.total - self.bytes.len() as u64;
        let start = cursor.max(first).min(self.total);
        let bytes: Vec<u8> = self
            .bytes
            .iter()
            .skip((start - first) as usize)
            .take(limit)
            .copied()
            .collect();
        json!({"text":String::from_utf8_lossy(&bytes),"next_cursor":start+bytes.len() as u64,"dropped_bytes":first.saturating_sub(cursor),"total_bytes":self.total,"complete":self.complete,"read_failed":self.read_failed,"encoding":"UTF-8 lossy; cursors count bytes, not characters"})
    }
}
struct Job {
    state: Mutex<(&'static str, Option<i32>)>,
    stdout: Arc<Mutex<Capture>>,
    stderr: Arc<Mutex<Capture>>,
    cancel: AtomicBool,
    started: Instant,
    supervisor: Mutex<Option<std::thread::JoinHandle<()>>>,
}
#[derive(Default)]
pub struct Jobs {
    entries: Mutex<BTreeMap<String, Arc<Job>>>,
    closed: AtomicBool,
}
impl Jobs {
    pub fn cancel_all(&self) {
        for job in self.entries.lock().unwrap().values() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
    pub fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let entries = self.entries.lock().unwrap();
        for job in entries.values() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        for job in entries.values() {
            if let Some(handle) = job.supervisor.lock().unwrap().take() {
                let _ = handle.join();
            }
        }
    }
    pub fn active(&self) -> bool {
        self.entries
            .lock()
            .unwrap()
            .values()
            .any(|j| j.state.lock().unwrap().0 == "running")
    }
    pub fn start(
        &self,
        root: &Path,
        program: &str,
        args: &[String],
        seconds: u64,
        task_cancel: Arc<AtomicBool>,
    ) -> Result<Value> {
        if program.is_empty()
            || program.len() > 200
            || args.len() > 30
            || args.iter().any(|a| a.len() > 1000)
            || !(1..=300).contains(&seconds)
        {
            bail!("Invalid command or timeout");
        }
        if task_cancel.load(Ordering::Relaxed) {
            bail!("Task cancelled; job not started");
        }
        #[cfg(not(unix))]
        {
            let _ = (root, program, args, seconds, task_cancel);
            bail!("Background jobs currently require Unix/Android process groups; use run_command on this platform");
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let mut entries = self.entries.lock().unwrap();
            if task_cancel.load(Ordering::Relaxed) {
                bail!("Task cancelled; job not started");
            }
            if self.closed.load(Ordering::SeqCst) {
                bail!("Job manager is shutting down");
            }
            if entries.len() >= MAX_JOBS {
                bail!("Job retention limit reached; restart the application after inspecting completed jobs");
            }
            if entries
                .values()
                .filter(|j| j.state.lock().unwrap().0 == "running")
                .count()
                >= 2
            {
                bail!("Two jobs already running; wait or cancel one");
            }
            let mut random = [0u8; 16];
            getrandom::fill(&mut random)
                .map_err(|e| anyhow::anyhow!("Job ID generation failed: {e}"))?;
            let id = random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let mut command = Command::new(program);
            command
                .args(args)
                .current_dir(root)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .env_clear()
                .process_group(0);
            for var in [
                "PATH",
                "HOME",
                "USERPROFILE",
                "SYSTEMROOT",
                "WINDIR",
                "TEMP",
                "TMP",
                "TMPDIR",
                "CARGO_HOME",
                "RUSTUP_HOME",
                "LANG",
                "LC_ALL",
                "VIRTUAL_ENV",
            ] {
                if let Some(value) = std::env::var_os(var) {
                    command.env(var, value);
                }
            }
            let mut child = command.spawn().context("Could not start job")?;
            let pid = child.id() as i32;
            let job = Arc::new(Job {
                state: Mutex::new(("running", None)),
                stdout: Arc::new(Mutex::new(Capture::default())),
                stderr: Arc::new(Mutex::new(Capture::default())),
                cancel: AtomicBool::new(false),
                started: Instant::now(),
                supervisor: Mutex::new(None),
            });
            let out = child.stdout.take().context("Missing stdout pipe")?;
            let err = child.stderr.take().context("Missing stderr pipe")?;
            drain(out, job.stdout.clone());
            drain(err, job.stderr.clone());
            let worker = job.clone();
            let supervisor = std::thread::spawn(move || {
                let result = loop {
                    let stop = if worker.cancel.load(Ordering::Relaxed)
                        || task_cancel.load(Ordering::Relaxed)
                    {
                        Some("cancelled")
                    } else if worker.started.elapsed() >= Duration::from_secs(seconds) {
                        Some("timed_out")
                    } else {
                        None
                    };
                    if let Some(state) = stop {
                        // Only a live child created by this manager is targeted. Never restore PID authority from disk.
                        unsafe {
                            libc::kill(-pid, libc::SIGKILL);
                        }
                        let _ = child.kill();
                        let _ = child.wait();
                        break (state, None);
                    }
                    // Observe exit without reaping: the leader keeps its PID reserved until
                    // remaining same-group descendants have been stopped.
                    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
                    let waited = unsafe {
                        libc::waitid(
                            libc::P_PID,
                            pid as libc::id_t,
                            info.as_mut_ptr(),
                            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                        )
                    };
                    if waited != 0 {
                        unsafe {
                            libc::kill(-pid, libc::SIGKILL);
                        }
                        let _ = child.kill();
                        let _ = child.wait();
                        break ("unknown", None);
                    }
                    if unsafe { info.assume_init().si_pid() } != 0 {
                        unsafe {
                            libc::kill(-pid, libc::SIGKILL);
                        }
                        break match child.wait() {
                            Ok(status) => ("exited", status.code()),
                            Err(_) => ("unknown", None),
                        };
                    }
                    std::thread::sleep(Duration::from_millis(40));
                };
                *worker.state.lock().unwrap() = result;
            });
            *job.supervisor.lock().unwrap() = Some(supervisor);
            entries.insert(id.clone(), job);
            Ok(
                json!({"ok":true,"job_id":id,"state":"running","command_success":false,"note":"Started only, not completed. Owned by this application; no restart persistence. Keep child processes in their inherited process group."}),
            )
        }
    }
    pub fn list(&self) -> Value {
        let ids = self
            .entries
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        json!({"jobs":ids.iter().filter_map(|id|self.status(id).ok()).collect::<Vec<_>>(),"note":"Live application only. No jobs are recovered or replayed after restart."})
    }
    fn get(&self, id: &str) -> Result<Arc<Job>> {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .context("Unknown job in this runtime")
    }
    pub fn status(&self, id: &str) -> Result<Value> {
        let job = self.get(id)?;
        let (state, code) = *job.state.lock().unwrap();
        Ok(
            json!({"ok":true,"job_id":id,"state":state,"exit_code":code,"command_success":state=="exited" && code==Some(0),"elapsed_seconds":job.started.elapsed().as_secs(),"stdout_bytes":job.stdout.lock().unwrap().total,"stderr_bytes":job.stderr.lock().unwrap().total}),
        )
    }
    pub fn output(&self, id: &str, stream: &str, cursor: u64, limit: usize) -> Result<Value> {
        if limit == 0 || limit > 16000 {
            bail!("Output limit must be 1..16000 bytes");
        }
        let job = self.get(id)?;
        let capture = match stream {
            "stdout" => &job.stdout,
            "stderr" => &job.stderr,
            _ => bail!("Stream must be stdout or stderr"),
        };
        let capture = capture.lock().unwrap();
        if cursor > capture.total {
            bail!("Output cursor is beyond the observed stream");
        }
        let output = capture.output(cursor, limit);
        Ok(json!({"ok":true,"job_id":id,"stream":stream,"output":output}))
    }
    pub fn cancel(&self, id: &str) -> Result<Value> {
        let job = self.get(id)?;
        job.cancel.store(true, Ordering::Relaxed);
        Ok(
            json!({"ok":true,"job_id":id,"cancellation_requested":true,"note":"Inspect job_status for the terminal outcome; this request is not proof of termination."}),
        )
    }
}
impl Drop for Jobs {
    fn drop(&mut self) {
        self.shutdown();
    }
}
#[cfg(unix)]
fn drain(mut reader: impl Read + Send + 'static, capture: Arc<Mutex<Capture>>) {
    std::thread::spawn(move || {
        let mut bytes = [0u8; 4096];
        loop {
            match reader.read(&mut bytes) {
                Ok(0) => {
                    capture.lock().unwrap().complete = true;
                    break;
                }
                Err(_) => {
                    let mut state = capture.lock().unwrap();
                    state.read_failed = true;
                    state.complete = true;
                    break;
                }
                Ok(n) => capture.lock().unwrap().append(&bytes[..n]),
            }
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_is_bounded_and_cursor_reports_loss() {
        let mut capture = Capture::default();
        capture.append(&vec![b'x'; CAP + 50]);
        assert_eq!(capture.bytes.len(), CAP);
        let output = capture.output(0, 10);
        assert_eq!(output["dropped_bytes"], 50);
        assert_eq!(output["next_cursor"], 60);
    }
    #[cfg(unix)]
    #[test]
    fn retention_limit_preserves_completed_results_and_restart_never_replays() {
        let root = tempfile::tempdir().unwrap();
        let jobs = Jobs::default();
        let token = Arc::new(AtomicBool::new(false));
        let mut ids = Vec::new();
        for _ in 0..MAX_JOBS {
            let started = jobs
                .start(
                    root.path(),
                    "sh",
                    &["-c".into(), "printf kept".into()],
                    5,
                    token.clone(),
                )
                .unwrap();
            let id = started["job_id"].as_str().unwrap().to_owned();
            let deadline = Instant::now() + Duration::from_secs(3);
            while jobs.status(&id).unwrap()["state"] == "running" && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(jobs.status(&id).unwrap()["exit_code"], 0);
            ids.push(id);
        }
        assert!(jobs
            .start(
                root.path(),
                "sh",
                &["-c".into(), "touch must-not-run".into()],
                5,
                token.clone()
            )
            .unwrap_err()
            .to_string()
            .contains("retention limit"));
        assert_eq!(jobs.list()["jobs"].as_array().unwrap().len(), MAX_JOBS);
        for id in &ids {
            assert_eq!(
                jobs.output(id, "stdout", 0, 64).unwrap()["output"]["text"],
                "kept"
            );
        }
        jobs.cancel_all();
        assert_eq!(jobs.list()["jobs"].as_array().unwrap().len(), MAX_JOBS);
        let fresh = Jobs::default();
        assert!(fresh.list()["jobs"].as_array().unwrap().is_empty());
        assert!(fresh.status(&ids[0]).is_err());
        assert!(!root.path().join("must-not-run").exists());
    }
    #[cfg(unix)]
    #[test]
    fn jobs_finish_and_cancel_without_replaying() {
        let root = tempfile::tempdir().unwrap();
        let jobs = Jobs::default();
        let task = Arc::new(AtomicBool::new(false));
        let started = jobs
            .start(
                root.path(),
                "sh",
                &["-c".into(), "printf hello; exit 7".into()],
                5,
                task.clone(),
            )
            .unwrap();
        let id = started["job_id"].as_str().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while jobs.status(id).unwrap()["state"] == "running" && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(jobs.status(id).unwrap()["exit_code"], 7);
        assert_eq!(jobs.status(id).unwrap()["command_success"], false);
        let started = jobs
            .start(root.path(), "sleep", &["60".into()], 5, task)
            .unwrap();
        let id = started["job_id"].as_str().unwrap();
        jobs.cancel(id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while jobs.status(id).unwrap()["state"] == "running" && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(jobs.status(id).unwrap()["state"], "cancelled");
        assert!(jobs.status("invented").is_err());
    }
    #[cfg(unix)]
    fn wait_terminal(jobs: &Jobs, id: &str) -> Value {
        let end = Instant::now() + Duration::from_secs(4);
        loop {
            let state = jobs.status(id).unwrap();
            if state["state"] != "running" {
                return state;
            }
            assert!(Instant::now() < end, "job failed to terminate");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    #[cfg(unix)]
    #[test]
    fn flood_output_drains_and_retains_only_tail() {
        let root = tempfile::tempdir().unwrap();
        let jobs = Jobs::default();
        let result = jobs
            .start(
                root.path(),
                "python",
                &[
                    "-c".into(),
                    "import sys; sys.stdout.write('x'*200000); sys.stderr.write('y'*200000)".into(),
                ],
                5,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        let id = result["job_id"].as_str().unwrap();
        assert_eq!(wait_terminal(&jobs, id)["command_success"], true);
        let end = Instant::now() + Duration::from_secs(3);
        for stream in ["stdout", "stderr"] {
            loop {
                let output = jobs.output(id, stream, 0, 100).unwrap();
                if output["output"]["complete"] == true {
                    assert_eq!(output["output"]["total_bytes"], 200000);
                    assert_eq!(output["output"]["dropped_bytes"], 200000 - CAP);
                    break;
                }
                assert!(Instant::now() < end);
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    #[cfg(unix)]
    #[test]
    fn timeout_cleans_same_group_descendants_before_they_write() {
        let root = tempfile::tempdir().unwrap();
        let jobs = Jobs::default();
        let result = jobs
            .start(
                root.path(),
                "sh",
                &[
                    "-c".into(),
                    "(sleep 2; printf unexpected > escaped.txt) & wait".into(),
                ],
                1,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        let id = result["job_id"].as_str().unwrap();
        assert_eq!(wait_terminal(&jobs, id)["state"], "timed_out");
        std::thread::sleep(Duration::from_millis(1200));
        assert!(!root.path().join("escaped.txt").exists());
    }
    #[cfg(unix)]
    #[test]
    fn natural_exit_cleans_remaining_same_group_children() {
        let root = tempfile::tempdir().unwrap();
        let jobs = Jobs::default();
        let result = jobs
            .start(
                root.path(),
                "sh",
                &[
                    "-c".into(),
                    "(sleep 1; printf unexpected > child.txt) & exit 0".into(),
                ],
                5,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        assert_eq!(
            wait_terminal(&jobs, result["job_id"].as_str().unwrap())["command_success"],
            true
        );
        std::thread::sleep(Duration::from_millis(1200));
        assert!(!root.path().join("child.txt").exists());
    }
    #[cfg(unix)]
    #[test]
    fn capacity_scope_and_cancelled_starts_are_enforced() {
        let root = tempfile::tempdir().unwrap();
        let jobs = Jobs::default();
        let other = Jobs::default();
        let task = Arc::new(AtomicBool::new(false));
        let a = jobs
            .start(root.path(), "sleep", &["60".into()], 5, task.clone())
            .unwrap();
        jobs.start(root.path(), "sleep", &["60".into()], 5, task.clone())
            .unwrap();
        assert!(jobs
            .start(root.path(), "sleep", &["60".into()], 5, task.clone())
            .is_err());
        assert!(other.cancel(a["job_id"].as_str().unwrap()).is_err());
        task.store(true, Ordering::Relaxed);
        assert_eq!(
            wait_terminal(&jobs, a["job_id"].as_str().unwrap())["state"],
            "cancelled"
        );
        assert!(jobs.start(root.path(), "echo", &[], 5, task).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn dropping_manager_stops_live_groups() {
        let root = tempfile::tempdir().unwrap();
        {
            let jobs = Jobs::default();
            jobs.start(
                root.path(),
                "sh",
                &[
                    "-c".into(),
                    "sleep 1; printf unexpected > after-drop.txt".into(),
                ],
                5,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        }
        std::thread::sleep(Duration::from_millis(1200));
        assert!(!root.path().join("after-drop.txt").exists());
    }
    #[cfg(unix)]
    #[test]
    fn invalid_start_and_spawn_failure_leave_no_jobs() {
        let root = tempfile::tempdir().unwrap();
        let jobs = Jobs::default();
        let task = Arc::new(AtomicBool::new(false));
        assert!(jobs
            .start(root.path(), "echo", &[], 0, task.clone())
            .is_err());
        assert!(jobs
            .start(root.path(), "echo", &[], 301, task.clone())
            .is_err());
        assert!(jobs
            .start(
                root.path(),
                "blue-dragon-deliberately-missing-binary",
                &[],
                5,
                task
            )
            .is_err());
        assert!(jobs.list()["jobs"].as_array().unwrap().is_empty());
    }
    #[cfg(not(unix))]
    #[test]
    fn unsupported_platform_never_spawns_a_job() {
        let root = tempfile::tempdir().unwrap();
        let jobs = Jobs::default();
        let error = jobs
            .start(
                root.path(),
                "echo",
                &[],
                5,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap_err();
        assert!(error.to_string().contains("require Unix/Android"));
        assert!(jobs.list()["jobs"].as_array().unwrap().is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn latched_cancel_survives_task_flag_reset_and_shutdown_rejects_new_jobs() {
        let root = tempfile::tempdir().unwrap();
        let jobs = Jobs::default();
        let task = Arc::new(AtomicBool::new(false));
        let result = jobs
            .start(root.path(), "sleep", &["60".into()], 60, task.clone())
            .unwrap();
        jobs.cancel_all();
        task.store(false, Ordering::Relaxed);
        assert_eq!(
            wait_terminal(&jobs, result["job_id"].as_str().unwrap())["state"],
            "cancelled"
        );
        jobs.shutdown();
        assert!(jobs
            .start(root.path(), "echo", &[], 5, task)
            .unwrap_err()
            .to_string()
            .contains("shutting down"));
    }
}
