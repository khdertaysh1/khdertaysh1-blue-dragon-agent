mod agent;
mod config;
mod provider;
mod project;
mod skills;
mod session;
mod tools;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::{self, Write};
use std::path::PathBuf;

use agent::{Agent, ApprovalMode};
use config::Settings;

#[derive(Debug, Parser)]
#[command(name = "blue-dragon", about = "Blue Dragon | AI coding agent", version)]
struct Cli {
    #[arg(long, global = true, default_value = ".", help = "Project workspace")]
    root: PathBuf,
    #[arg(
        long,
        global = true,
        env = "BLUE_DRAGON_MODEL",
        default_value = "gpt-4o-mini"
    )]
    model: String,
    #[arg(
        long,
        global = true,
        env = "BLUE_DRAGON_BASE_URL",
        default_value = "https://api.openai.com/v1"
    )]
    base_url: String,
    #[arg(long, global = true, help = "Deny all filesystem writes and commands")]
    read_only: bool,
    #[arg(
        long,
        global = true,
        help = "Resume a saved session by ID; 'latest' uses most recent"
    )]
    resume: Option<String>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start conversational agent with interactive approval prompts
    Chat,
    /// Execute one task; approval is still required for writes/commands
    Run { task: String },
    /// Check local setup without uploading data
    Doctor,
    /// List saved local sessions
    Sessions,
    /// Inspect file types, manifests and language distribution without sending code to AI
    Map,
    /// List user-level local Markdown skills
    Skills,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Blue Dragon error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Cli::parse();
    let root = args
        .root
        .canonicalize()
        .with_context(|| format!("Workspace {:?} is not accessible", args.root))?;
    if !root.is_dir() {
        anyhow::bail!("Workspace must be a directory");
    }
    let settings = Settings::new(root, args.base_url, args.model)?;
    match args.command {
        Some(Command::Doctor) => {
            println!("Blue Dragon v{} | Diagnostic", env!("CARGO_PKG_VERSION"));
            println!(
                "Platform : {} / {}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            println!("Workspace: {}", settings.root.display());
            println!("Provider : {}", settings.safe_endpoint());
            println!("Model    : {}", settings.model);
            println!(
                "API key  : {}",
                if settings.api_key.is_some() {
                    "configured"
                } else {
                    "not configured (local keyless servers supported)"
                }
            );
            println!("Sessions : {}", settings.sessions_dir.display());
            Ok(())
        }
        Some(Command::Sessions) => {
            for info in session::list_sessions(&settings.sessions_dir)? {
                println!("{} | {} | {}", info.id, info.updated_at, info.workspace);
            }
            Ok(())
        }
        Some(Command::Map) => {
            println!("{}", serde_json::to_string_pretty(&project::inspect(&settings.root)?)?);
            Ok(())
        }
        Some(Command::Skills) => {
            println!("{}", serde_json::to_string_pretty(&skills::list(&settings.sessions_dir)?)?);
            Ok(())
        }
        cmd => {
            let approval = if args.read_only {
                ApprovalMode::ReadOnly
            } else {
                ApprovalMode::Ask
            };
            let mut agent = Agent::new(settings, approval, args.resume.as_deref())?;
            print_banner();
            match cmd {
                Some(Command::Run { task }) => agent.turn(&task),
                None | Some(Command::Chat) => {
                    println!("Type /help for commands, /exit to quit.");
                    loop {
                        print!("\nblue-dragon> ");
                        io::stdout().flush()?;
                        let mut text = String::new();
                        if io::stdin().read_line(&mut text)? == 0 {
                            break;
                        }
                        let text = text.trim();
                        match text {
                            "/exit" | "/quit" => break,
                            "/help" => {
                                println!("/help /exit /session /diff /status /map /skills /skill <name> /doctor — or enter a task");
                                continue;
                            }
                            "/session" => {
                                println!("Session: {}", agent.session_id());
                                continue;
                            }
                            "/diff" => {
                                println!("{}", agent.git_diff()?);
                                continue;
                            }
                            "/map" => {
                                println!("{}", serde_json::to_string_pretty(&agent.project_map()?)?);
                                continue;
                            }
                            "/status" => {
                                println!("{}", serde_json::to_string_pretty(&agent.git_status()?)?);
                                continue;
                            }
                            "/skills" => {
                                println!("{}", serde_json::to_string_pretty(&agent.list_skills()?)?);
                                continue;
                            }
                            name if name.starts_with("/skill ") => {
                                let name = name.trim_start_matches("/skill ").trim();
                                match agent.activate_skill(name) {
                                    Ok(()) => println!("Activated local skill: {name}"),
                                    Err(error) => eprintln!("Skill error: {error:#}"),
                                }
                                continue;
                            }
                            "/doctor" => {
                                println!(
                                    "Model: {} | root: {}",
                                    agent.model(),
                                    agent.root().display()
                                );
                                continue;
                            }
                            "" => continue,
                            _ => {}
                        }
                        if let Err(error) = agent.turn(text) {
                            eprintln!("Task failed: {error:#}");
                        }
                    }
                    println!("Session saved. Resume with --resume {}", agent.session_id());
                    Ok(())
                }
                _ => unreachable!(),
            }
        }
    }
}

fn print_banner() {
    println!("\n  ╔═══════════════════════════════════════╗");
    println!("  ║  BLUE DRAGON  ◆  AI CODING AGENT     ║");
    println!("  ╚═══════════════════════════════════════╝");
    println!("  Consent-first • Cross-platform • Rust");
}
