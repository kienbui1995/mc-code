use std::io::{self, Read, Write};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use mc_core::LlmProvider;
use mc_tui::{App, AppEvent, UiMessage};

#[derive(Parser)]
#[command(name = "magic-code", version, about = "Open-source TUI agentic AI coding agent")]
struct Cli {
    #[arg(long, default_value = "claude-sonnet-4-20250514")]
    model: String,
    #[arg(long, default_value = "8192")]
    max_tokens: u32,
    #[arg(long, default_value = "anthropic")]
    provider: String,
    #[arg(long)]
    base_url: Option<String>,
    #[arg(long)]
    api_key: Option<String>,
    #[arg(long, short)]
    verbose: bool,
    /// Resume the last session.
    #[arg(long)]
    resume: bool,
    /// Resume a specific session by name.
    #[arg(long)]
    session_id: Option<String>,
    /// Pipe mode: read stdin, no TUI.
    #[arg(long)]
    pipe: bool,
    prompt: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let filter = if cli.verbose { "debug" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)))
        .init();

    let cwd = std::env::current_dir()?;
    let config = mc_config::ConfigLoader::new(&cwd).load()?;
    let project = mc_config::ProjectContext::discover(&cwd);
    let model = if cli.model == "claude-sonnet-4-20250514" { config.model.clone() } else { cli.model.clone() };
    let provider_name = if cli.provider == "anthropic" { config.provider.clone() } else { cli.provider.clone() };
    let system = build_system_prompt(&project);
    let mut prompt = cli.prompt.join(" ");

    // Pipe mode: read from stdin
    if cli.pipe || (!atty_stdin() && prompt.is_empty()) {
        let mut stdin_buf = String::new();
        io::stdin().read_to_string(&mut stdin_buf)?;
        if prompt.is_empty() {
            prompt = stdin_buf;
        } else {
            prompt = format!("{prompt}\n\n{stdin_buf}");
        }
    }

    let provider_config = config.provider_config.clone();
    let format = provider_config.format.as_deref().unwrap_or("");

    let resume_session = if cli.resume {
        Some("last".to_string())
    } else {
        cli.session_id.clone()
    };

    if let Some(ref base_url) = cli.base_url {
        let p = mc_provider::GenericProvider::new(base_url.clone(), cli.api_key.clone());
        return dispatch(&model, cli.max_tokens, &system, &prompt, p, &config, resume_session.clone());
    }

    match provider_name.as_str() {
        "anthropic" if format != "openai-compatible" => {
            let p = mc_provider::AnthropicProvider::from_env().context("set ANTHROPIC_API_KEY")?;
            dispatch(&model, cli.max_tokens, &system, &prompt, p, &config, resume_session.clone())
        }
        "openai" => {
            let key = cli.api_key.clone()
                .or_else(|| std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.is_empty()))
                .context("set OPENAI_API_KEY")?;
            let p = mc_provider::GenericProvider::new("https://api.openai.com".into(), Some(key));
            dispatch(&model, cli.max_tokens, &system, &prompt, p, &config, resume_session.clone())
        }
        "ollama" => {
            let p = mc_provider::GenericProvider::ollama();
            dispatch(&model, cli.max_tokens, &system, &prompt, p, &config, resume_session.clone())
        }
        "gemini" => {
            let p = mc_provider::GeminiProvider::from_env().context("set GEMINI_API_KEY")?;
            dispatch(&model, cli.max_tokens, &system, &prompt, p, &config, resume_session.clone())
        }
        "litellm" => {
            let base = provider_config.base_url.clone()
                .or_else(|| std::env::var("LITELLM_BASE_URL").ok())
                .unwrap_or_else(|| "http://localhost:4000".to_string());
            let key = cli.api_key.clone().or_else(|| resolve_api_key(&provider_config));
            let p = mc_provider::GenericProvider::new(base, key);
            dispatch(&model, cli.max_tokens, &system, &prompt, p, &config, resume_session.clone())
        }
        name => {
            if let Some(base) = &provider_config.base_url {
                let key = cli.api_key.clone().or_else(|| resolve_api_key(&provider_config));
                let p = mc_provider::GenericProvider::new(base.clone(), key);
                dispatch(&model, cli.max_tokens, &system, &prompt, p, &config, resume_session.clone())
            } else {
                bail!("unknown provider '{name}'. Set base_url in config or use --base-url flag.")
            }
        }
    }
}

fn resolve_api_key(config: &mc_config::ProviderConfig) -> Option<String> {
    if config.api_key_env.is_empty() { return None; }
    std::env::var(&config.api_key_env).ok().filter(|k| !k.is_empty())
}

fn dispatch(
    model: &str,
    max_tokens: u32,
    system: &str,
    prompt: &str,
    provider: impl LlmProvider + 'static,
    config: &mc_config::RuntimeConfig,
    resume_session: Option<String>,
) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let policy = build_permission_policy(config);
    let hooks = build_hooks(config);
    if prompt.trim().is_empty() {
        rt.block_on(run_tui(model, max_tokens, system, provider, policy, hooks, resume_session))
    } else {
        rt.block_on(run_single(model, max_tokens, prompt, system, &provider, &policy, hooks))
    }
}

fn build_hooks(config: &mc_config::RuntimeConfig) -> Vec<mc_tools::Hook> {
    config.hooks.iter().map(|h| {
        let event = match h.event.as_str() {
            "pre_tool_call" => mc_tools::HookEvent::PreToolCall,
            "pre_compact" => mc_tools::HookEvent::PreCompact,
            "post_compact" => mc_tools::HookEvent::PostCompact,
            _ => mc_tools::HookEvent::PostToolCall,
        };
        mc_tools::Hook {
            event,
            command: h.command.clone(),
            match_tools: h.match_tools.clone(),
        }
    }).collect()
}

fn build_permission_policy(config: &mc_config::RuntimeConfig) -> mc_tools::PermissionPolicy {
    use mc_config::PermissionMode as CfgPerm;
    match config.permission_mode {
        CfgPerm::ReadOnly => {
            mc_tools::PermissionPolicy::new(mc_tools::PermissionMode::Deny)
                .with_tool_mode("read_file", mc_tools::PermissionMode::Allow)
                .with_tool_mode("glob_search", mc_tools::PermissionMode::Allow)
                .with_tool_mode("grep_search", mc_tools::PermissionMode::Allow)
        }
        CfgPerm::WorkspaceWrite => {
            mc_tools::PermissionPolicy::new(mc_tools::PermissionMode::Allow)
                .with_tool_mode("bash", mc_tools::PermissionMode::Prompt)
        }
        CfgPerm::FullAccess => {
            mc_tools::PermissionPolicy::new(mc_tools::PermissionMode::Allow)
        }
    }
}

#[allow(clippy::too_many_lines, clippy::unused_async)]
async fn run_tui(
    model: &str,
    max_tokens: u32,
    system: &str,
    provider: impl LlmProvider + 'static,
    policy: mc_tools::PermissionPolicy,
    hooks: Vec<mc_tools::Hook>,
    resume_session: Option<String>,
) -> Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let runtime = Arc::new(tokio::sync::Mutex::new({
        let mut rt = mc_core::ConversationRuntime::new(model.to_string(), max_tokens, system.to_string());
        if !hooks.is_empty() {
            rt.set_hooks(mc_tools::HookEngine::new(hooks));
        }
        if let Some(ref name) = resume_session {
            let path = session_path(name);
            if let Ok(session) = mc_core::Session::load(&path) {
                rt.session = session;
            }
        }
        rt
    }));
    let provider = Arc::new(provider);
    let mut app = App::new(model.to_string());
    if resume_session.is_some() {
        app.output_lines.push("Session resumed.".into());
    }

    let (ui_tx, mut ui_rx) = mpsc::unbounded_channel::<UiMessage>();
    // Track current turn's cancel token
    let mut turn_cancel: Option<CancellationToken> = None;

    loop {
        terminal.draw(|f| mc_tui::ui::draw(f, &mut app))?;

        // Drain all pending UI messages from background task
        while let Ok(msg) = ui_rx.try_recv() {
            match msg {
                UiMessage::Delta(t) => app.handle_event(AppEvent::StreamDelta(t)),
                UiMessage::ToolCall(n) => app.handle_event(AppEvent::ToolCall(n)),
                UiMessage::Usage { input, output } => {
                    app.total_input_tokens += input;
                    app.total_output_tokens += output;
                }
                UiMessage::Done => {
                    app.handle_event(AppEvent::StreamDone);
                    turn_cancel = None;
                }
                UiMessage::Error(e) => {
                    app.handle_event(AppEvent::Error(e));
                    turn_cancel = None;
                }
            }
        }

        if event::poll(std::time::Duration::from_millis(30))? {
            if let Event::Key(key) = event::read()? {
                match key {
                    // Ctrl+C: cancel current turn, or quit if idle
                    event::KeyEvent { code: KeyCode::Char('c'), modifiers, .. }
                        if modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        if let Some(ref cancel) = turn_cancel {
                            cancel.cancel();
                            app.handle_event(AppEvent::StreamDelta("\n[cancelled]".into()));
                            app.handle_event(AppEvent::StreamDone);
                            turn_cancel = None;
                        } else {
                            break;
                        }
                    }
                    // Ctrl+U: clear input line
                    event::KeyEvent { code: KeyCode::Char('u'), modifiers, .. }
                        if modifiers.contains(KeyModifiers::CONTROL) => app.input.clear(),
                    // Ctrl+W: delete word
                    event::KeyEvent { code: KeyCode::Char('w'), modifiers, .. }
                        if modifiers.contains(KeyModifiers::CONTROL) => app.input.delete_word(),
                    // Scroll
                    event::KeyEvent { code: KeyCode::PageUp, .. } => app.scroll_up(10),
                    event::KeyEvent { code: KeyCode::PageDown, .. } => app.scroll_down(10),
                    event::KeyEvent { code: KeyCode::Home, modifiers, .. }
                        if modifiers.contains(KeyModifiers::CONTROL) => app.scroll_home(),
                    event::KeyEvent { code: KeyCode::End, modifiers, .. }
                        if modifiers.contains(KeyModifiers::CONTROL) => app.scroll_end(),
                    // History navigation (Up/Down when not in multiline)
                    event::KeyEvent { code: KeyCode::Up, .. } => app.history_up(),
                    event::KeyEvent { code: KeyCode::Down, .. } => app.history_down(),
                    // Shift+Enter: newline in input
                    event::KeyEvent { code: KeyCode::Enter, modifiers, .. }
                        if modifiers.contains(KeyModifiers::SHIFT) => app.input.insert_newline(),
                    event::KeyEvent { code: KeyCode::Enter, .. } => {
                        if let Some(evt) = app.submit_input() {
                            match evt {
                                AppEvent::UserSubmit(text) => {
                                    app.handle_event(AppEvent::UserSubmit(text.clone()));
                                    let cancel = CancellationToken::new();
                                    turn_cancel = Some(cancel.clone());
                                    let tx = ui_tx.clone();
                                    let rt = Arc::clone(&runtime);
                                    let prov = Arc::clone(&provider);
                                    let pol = policy.clone();
                                    tokio::spawn(async move {
                                        let result = {
                                            let mut runtime = rt.lock().await;
                                            runtime.run_turn(&*prov, &text, &pol, &mut |ev| {
                                                match ev {
                                                    mc_provider::ProviderEvent::TextDelta(t) =>
                                                        { let _ = tx.send(UiMessage::Delta(t.clone())); }
                                                    mc_provider::ProviderEvent::ToolUse { name, .. } =>
                                                        { let _ = tx.send(UiMessage::ToolCall(name.clone())); }
                                                    mc_provider::ProviderEvent::Usage(u) =>
                                                        { let _ = tx.send(UiMessage::Usage { input: u.input_tokens, output: u.output_tokens }); }
                                                    mc_provider::ProviderEvent::MessageStop => {}
                                                }
                                            }, &cancel).await
                                        };
                                        match result {
                                            Ok(_) => { let _ = tx.send(UiMessage::Done); }
                                            Err(e) => { let _ = tx.send(UiMessage::Error(e.to_string())); }
                                        }
                                    });
                                }
                                other => app.handle_event(other),
                            }
                        }
                    }
                    event::KeyEvent { code: KeyCode::Backspace, .. } => app.input.backspace(),
                    event::KeyEvent { code: KeyCode::Left, .. } => app.input.move_left(),
                    event::KeyEvent { code: KeyCode::Right, .. } => app.input.move_right(),
                    event::KeyEvent { code: KeyCode::Char(c), .. } => app.input.insert(c),
                    _ => {}
                }
            }
        }

        if app.should_quit { break; }

        // Sync plan mode
        if let Ok(mut rt) = runtime.try_lock() {
            rt.plan_mode = app.plan_mode;
        }

        // Handle compact
        if app.compact_requested {
            app.compact_requested = false;
            if let Ok(mut rt) = runtime.try_lock() {
                mc_core::compact_session(&mut rt.session, 4);
            }
            app.handle_event(AppEvent::StreamDelta("Session compacted.".into()));
        }

        // Handle save
        if let Some(name) = app.save_requested.take() {
            let path = session_path(&name);
            let result = runtime.try_lock()
                .map_err(|e| anyhow::anyhow!("{e}"))
                .and_then(|rt| rt.session.save(&path).map_err(Into::into));
            match result {
                Ok(()) => app.handle_event(AppEvent::StreamDelta(format!("Saved to {}", path.display()))),
                Err(e) => app.handle_event(AppEvent::Error(e.to_string())),
            }
        }

        // Handle load
        if let Some(name) = app.load_requested.take() {
            let path = session_path(&name);
            match mc_core::Session::load(&path) {
                Ok(s) => {
                    if let Ok(mut rt) = runtime.try_lock() { rt.session = s; }
                    app.handle_event(AppEvent::StreamDelta(format!("Loaded from {}", path.display())));
                }
                Err(e) => app.handle_event(AppEvent::Error(e.to_string())),
            }
        }
    }

    // Auto-save session as "last" for --resume
    if let Ok(rt) = runtime.try_lock() {
        let _ = rt.session.save(&session_path("last"));
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

async fn run_single(model: &str, max_tokens: u32, prompt: &str, system: &str, provider: &dyn LlmProvider, policy: &mc_tools::PermissionPolicy, hooks: Vec<mc_tools::Hook>) -> Result<()> {
    let cancel = CancellationToken::new();
    let mut runtime = mc_core::ConversationRuntime::new(model.to_string(), max_tokens, system.to_string());
    if !hooks.is_empty() {
        runtime.set_hooks(mc_tools::HookEngine::new(hooks));
    }

    // Cancel on Ctrl+C in single-shot mode
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        cancel_clone.cancel();
    });

    let mut stdout = io::stdout();
    let result = runtime.run_turn(provider, prompt, policy, &mut |event| {
        if let mc_provider::ProviderEvent::TextDelta(text) = event {
            let _ = write!(stdout, "{text}");
            let _ = stdout.flush();
        }
    }, &cancel).await.context("turn failed")?;

    println!();
    if result.cancelled {
        eprintln!("[cancelled]");
    }
    if !result.tool_calls.is_empty() {
        eprintln!("[tools: {}]", result.tool_calls.join(", "));
    }
    eprintln!("[tokens: {}↓ {}↑ | {} iters]", result.usage.input_tokens, result.usage.output_tokens, result.iterations);
    Ok(())
}

fn session_path(name: &str) -> std::path::PathBuf {
    let base = std::env::var_os("HOME").map_or_else(
        || std::path::PathBuf::from("sessions"),
        |h| std::path::PathBuf::from(h).join(".local/share/magic-code/sessions"),
    );
    base.join(format!("{name}.json"))
}

fn atty_stdin() -> bool {
    // Simple heuristic: if stdin has a known terminal size, it's a TTY
    crossterm::terminal::size().is_ok()
}


fn build_system_prompt(project: &mc_config::ProjectContext) -> String {
    let mut parts = vec![
        "You are magic-code, an expert AI coding assistant running in the user's terminal.\n\n\
         ## Tools\n\
         - `bash`: Execute shell commands. Prefer short, targeted commands.\n\
         - `read_file`: Read files with optional offset/limit for large files.\n\
         - `write_file`: Create or overwrite files. Always write complete file content.\n\
         - `edit_file`: Replace specific text in files. Use for surgical edits.\n\
         - `glob_search`: Find files by pattern.\n\
         - `grep_search`: Search file contents with regex.\n\
         - `subagent`: Delegate independent subtasks to an isolated agent.\n\n\
         ## Guidelines\n\
         - Be concise. Show code, not explanations of code.\n\
         - Read files before editing to understand current state.\n\
         - Use edit_file for small changes, write_file for new files or rewrites.\n\
         - Run tests after making changes to verify correctness.\n\
         - If a task has independent parts, use subagent to parallelize.\n\
         - When you encounter errors, read the relevant code and fix systematically."
            .to_string(),
        format!("Working directory: {}", project.cwd.display()),
    ];
    if !project.detected_stack.is_empty() {
        parts.push(format!("Detected stack: {}", project.detected_stack.join(", ")));
    }
    if let Some(s) = &project.git_status {
        parts.push(format!("Git status:\n{s}"));
    }
    for f in &project.instruction_files {
        parts.push(format!("Project instructions ({}):\n{}", f.path.display(), f.content));
    }
    parts.join("\n\n")
}
