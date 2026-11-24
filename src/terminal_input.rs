//! Single CLI input reader. Paste is conversation data, never an approval.
use anyhow::{bail, Result};
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute, terminal,
};
use std::{
    collections::VecDeque,
    io::{self, IsTerminal, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    time::Duration,
};
static MACHINE_OUTPUT: AtomicBool = AtomicBool::new(false);
pub fn prompt_output() -> Box<dyn Write> {
    if MACHINE_OUTPUT.load(Ordering::Relaxed) {
        Box::new(io::stderr())
    } else {
        Box::new(io::stdout())
    }
}
static CANCELLED: AtomicBool = AtomicBool::new(false);
pub fn cancelled() -> bool {
    CANCELLED.load(Ordering::Relaxed)
}
#[cfg(unix)]
extern "C" fn interrupt(_: libc::c_int) {
    CANCELLED.store(true, Ordering::Relaxed);
}
#[derive(Default)]
struct Input {
    draft: String,
    queued: VecDeque<String>,
}
static INPUT: OnceLock<Mutex<Input>> = OnceLock::new();
fn state() -> &'static Mutex<Input> {
    INPUT.get_or_init(Default::default)
}
pub struct Guard {
    interactive: bool,
    machine: bool,
    #[cfg(unix)]
    previous: Option<libc::sighandler_t>,
}
impl Guard {
    pub fn start() -> Result<Self> {
        Self::with_output(false)
    }
    pub fn with_output(machine: bool) -> Result<Self> {
        let interactive = io::stdin().is_terminal()
            && if machine {
                io::stderr().is_terminal()
            } else {
                io::stdout().is_terminal()
            };
        #[cfg(unix)]
        let mut previous = None;
        if interactive {
            terminal::enable_raw_mode()?;
            #[cfg(unix)]
            unsafe {
                let mut t: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(libc::STDIN_FILENO, &mut t) == 0 {
                    t.c_oflag |= libc::OPOST | libc::ONLCR;
                    t.c_lflag |= libc::ISIG;
                    libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &t);
                }
                previous = Some(libc::signal(
                    libc::SIGINT,
                    interrupt as *const () as libc::sighandler_t,
                ));
            }
            if machine {
                execute!(io::stderr(), EnableBracketedPaste)?;
            } else {
                execute!(io::stdout(), EnableBracketedPaste)?;
            }
        }
        MACHINE_OUTPUT.store(machine, Ordering::Relaxed);
        Ok(Self {
            interactive,
            machine,
            #[cfg(unix)]
            previous,
        })
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        MACHINE_OUTPUT.store(false, Ordering::Relaxed);
        if self.interactive {
            if self.machine {
                let _ = execute!(io::stderr(), DisableBracketedPaste);
            } else {
                let _ = execute!(io::stdout(), DisableBracketedPaste);
            }
            let _ = terminal::disable_raw_mode();
            #[cfg(unix)]
            if let Some(previous) = self.previous {
                unsafe {
                    libc::signal(libc::SIGINT, previous);
                }
            }
        }
    }
}
fn interactive() -> bool {
    terminal::is_raw_mode_enabled().unwrap_or(false) && io::stdin().is_terminal()
}
fn append(i: &mut Input, s: &str) -> Result<()> {
    if i.draft.len() + s.len() > 1024 * 1024 {
        bail!("Input exceeds 1 MiB; no truncated task submitted");
    }
    i.draft.push_str(s);
    Ok(())
}
fn collect(i: &mut Input, e: Event, echo: bool) -> Result<Option<bool>> {
    match e {
        Event::Paste(s) => {
            append(i, &s)?;
            if echo {
                print!("{s}");
            }
        }
        Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
            KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(false))
            }
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                CANCELLED.store(true, Ordering::Relaxed);
                return Ok(Some(true));
            }
            KeyCode::Enter => {
                let s = std::mem::take(&mut i.draft);
                if !s.trim().is_empty() {
                    i.queued.push_back(s);
                }
                if echo {
                    println!();
                }
                return Ok(Some(true));
            }
            KeyCode::Backspace => {
                if i.draft.pop().is_some() && echo {
                    print!("\u{8} \u{8}");
                }
            }
            KeyCode::Char(c) => {
                append(i, &c.to_string())?;
                if echo {
                    print!("{c}");
                }
            }
            _ => {}
        },
        _ => {}
    }
    io::stdout().flush()?;
    Ok(None)
}
pub fn task() -> Result<Option<String>> {
    CANCELLED.store(false, Ordering::Relaxed);
    if !interactive() {
        let mut s = String::new();
        return Ok(if io::stdin().read_line(&mut s)? == 0 {
            None
        } else {
            Some(s)
        });
    }
    let mut i = state().lock().unwrap();
    loop {
        if cancelled() {
            i.draft.clear();
            println!("Input cancelled");
            return Ok(Some(String::new()));
        }
        if let Some(s) = i.queued.pop_front() {
            return Ok(Some(s));
        }
        if event::poll(Duration::from_millis(100))?
            && collect(&mut i, event::read()?, true)? == Some(false)
        {
            return Ok(None);
        }
    }
}
pub fn prepare_approval() -> Result<()> {
    if interactive() {
        let mut i = state().lock().unwrap();
        while event::poll(Duration::ZERO)? {
            let e = event::read()?;
            if matches!(e,Event::Key(k) if k.code==KeyCode::Enter) {
                append(&mut i, "\n")?;
            } else {
                let _ = collect(&mut i, e, false)?;
            }
        }
        if !i.draft.is_empty() {
            writeln!(
                prompt_output(),
                "Conversation draft retained; approval requires a NEW Y or N choice."
            )?;
        }
    }
    Ok(())
}
pub fn approval() -> Result<bool> {
    if !interactive() {
        let mut s = String::new();
        if io::stdin().read_line(&mut s)? == 0 {
            bail!("approval_eof: input closed; no action started");
        }
        return match s.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => Ok(true),
            "n" | "no" => Ok(false),
            _ => bail!("approval_invalid_input: expected explicit y/n; no action started"),
        };
    }
    loop {
        if cancelled() {
            bail!("approval_cancelled: explicit cancellation; no action started");
        }
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        match event::read()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Char('y' | 'Y') => {
                    writeln!(prompt_output(), "y")?;
                    return Ok(true);
                }
                KeyCode::Char('n' | 'N') => {
                    writeln!(prompt_output(), "n")?;
                    return Ok(false);
                }
                KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    bail!("approval_eof: input closed; no action started")
                }
                KeyCode::Esc => {
                    bail!("approval_cancelled: explicit cancellation; no action started")
                }
                _ => {}
            },
            Event::Paste(s) => {
                append(&mut state().lock().unwrap(), &s)?;
                writeln!(
                    prompt_output(),
                    "Pasted draft retained; choose Y/N separately."
                )?;
            }
            _ => {}
        }
    }
}
