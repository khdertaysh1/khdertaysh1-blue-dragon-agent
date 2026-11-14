mod agent;
mod anthropic;
mod browser;
mod config;
mod mcp;
mod mcp_protocol;
mod models;
mod plan;
mod project;
mod provider;
mod rotation;
mod session;
mod skills;
mod streaming;
#[cfg(test)]
mod test_support;
mod tools;
mod tui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use agent::{Agent, ApprovalMode};
use config::Settings;

#[derive(Debug, Parser)]
#[command(name = "blue-dragon", about = "Blue Dragon | AI coding agent", version)]
struct Cli {
    #[arg(long, global = true, default_value = ".", help = "Project workspace")]
    root: PathBuf,
    #[arg(long, global = true, env = "BLUE_DRAGON_MODEL")]
    model: Option<String>,
    #[arg(long, global = true, env = "BLUE_DRAGON_BASE_URL")]
    base_url: Option<String>,
    #[arg(
        long,
        global = true,
        help = "Deny writes, commands, web fetches and external MCP actions"
    )]
    read_only: bool,
    #[arg(long, global = true, env = "BLUE_DRAGON_PROVIDER")]
    provider: Option<String>,
    #[arg(
        long,
        global = true,
        help = "Resume a saved session by ID; 'latest' uses most recent"
    )]
    resume: Option<String>,
    #[arg(
        long,
        global = true,
        env = "BLUE_DRAGON_MCP_CONFIG",
        help = "Opt-in JSON file listing trusted local MCP servers"
    )]
    mcp_config: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        help = "Use non-streaming model responses for older compatible servers"
    )]
    no_stream: bool,
    #[arg(
        long,
        global = true,
        help = "Disable automatic Blue Free model rotation"
    )]
    no_rotation: bool,
    #[arg(long, global=true, default_value_t=24, value_parser=clap::value_parser!(u16).range(1..=100))]
    max_steps: u16,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Open and review a URL automatically (first-use Termux browser setup included)
    Inspect { url: String },
    #[command(external_subcommand)]
    Url(Vec<String>),
    /// Run the authenticated localhost extension bridge until Ctrl+C
    BrowserBridge {
        #[arg(long, default_value_t = 9269)]
        port: u16,
    },
    /// List keyless free models; --refresh updates the live Kilo catalog
    Models {
        #[arg(long)]
        refresh: bool,
    },
    /// Save a provider/model selection, without making a model request
    Model {
        target_provider: String,
        target_model: Option<String>,
    },
    /// Start conversational agent with interactive approval prompts
    Chat,
    /// Full-screen terminal with live answers and per-action approvals
    Tui,
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
    if let Some(Command::Inspect { url }) = &args.command {
        browser::validate_url(url)?;
    }
    if let Some(Command::Url(words)) = &args.command {
        if words.len() != 1 || browser::inspection::bare_url(&words[0]).is_none() {
            anyhow::bail!("Unknown command. Use blue-dragon inspect https://example.com or paste a URL in chat/TUI.");
        }
    }
    let home = config::app_home()?;
    match &args.command {
        Some(Command::BrowserBridge { port }) => {
            let mut browser = browser::Browser::new(&home);
            println!(
                "{}",
                serde_json::to_string_pretty(&browser.command(&format!("start {port}"))?)?
            );
            io::stdout().flush()?;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
        Some(Command::Models { refresh }) => {
            println!("{}", models::menu());
            println!("{}", models::kilo_listing(&home, *refresh)?);
            return Ok(());
        }
        Some(Command::Model {
            target_provider,
            target_model,
        }) => {
            let selection = models::Selection::new(target_provider, target_model.as_deref())?;
            models::save(&home, &selection)?;
            println!(
                "Saved {} / {}. No request sent. Run blue-dragon tui.",
                selection.provider, selection.model
            );
            return Ok(());
        }
        _ => {}
    }
    if args.provider.is_some() && args.base_url.is_some() {
        anyhow::bail!("Use either --provider or --base-url, not both");
    }
    let selection = if args.base_url.is_some() {
        None
    } else if let Some(provider) = args.provider.as_deref() {
        Some(models::Selection::new(provider, args.model.as_deref())?)
    } else if let Some(mut saved) = models::load(&home)? {
        if let Some(model) = args.model.as_deref() {
            saved = models::Selection::new(&saved.provider, Some(model))?;
        }
        Some(saved)
    } else {
        // Preserve existing explicit key/model configurations; fresh installs start keyless.
        let provider = if args.model.is_some()
            || std::env::var_os("BLUE_DRAGON_API_KEY").is_some()
            || std::env::var_os("OPENAI_API_KEY").is_some()
        {
            "openai"
        } else {
            "free"
        };
        Some(models::Selection::new(provider, args.model.as_deref())?)
    };

    let root = args
        .root
        .canonicalize()
        .with_context(|| format!("Workspace {:?} is not accessible", args.root))?;
    if !root.is_dir() {
        anyhow::bail!("Workspace must be a directory");
    }
    let base = selection
        .as_ref()
        .map(|s| s.base().to_owned())
        .or(args.base_url)
        .unwrap();
    let model = selection
        .as_ref()
        .map(|s| s.model.clone())
        .or(args.model)
        .unwrap_or_else(|| "gpt-4o-mini".into());
    let mut settings = Settings::new(root, base, model)?;
    if let Some(selection) = selection {
        settings.api_key = selection.key();
        // Legacy global key is accepted only for OpenAI, never sent to free/local/other services.
        if selection.provider == "openai" {
            settings.api_key = std::env::var("BLUE_DRAGON_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
                .or(settings.api_key);
        }
    }
    settings.mcp_config = args.mcp_config;
    settings.stream = !args.no_stream;
    settings.free_rotation = !args.no_rotation;
    settings.max_steps = usize::from(args.max_steps);
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
            println!(
                "{}",
                serde_json::to_string_pretty(&project::inspect(&settings.root)?)?
            );
            Ok(())
        }
        Some(Command::Skills) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&skills::list(&settings.sessions_dir)?)?
            );
            Ok(())
        }
        Some(Command::Tui) => tui::run(settings, args.read_only, args.resume.as_deref()),
        None if io::stdin().is_terminal() && io::stdout().is_terminal() => {
            tui::run(settings, args.read_only, args.resume.as_deref())
        }
        cmd => {
            let approval = if args.read_only {
                ApprovalMode::ReadOnly
            } else {
                ApprovalMode::Ask
            };
            let mut agent = Agent::new(settings, approval, args.resume.as_deref())?;
            print_banner();
            println!(
                "Model: {}. Conversation is sent to this endpoint when you enter a task.",
                agent.model_status()
            );
            match cmd {
                Some(Command::Run { task }) => agent.turn(&task),
                Some(Command::Inspect { url }) => agent.turn(&url),
                Some(Command::Url(words)) => agent.turn(&words[0]),
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
                                println!("/help /browser [status|start|pair|attach PORT|launch|stop] /rotation /models /model [PROVIDER MODEL] /exit /session /diff /status /map /skills /skill <name> /plan /report /mcp /connect <name> /doctor — or enter a task");
                                continue;
                            }
                            "/model" | "/models" => {
                                println!(
                                    "{}\n{}\n{}",
                                    agent.model_status(),
                                    models::menu(),
                                    models::kilo_listing(&home, false)?
                                );
                                continue;
                            }
                            "/models refresh" => {
                                println!("{}", models::kilo_listing(&home, true)?);
                                continue;
                            }
                            "/models openrouter" => {
                                println!("{}", models::refresh()?);
                                continue;
                            }
                            line if line.starts_with("/model ") => {
                                match agent.select_model(line.trim_start_matches("/model ")) {
                                    Ok(text) => println!("{text}"),
                                    Err(e) => eprintln!("Model: {e:#}"),
                                }
                                continue;
                            }
                            "/rotation" => {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&agent.rotation_status())?
                                );
                                continue;
                            }
                            "/browser" => {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(
                                        &agent.browser_command("status")?
                                    )?
                                );
                                continue;
                            }
                            line if line.starts_with("/browser ") => {
                                match agent.browser_command(line.trim_start_matches("/browser ")) {
                                    Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
                                    Err(e) => eprintln!("Browser: {e:#}"),
                                }
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
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&agent.project_map()?)?
                                );
                                continue;
                            }
                            "/status" => {
                                println!("{}", serde_json::to_string_pretty(&agent.git_status()?)?);
                                continue;
                            }
                            "/skills" => {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&agent.list_skills()?)?
                                );
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
                            "/plan" => {
                                println!("{}", agent.plan_text());
                                continue;
                            }
                            "/report" => {
                                println!("{}", agent.report_text());
                                continue;
                            }
                            "/mcp" => {
                                println!("{}", agent.mcp_list());
                                continue;
                            }
                            name if name.starts_with("/connect ") => {
                                match agent.connect_mcp(name.trim_start_matches("/connect ").trim())
                                {
                                    Ok(v) => println!("{v}"),
                                    Err(e) => eprintln!("MCP: {e:#}"),
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

#[cfg(test)]
mod cli_tests {
    use super::*;
    #[test]
    fn accepts_inspect_and_bare_url_commands() {
        assert!(matches!(
            Cli::try_parse_from(["blue-dragon", "inspect", "https://example.com"])
                .unwrap()
                .command,
            Some(Command::Inspect { .. })
        ));
        assert!(
            matches!(Cli::try_parse_from(["blue-dragon", "https://example.com"]).unwrap().command, Some(Command::Url(v)) if v == ["https://example.com"])
        );
    }
    #[test]
    fn model_selection_positionals_do_not_shadow_global_options() {
        let args = Cli::try_parse_from(["blue-dragon", "model", "free"]).unwrap();
        assert!(
            matches!(args.command, Some(Command::Model { target_provider, target_model: None }) if target_provider == "free")
        );
        let args = Cli::try_parse_from(["blue-dragon", "--provider", "free", "tui"]).unwrap();
        assert_eq!(args.provider.as_deref(), Some("free"));
        assert!(Cli::try_parse_from(["blue-dragon", "models", "--refresh"]).is_ok());
    }
}
