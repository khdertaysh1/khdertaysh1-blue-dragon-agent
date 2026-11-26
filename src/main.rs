mod agent;
mod agent_tool_reply;
mod anthropic;
mod artifacts;
mod audit;
mod audit_engines;
mod browser;
mod config;
mod continuity;
mod delegates;
mod edit_proposals;
mod exact_file;
mod file_receipts;
mod file_review;
mod intelligence;
mod jobs;
mod lab_report;
mod lab_runner;
mod local_files;
mod mcp;
mod mcp_protocol;
mod mission;
mod models;
mod network_history;
mod operator;
mod plan;
mod project;
mod provider;
mod provider_budget;
mod provider_contract;
mod provider_diagnostics;
mod provider_traffic;
mod reasoning;
mod rotation;
mod saved_file;
mod scope;
mod session;
mod skills;
mod source_discovery;
mod source_report;
mod source_trace;
mod streaming;
mod swarm_tree;
mod terminal_input;
mod test_coverage;
#[cfg(test)]
mod test_support;
mod tool_arguments;
mod tool_selection;
mod tools;
mod tui;
mod tui_text;
mod vision;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use agent::{Agent, ApprovalMode};
use config::Settings;

#[derive(Debug, Parser)]
#[command(name = "blue-dragon", about = "Blue Dragon | AI coding agent", version = env!("BLUE_DRAGON_PUBLIC_VERSION"))]
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
    /// Start an empty terminal session without showing recent conversations.
    #[arg(long, global = true, conflicts_with = "resume")]
    new_session: bool,
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
        help = "Disable automatic provider and model switching"
    )]
    no_rotation: bool,
    /// Compatibility flag: verified same-provider free recovery is automatic unless --no-rotation.
    #[arg(long)]
    free_model_fallback: bool,
    /// Maximum steps per turn (24 by default, 48 for read-only work).
    #[arg(long, global=true, env="BLUE_DRAGON_MAX_STEPS", value_parser=clap::value_parser!(u16).range(1..=100))]
    max_steps: Option<u16>,
    #[arg(long, global=true, default_value_t=32768, value_parser=clap::value_parser!(u32).range(8192..=1048576), help="Context window budget including tools and reply reserve; match the selected model")]
    context_tokens: u32,
    #[arg(
        long,
        global = true,
        help = "Disable cross-session project-note retrieval and storage"
    )]
    no_project_memory: bool,
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
    Run {
        task: String,
        /// Write a single JSON result to stdout.
        #[arg(long, conflicts_with = "ndjson")]
        json: bool,
        /// Stream one JSON event per line, followed by a result.
        #[arg(long)]
        ndjson: bool,
    },
    /// Check local setup without uploading data
    Doctor,
    /// List saved local sessions, or find a conversation by words and optionally reopen it.
    Sessions {
        query: Option<String>,
        /// Rank matches using the incremental local session index
        #[arg(long, requires = "query")]
        indexed: bool,
        #[arg(long, requires = "query")]
        open: bool,
    },
    /// Inspect file types, manifests and language distribution without sending code to AI
    Map,
    /// List user-level local Markdown skills
    Skills,
}

fn run_machine(
    settings: Settings,
    approval: ApprovalMode,
    resume: Option<&str>,
    task: &str,
    stream: bool,
) -> Result<()> {
    let _input_guard = terminal_input::Guard::with_output(true)?;
    let mut agent = Agent::new(settings, approval, resume)?;
    let (tx, rx) = std::sync::mpsc::sync_channel(64);
    agent.set_output(tx);
    let reader = std::thread::spawn(move || -> Result<Option<String>> {
        let mut answer = None;
        for event in rx {
            if stream {
                println!("{}", serde_json::to_string(&event)?);
                io::stdout().flush()?;
            }
            if let agent::AgentEvent::Answer(text) = event {
                answer = Some(text);
            }
        }
        Ok(answer)
    });
    let mut result = agent.run_once(task);
    let report = agent.last_turn_report();
    let status = report["task_status"].clone();
    if result.is_ok() && report["finished"] == false {
        result = Err(anyhow::anyhow!("Task incomplete ({status})"));
    }
    let id = agent.session_id().to_owned();
    drop(agent);
    let answer = reader
        .join()
        .map_err(|_| anyhow::anyhow!("Event output interrupted"))??;
    println!(
        "{}",
        serde_json::json!({"event":"result","session":id,"task_status":status,"ok":result.is_ok(),"answer":answer,"error":result.as_ref().err().map(ToString::to_string)})
    );
    result
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
        Some(models::Selection::new("kilo", args.model.as_deref())?)
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
        .unwrap_or_else(|| models::KILO_MODEL.into());
    let mut settings = Settings::new(root, base, model)?;
    settings.mcp_config = args.mcp_config;
    settings.stream = !args.no_stream;
    settings.free_rotation = !args.no_rotation;
    settings.free_model_fallback = !args.no_rotation || args.free_model_fallback;
    settings.max_steps =
        usize::from(
            args.max_steps
                .unwrap_or(if args.read_only { 48 } else { 24 }),
        );
    settings.context_tokens = args.context_tokens as usize;
    settings.project_memory = !args.no_project_memory;
    match args.command {
        Some(Command::Doctor) => {
            println!(
                "Blue Dragon v{} | Diagnostic",
                env!("BLUE_DRAGON_PUBLIC_VERSION")
            );
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
        Some(Command::Sessions {
            query: Some(query),
            indexed,
            open,
        }) => {
            anyhow::ensure!(!query.trim().is_empty(), "A search term is required");
            let view = if indexed {
                session::indexed::search(&settings.sessions_dir, &query)?
            } else {
                session::search_sessions(&settings.sessions_dir, &query)?
            };
            if !open {
                println!("{}", serde_json::to_string_pretty(&view)?);
                return Ok(());
            }
            anyhow::ensure!(
                io::stdin().is_terminal() && io::stdout().is_terminal(),
                "Opening a session requires an interactive terminal"
            );
            let rows = view["sessions"]
                .as_array()
                .context("Session results unavailable")?;
            if rows.is_empty() {
                println!("No conversation matches");
                return Ok(());
            }
            for (index, row) in rows.iter().enumerate() {
                println!(
                    "{}. {} | {} | {}",
                    index + 1,
                    row["updated_at"],
                    row["workspace"],
                    row["match"]["excerpt"]
                );
            }
            print!("Choose a conversation number (Enter to cancel): ");
            io::stdout().flush()?;
            let mut choice = String::new();
            io::stdin().read_line(&mut choice)?;
            if choice.trim().is_empty() {
                return Ok(());
            }
            let row = choice
                .trim()
                .parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
                .and_then(|i| rows.get(i))
                .context("Invalid conversation number")?;
            let path = PathBuf::from(row["workspace"].as_str().context("Missing workspace")?);
            anyhow::ensure!(path.is_dir(), "Saved workspace is unavailable");
            settings.root = path.canonicalize()?;
            tui::run(
                settings,
                args.read_only,
                Some(row["id"].as_str().context("Missing ID")?),
                false,
            )
        }
        Some(Command::Sessions { query: None, .. }) => {
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
        Some(Command::Tui) => tui::run(
            settings,
            args.read_only,
            args.resume.as_deref(),
            args.new_session,
        ),
        None if io::stdin().is_terminal() && io::stdout().is_terminal() => tui::run(
            settings,
            args.read_only,
            args.resume.as_deref(),
            args.new_session,
        ),
        cmd => {
            let approval = if args.read_only {
                ApprovalMode::ReadOnly
            } else {
                ApprovalMode::Ask
            };
            if let Some(Command::Run { task, json, ndjson }) = &cmd {
                if *json || *ndjson {
                    return run_machine(settings, approval, args.resume.as_deref(), task, *ndjson);
                }
            }
            let _input_guard = terminal_input::Guard::start()?;
            let mut agent = Agent::new(settings, approval, args.resume.as_deref())?;
            print_banner();
            println!(
                "Model: {}. Conversation is sent to this endpoint when you enter a task.",
                agent.model_status()
            );
            match cmd {
                Some(Command::Run { task, .. }) => agent.run_once(&task),
                Some(Command::Inspect { url }) => agent.run_once(&url),
                Some(Command::Url(words)) => agent.run_once(&words[0]),
                None | Some(Command::Chat) => {
                    agent.set_synchronous_chat_input();
                    if args.resume.is_some() {
                        println!("{}", agent.resume_summary());
                    }
                    println!("Type /help for commands, /exit to quit.");
                    loop {
                        print!("\nblue-dragon> ");
                        io::stdout().flush()?;
                        let Some(text) = terminal_input::task()? else {
                            break;
                        };
                        let text = text.trim();
                        if text.is_empty() {
                            continue;
                        }
                        match text {
                            "/exit" | "/quit" => break,
                            "/help" => {
                                println!("/help /audit-report /coverage /scope [allow URL read|interact|mutate ACCOUNT|clear] /browser [status|start|pair|attach PORT|launch|stop] /rotation /compact /context /findings /jobs [ID|cancel ID] /memory [query|forget ID] /checks /mission /models /model [PROVIDER MODEL] /exit /session /checkpoint /diff /status /map /skills /skill <name> /plan /report /saved-files /resume-file ID [task] /review-file PATH /review-response REQUEST PATH /resume-review /inventory EVIDENCE_ID [OFFSET] /swarm PATH | TASK /swarm auto PATH | TASK /swarm fix PATH | JSON_ARGV | TASK /swarm status /httpflow EVIDENCE_ID [OFFSET] /desktop open PATH /websearch WORDS /clipboard copy TEXT|paste /sessions [WORDS] /sessions search WORDS /conversation-search WORDS /network EVIDENCE_ID [REQUEST_ID|*|--rows] [OFFSET] /tool-result EVIDENCE_ID /mcp [prompts SERVER|prompt SERVER NAME] /connect <name> /doctor — or enter a task");
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
                            line if line.starts_with("/model ") => {
                                match agent.select_model(line.trim_start_matches("/model ")) {
                                    Ok(text) => println!("{text}"),
                                    Err(e) => eprintln!("Model: {e:#}"),
                                }
                                continue;
                            }
                            line if line == "/jobs" || line.starts_with("/jobs ") => {
                                match agent
                                    .jobs_command(line.strip_prefix("/jobs").unwrap_or("").trim())
                                {
                                    Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
                                    Err(e) => eprintln!("Jobs: {e:#}"),
                                }
                                continue;
                            }
                            line if line == "/scope" || line.starts_with("/scope ") => {
                                match agent
                                    .scope_command(line.strip_prefix("/scope").unwrap_or("").trim())
                                {
                                    Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
                                    Err(e) => eprintln!("Scope: {e:#}"),
                                }
                                continue;
                            }
                            "/audit-report" => {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&agent.reproduction_report())?
                                );
                                continue;
                            }
                            "/coverage" => {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&agent.audit_report())?
                                );
                                continue;
                            }
                            "/mission" => {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&agent.mission_report())?
                                );
                                continue;
                            }
                            "/checks" => {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&agent.check_status())?
                                );
                                continue;
                            }
                            "/memory" => {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&agent.memory_command("")?)?
                                );
                                continue;
                            }
                            line if line.starts_with("/memory ") => {
                                match agent.memory_command(line.trim_start_matches("/memory ")) {
                                    Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
                                    Err(e) => eprintln!("Memory: {e:#}"),
                                };
                                continue;
                            }
                            "/compact" => {
                                println!("{}", agent.compact_context()?);
                                continue;
                            }
                            "/context" => {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&agent.context_status())?
                                );
                                continue;
                            }
                            "/findings" => {
                                println!("{}", agent.findings_text()?);
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
                            cmd if matches!(
                                cmd.split_whitespace().next(),
                                Some("/sessions" | "/conversation-search")
                            ) =>
                            {
                                match agent.history_search(cmd) {
                                    Ok(value) => {
                                        println!("{}", serde_json::to_string_pretty(&value)?)
                                    }
                                    Err(error) => eprintln!("{error}"),
                                }
                                continue;
                            }
                            "/checkpoint" | "وين وقفنا" | "وين وقفنا؟" => {
                                println!("{}", agent.resume_summary());
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
                            name if name == "/network" || name.starts_with("/network ") => {
                                match agent.saved_network(name.trim_start_matches("/network")) {
                                    Ok(view) => {
                                        println!("{}", serde_json::to_string_pretty(&view)?)
                                    }
                                    Err(e) => eprintln!("{e}"),
                                }
                                continue;
                            }
                            name if name.starts_with("/tool-result ") => {
                                match agent.inspect_saved_tool(
                                    name.trim_start_matches("/tool-result ").trim(),
                                ) {
                                    Ok(value) => {
                                        println!("{}", serde_json::to_string_pretty(&value)?)
                                    }
                                    Err(_) => eprintln!(
                                        "Saved tool evidence unavailable or integrity check failed"
                                    ),
                                }
                                continue;
                            }
                            cmd if operator::matches(cmd) => {
                                match agent.operator_command(cmd) {
                                    Ok(result) => {
                                        println!("{}", serde_json::to_string_pretty(&result)?)
                                    }
                                    Err(error) => eprintln!("{error:#}"),
                                }
                                continue;
                            }
                            cmd if cmd.split_whitespace().next() == Some("/inventory") => {
                                match agent.saved_inventory(cmd) {
                                    Ok(value) => {
                                        println!("{}", serde_json::to_string_pretty(&value)?)
                                    }
                                    Err(error) => eprintln!("{error:#}"),
                                }
                                continue;
                            }
                            "/saved-files" => {
                                println!("{}", serde_json::to_string_pretty(&agent.saved_files())?);
                                continue;
                            }
                            line if matches!(
                                line.split(' ').next(),
                                Some("/review-file" | "/review-response" | "/resume-review")
                            ) =>
                            {
                                if let Err(error) = agent.review_command(line) {
                                    eprintln!("Review failed: {error:#}");
                                }
                                continue;
                            }
                            line if line == "/resume-file" || line.starts_with("/resume-file ") => {
                                if let Err(error) =
                                    agent.resume_file(line["/resume-file".len()..].trim())
                                {
                                    eprintln!("Continuation failed: {error:#}");
                                }
                                continue;
                            }
                            "/report" => {
                                println!("{}", agent.report_text());
                                continue;
                            }
                            cmd if cmd.starts_with("/mcp ") => {
                                match agent.mcp_prompt(&cmd[5..]) {
                                    Ok(v) => println!("{v}"),
                                    Err(e) => eprintln!("MCP: {e:#}"),
                                }
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
