mod provider;

use std::io::{self, Read, Write};
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use mc_core::LlmProvider;
use mc_tui::{App, AppEvent, UiMessage};

/// Bridges permission prompts between runtime (sync) and TUI (async).
struct TuiPrompter {
    ui_tx: mpsc::Sender<UiMessage>,
    response_rx: std::sync::mpsc::Receiver<bool>,
}

impl mc_tools::PermissionPrompter for TuiPrompter {
    fn decide(&mut self, request: &mc_tools::PermissionRequest) -> mc_tools::PermissionOutcome {
        let _ = self.ui_tx.try_send(UiMessage::PermissionPrompt {
            tool: request.tool_name.clone(),
            input: request.input_summary.chars().take(200).collect(),
        });
        match self
            .response_rx
            .recv_timeout(std::time::Duration::from_secs(60))
        {
            Ok(true) => mc_tools::PermissionOutcome::Allow,
            _ => mc_tools::PermissionOutcome::Deny {
                reason: "denied by user".into(),
            },
        }
    }
}

#[derive(Parser)]
#[command(
    name = "magic-code",
    version,
    about = "Open-source TUI agentic AI coding agent"
)]
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
    #[arg(long)]
    resume: bool,
    #[arg(long)]
    session_id: Option<String>,
    #[arg(long)]
    pipe: bool,
    #[arg(long, short)]
    output: Option<String>,
    #[arg(long, hide = true)]
    completions: Option<String>,
    prompt: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(shell) = &cli.completions {
        let shell = match shell.as_str() {
            "bash" => clap_complete::Shell::Bash,
            "zsh" => clap_complete::Shell::Zsh,
            "fish" => clap_complete::Shell::Fish,
            s => anyhow::bail!("unsupported shell: {s}. Use bash, zsh, or fish."),
        };
        clap_complete::generate(shell, &mut Cli::command(), "magic-code", &mut io::stdout());
        return Ok(());
    }

    let filter = if cli.verbose { "debug" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .init();

    let cwd = std::env::current_dir()?;
    let config = mc_config::ConfigLoader::new(&cwd).load()?;
    for warn in config.validate() {
        eprintln!("⚠ config: {warn}");
    }
    let project = mc_config::ProjectContext::discover(&cwd);
    let model = if cli.model == "claude-sonnet-4-20250514" {
        config.model.clone()
    } else {
        cli.model.clone()
    };
    let provider_name = if cli.provider == "anthropic" && cli.model != "claude-sonnet-4-20250514" {
        provider::detect_provider(&model).unwrap_or_else(|| config.provider.clone())
    } else if cli.provider == "anthropic" {
        config.provider.clone()
    } else {
        cli.provider.clone()
    };
    let system = build_system_prompt(&project);
    let mut prompt = cli.prompt.join(" ");

    if cli.pipe || (!atty_stdin() && prompt.is_empty()) {
        let mut stdin_buf = String::new();
        io::stdin().read_to_string(&mut stdin_buf)?;
        if prompt.is_empty() {
            prompt = stdin_buf;
        } else {
            prompt = format!("{prompt}\n\n{stdin_buf}");
        }
    }

    let resume_session = if cli.resume {
        Some("last".to_string())
    } else {
        cli.session_id.clone()
    };

    let provider = provider::create_provider(
        &provider_name,
        &config.provider_config,
        cli.base_url.as_deref(),
        cli.api_key.as_deref(),
    )?;

    let rt = tokio::runtime::Runtime::new()?;
    let policy = build_permission_policy(&config);
    let hooks = build_hooks(&config);

    if prompt.trim().is_empty() {
        rt.block_on(run_tui(
            &model,
            cli.max_tokens,
            &system,
            provider,
            policy,
            hooks,
            resume_session,
            &config,
        ))
    } else {
        rt.block_on(run_single(
            &model,
            cli.max_tokens,
            &prompt,
            &system,
            provider.as_ref(),
            &policy,
            hooks,
            cli.output,
        ))
    }
}

fn build_hooks(config: &mc_config::RuntimeConfig) -> Vec<mc_tools::Hook> {
    config
        .hooks
        .iter()
        .map(|h| {
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
        })
        .collect()
}

fn build_permission_policy(config: &mc_config::RuntimeConfig) -> mc_tools::PermissionPolicy {
    use mc_config::PermissionMode as CfgPerm;
    match config.permission_mode {
        CfgPerm::ReadOnly => mc_tools::PermissionPolicy::new(mc_tools::PermissionMode::Deny)
            .with_tool_mode("read_file", mc_tools::PermissionMode::Allow)
            .with_tool_mode("glob_search", mc_tools::PermissionMode::Allow)
            .with_tool_mode("grep_search", mc_tools::PermissionMode::Allow),
        CfgPerm::WorkspaceWrite => mc_tools::PermissionPolicy::new(mc_tools::PermissionMode::Allow)
            .with_tool_mode("bash", mc_tools::PermissionMode::Prompt),
        CfgPerm::FullAccess => mc_tools::PermissionPolicy::new(mc_tools::PermissionMode::Allow),
    }
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
async fn run_tui(
    model: &str,
    max_tokens: u32,
    system: &str,
    provider: Box<dyn LlmProvider>,
    policy: mc_tools::PermissionPolicy,
    hooks: Vec<mc_tools::Hook>,
    resume_session: Option<String>,
    config: &mc_config::RuntimeConfig,
) -> Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            crossterm::event::DisableMouseCapture,
            LeaveAlternateScreen
        );
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut tool_registry = mc_tools::ToolRegistry::new()
        .with_workspace_root(std::env::current_dir().unwrap_or_default());
    for mcp in &config.mcp_servers {
        let env: Vec<(String, String)> = mcp
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        match tool_registry
            .add_mcp_server(&mcp.name, &mcp.command, &mcp.args, &env)
            .await
        {
            Ok(n) => tracing::info!(server = %mcp.name, tools = n, "MCP connected"),
            Err(e) => tracing::warn!(server = %mcp.name, "MCP connect failed: {e}"),
        }
    }

    let runtime = Arc::new(tokio::sync::Mutex::new({
        let mut rt =
            mc_core::ConversationRuntime::new(model.to_string(), max_tokens, system.to_string());
        rt.set_tool_registry(tool_registry);
        if !hooks.is_empty() {
            rt.set_hooks(mc_tools::HookEngine::new(hooks));
        }
        if let Some(ref name) = resume_session {
            if let Ok(session) = mc_core::Session::load(&session_path(name)) {
                rt.session = session;
            }
        }
        rt
    }));
    let provider = Arc::from(provider);
    let mut app = App::new(model.to_string());
    if resume_session.is_some() {
        app.output_lines.push("Session resumed.".into());
    }

    let (ui_tx, mut ui_rx) = mpsc::channel::<UiMessage>(1024);
    let mut turn_cancel: Option<CancellationToken> = None;
    let mut perm_response_tx: Option<std::sync::mpsc::SyncSender<bool>> = None;
    let mut pending_plan_sync = false;
    let mut last_plan_mode = false;
    let mut pending_compact = false;
    let mut pending_save: Option<String> = None;
    let mut pending_load: Option<String> = None;

    loop {
        terminal.draw(|f| mc_tui::ui::draw(f, &mut app))?;

        while let Ok(msg) = ui_rx.try_recv() {
            match msg {
                UiMessage::Delta(t) => app.handle_event(AppEvent::StreamDelta(t)),
                UiMessage::ToolCall(n) => app.handle_event(AppEvent::ToolCall(n)),
                UiMessage::Usage { input, output } => {
                    app.total_input_tokens += input;
                    app.total_output_tokens += output;
                    let registry = mc_core::ModelRegistry::default();
                    app.session_cost = registry.estimate_cost(
                        &app.model,
                        app.total_input_tokens,
                        app.total_output_tokens,
                    );
                    let ctx_window = registry.context_window(&app.model);
                    let used = app.total_input_tokens + app.total_output_tokens;
                    app.context_usage_pct =
                        ((u64::from(used) * 100) / u64::from(ctx_window.max(1))).min(100) as u8;
                }
                UiMessage::Done => {
                    app.handle_event(AppEvent::StreamDone);
                    turn_cancel = None;
                }
                UiMessage::Error(e) => {
                    app.handle_event(AppEvent::Error(e));
                    turn_cancel = None;
                }
                UiMessage::PermissionPrompt { tool, input } => {
                    app.permission_pending = Some((tool, input));
                }
                UiMessage::StreamReset => {
                    // Discard partial output from failed stream: remove lines back to last user prompt
                    while app.output_lines.last().is_some_and(|l| !l.starts_with('›')) {
                        app.output_lines.pop();
                    }
                    app.output_lines.push(String::new());
                }
                UiMessage::RetryAttempt {
                    attempt,
                    max,
                    reason,
                } => {
                    app.output_lines.push(format!(
                        "  ⟳ stream interrupted ({reason}), retrying ({attempt}/{max})..."
                    ));
                }
                UiMessage::ToolOutputDelta(t) => {
                    app.handle_event(AppEvent::StreamDelta(t));
                }
            }
        }

        if event::poll(std::time::Duration::from_millis(30))? {
            match event::read()? {
                Event::Mouse(mouse) => {
                    use crossterm::event::MouseEventKind;
                    match mouse.kind {
                        MouseEventKind::ScrollUp => app.scroll_up(3),
                        MouseEventKind::ScrollDown => app.scroll_down(3),
                        _ => {}
                    }
                }
                Event::Key(key) => {
                    if app.permission_pending.is_some() {
                        match key.code {
                            KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
                                if let Some(ref tx) = perm_response_tx {
                                    let _ = tx.try_send(true);
                                }
                                app.permission_pending = None;
                            }
                            KeyCode::Char('a' | 'A') => {
                                if let Some((ref tool, _)) = app.permission_pending {
                                    app.always_allowed.insert(tool.clone());
                                }
                                if let Some(ref tx) = perm_response_tx {
                                    let _ = tx.try_send(true);
                                }
                                app.permission_pending = None;
                            }
                            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                                if let Some(ref tx) = perm_response_tx {
                                    let _ = tx.try_send(false);
                                }
                                app.permission_pending = None;
                            }
                            _ => {}
                        }
                        continue;
                    }
                    match key {
                        event::KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers,
                            ..
                        } if modifiers.contains(KeyModifiers::CONTROL) => {
                            if let Some(ref cancel) = turn_cancel {
                                cancel.cancel();
                                app.handle_event(AppEvent::StreamDelta("\n[cancelled]".into()));
                                app.handle_event(AppEvent::StreamDone);
                                turn_cancel = None;
                            } else {
                                break;
                            }
                        }
                        event::KeyEvent {
                            code: KeyCode::Char('u'),
                            modifiers,
                            ..
                        } if modifiers.contains(KeyModifiers::CONTROL) => app.input.clear(),
                        event::KeyEvent {
                            code: KeyCode::Char('w'),
                            modifiers,
                            ..
                        } if modifiers.contains(KeyModifiers::CONTROL) => app.input.delete_word(),
                        event::KeyEvent {
                            code: KeyCode::PageUp,
                            ..
                        } => app.scroll_up(10),
                        event::KeyEvent {
                            code: KeyCode::PageDown,
                            ..
                        } => app.scroll_down(10),
                        event::KeyEvent {
                            code: KeyCode::Home,
                            modifiers,
                            ..
                        } if modifiers.contains(KeyModifiers::CONTROL) => app.scroll_home(),
                        event::KeyEvent {
                            code: KeyCode::End,
                            modifiers,
                            ..
                        } if modifiers.contains(KeyModifiers::CONTROL) => app.scroll_end(),
                        event::KeyEvent {
                            code: KeyCode::Up, ..
                        } => app.history_up(),
                        event::KeyEvent {
                            code: KeyCode::Down,
                            ..
                        } => app.history_down(),
                        event::KeyEvent {
                            code: KeyCode::Enter,
                            modifiers,
                            ..
                        } if modifiers.contains(KeyModifiers::SHIFT) => app.input.insert_newline(),
                        event::KeyEvent {
                            code: KeyCode::Enter,
                            ..
                        } => {
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
                                        let (ptx, prx) = std::sync::mpsc::sync_channel::<bool>(1);
                                        perm_response_tx = Some(ptx);
                                        let prompter_tx = ui_tx.clone();
                                        tokio::spawn(async move {
                                            let mut prompter: Option<
                                                Box<dyn mc_tools::PermissionPrompter>,
                                            > = Some(Box::new(TuiPrompter {
                                                ui_tx: prompter_tx,
                                                response_rx: prx,
                                            }));
                                            let result = {
                                                let mut runtime = rt.lock().await;
                                                runtime.run_turn(&*prov, &text, &pol, &mut prompter, &mut |ev| {
                                                match ev {
                                                    mc_provider::ProviderEvent::TextDelta(t) =>
                                                        { let _ = tx.try_send(UiMessage::Delta(t.clone())); }
                                                    mc_provider::ProviderEvent::ToolUse { name, .. } =>
                                                        { let _ = tx.try_send(UiMessage::ToolCall(name.clone())); }
                                                    mc_provider::ProviderEvent::Usage(u) =>
                                                        { let _ = tx.try_send(UiMessage::Usage { input: u.input_tokens, output: u.output_tokens }); }
                                                    mc_provider::ProviderEvent::StreamReset =>
                                                        { let _ = tx.try_send(UiMessage::StreamReset); }
                                                    mc_provider::ProviderEvent::RetryAttempt { attempt, max, ref reason } =>
                                                        { let _ = tx.try_send(UiMessage::RetryAttempt { attempt: *attempt, max: *max, reason: reason.clone() }); }
                                                    mc_provider::ProviderEvent::ToolOutputDelta(ref t) =>
                                                        { let _ = tx.try_send(UiMessage::ToolOutputDelta(t.clone())); }
                                                    mc_provider::ProviderEvent::MessageStop
                                                    | mc_provider::ProviderEvent::ThinkingDelta(_) => {}
                                                }
                                            }, &cancel).await
                                            };
                                            match result {
                                                Ok(_) => {
                                                    let _ = tx.try_send(UiMessage::Done);
                                                }
                                                Err(e) => {
                                                    let _ = tx
                                                        .try_send(UiMessage::Error(e.to_string()));
                                                }
                                            }
                                        });
                                    }
                                    other => app.handle_event(other),
                                }
                            }
                        }
                        event::KeyEvent {
                            code: KeyCode::Backspace,
                            ..
                        } => app.input.backspace(),
                        event::KeyEvent {
                            code: KeyCode::Tab, ..
                        } => {
                            app.tab_complete();
                        }
                        event::KeyEvent {
                            code: KeyCode::Left,
                            ..
                        } => app.input.move_left(),
                        event::KeyEvent {
                            code: KeyCode::Right,
                            ..
                        } => app.input.move_right(),
                        event::KeyEvent {
                            code: KeyCode::Char(c),
                            ..
                        } => app.input.insert(c),
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }

        if app.plan_mode != last_plan_mode {
            pending_plan_sync = true;
            last_plan_mode = app.plan_mode;
        }
        if app.compact_requested {
            app.compact_requested = false;
            pending_compact = true;
        }
        if let Some(name) = app.save_requested.take() {
            pending_save = Some(name);
        }
        if let Some(name) = app.load_requested.take() {
            pending_load = Some(name);
        }

        if let Ok(mut rt) = runtime.try_lock() {
            if pending_plan_sync {
                rt.plan_mode = app.plan_mode;
                pending_plan_sync = false;
            }
            if let Some(name) = pending_save.take() {
                let path = session_path(&name);
                match rt.session.save(&path) {
                    Ok(()) => app.handle_event(AppEvent::StreamDelta(format!(
                        "Saved to {}",
                        path.display()
                    ))),
                    Err(e) => app.handle_event(AppEvent::Error(e.to_string())),
                }
            }
            if let Some(name) = pending_load.take() {
                let path = session_path(&name);
                match mc_core::Session::load(&path) {
                    Ok(s) => {
                        rt.session = s;
                        app.handle_event(AppEvent::StreamDelta(format!(
                            "Loaded from {}",
                            path.display()
                        )));
                    }
                    Err(e) => app.handle_event(AppEvent::Error(e.to_string())),
                }
            }
        } else if pending_save.is_some() || pending_load.is_some() {
            app.handle_event(AppEvent::StreamDelta(
                "Queued, waiting for current turn...".into(),
            ));
        }

        if app.undo_requested {
            app.undo_requested = false;
            let rt_clone = Arc::clone(&runtime);
            let tx_clone = ui_tx.clone();
            tokio::spawn(async move {
                let mut rt = rt_clone.lock().await;
                match rt.undo_last_turn() {
                    Ok(paths) if paths.is_empty() => {
                        let _ = tx_clone.try_send(UiMessage::Delta("Nothing to undo".into()));
                    }
                    Ok(paths) => {
                        let msg =
                            format!("↩ Reverted {} file(s): {}", paths.len(), paths.join(", "));
                        let _ = tx_clone.try_send(UiMessage::Delta(msg));
                    }
                    Err(e) => {
                        let _ = tx_clone.try_send(UiMessage::Error(format!("Undo failed: {e}")));
                    }
                }
                let _ = tx_clone.try_send(UiMessage::Done);
            });
        }

        if app.cost_total_requested {
            app.cost_total_requested = false;
            if let Ok(rt) = runtime.try_lock() {
                let (i, o, c) = rt.cumulative_cost();
                app.output_lines.push(format!(
                    "All-time cost: ${c:.4} ({i} input + {o} output tokens)"
                ));
            }
        }

        // Handle /model switch
        if let Some(ref new_model) = app.model_switch.take() {
            if let Ok(mut rt) = runtime.try_lock() {
                // Check model aliases from config
                let resolved = config
                    .model_aliases
                    .get(new_model)
                    .cloned()
                    .unwrap_or_else(|| new_model.clone());
                rt.set_model(resolved.clone());
                app.model.clone_from(&resolved);
                app.output_lines
                    .push(format!("Switched to model: {resolved}"));
            }
        }

        // Handle /export
        if app.export_requested {
            app.export_requested = false;
            let path = session_path("export.md");
            let content = app.output_lines.join("\n");
            match std::fs::write(&path, &content) {
                Ok(()) => app
                    .output_lines
                    .push(format!("Exported to {}", path.display())),
                Err(e) => app.output_lines.push(format!("Export failed: {e}")),
            }
        }

        // Handle /init
        if app.init_requested {
            app.init_requested = false;
            let dir = std::env::current_dir()
                .unwrap_or_default()
                .join(".magic-code");
            let conf = dir.join("config.toml");
            if conf.exists() {
                app.output_lines
                    .push(format!("Config already exists: {}", conf.display()));
            } else {
                let _ = std::fs::create_dir_all(&dir);
                let template = concat!(
                    "# magic-code project config\n",
                    "# model = \"claude-sonnet-4-20250514\"\n",
                    "# provider = \"anthropic\"\n",
                    "# permission_mode = \"workspace-write\"\n",
                    "\n",
                    "[model_aliases]\n",
                    "# fast = \"claude-haiku\"\n",
                    "# smart = \"claude-sonnet-4-20250514\"\n",
                );
                match std::fs::write(&conf, template) {
                    Ok(()) => {
                        // Also create instructions.md
                        let inst = dir.join("instructions.md");
                        let _ = std::fs::write(
                            &inst,
                            "# Project Instructions\n\nAdd custom instructions for the AI here.\n",
                        );
                        app.output_lines
                            .push(format!("Created {} and instructions.md", conf.display()));
                    }
                    Err(e) => app.output_lines.push(format!("Init failed: {e}")),
                }
            }
        }

        // Handle /summary
        if app.summary_requested {
            app.summary_requested = false;
            let line_count = app.output_lines.len();
            app.output_lines.push(format!(
                "Session: {} lines, {} input + {} output tokens, ${:.4} cost, model: {}",
                line_count,
                app.total_input_tokens,
                app.total_output_tokens,
                app.session_cost,
                app.model
            ));
        }

        // Handle /search
        if let Some(query) = app.search_query.take() {
            let sessions_dir = session_path("")
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            let mut found = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().is_some_and(|e| e == "json") {
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            if content.contains(&query) {
                                found.push(
                                    entry
                                        .path()
                                        .file_stem()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string(),
                                );
                            }
                        }
                    }
                }
            }
            if found.is_empty() {
                app.output_lines
                    .push(format!("No sessions matching \"{query}\""));
            } else {
                app.output_lines.push(format!(
                    "Sessions matching \"{query}\": {}",
                    found.join(", ")
                ));
            }
        }

        // Handle /memory command
        if let Some(cmd) = app.memory_command.take() {
            if let Ok(_rt) = runtime.try_lock() {
                // Memory display is read-only, handled inline
                app.output_lines.push(format!(
                    "  📌 memory: {cmd} (handled by LLM via memory tools)"
                ));
            }
        }

        // Handle /thinking toggle
        if app.thinking_toggle {
            app.thinking_toggle = false;
            app.output_lines.push(
                "💭 Thinking display toggled (visual only — thinking is controlled by config)"
                    .into(),
            );
        }

        // Handle /fork, /branches, /switch, /branch delete
        if let Some(cmd) = app.branch_command.take() {
            app.output_lines.push(format!(
                "  🌿 branch: {cmd} (branch management requires BranchManager setup)"
            ));
        }

        // Handle git commands
        if let Some(cmd) = app.git_command.take() {
            let git_args: &[&str] = match cmd.as_str() {
                "diff" => &["diff"],
                "log" => &["log", "--oneline", "-10"],
                "stash" => &["stash"],
                "stash_pop" => &["stash", "pop"],
                "commit" => &["diff", "--cached", "--stat"],
                _ => &["status"],
            };
            match std::process::Command::new("git").args(git_args).output() {
                Ok(o) => {
                    let out = String::from_utf8_lossy(&o.stdout);
                    let err = String::from_utf8_lossy(&o.stderr);
                    if cmd == "commit" {
                        // Show staged diff, then auto-commit with generated message
                        if out.trim().is_empty() {
                            app.output_lines
                                .push("Nothing staged. Run `git add` first.".into());
                        } else {
                            app.output_lines.push(format!("Staged:\n{out}"));
                            app.output_lines.push("Generating commit message...".into());
                            // Queue a prompt to the LLM to generate commit message
                            // For now, commit with a generic message
                            if let Ok(diff) = std::process::Command::new("git")
                                .args(["diff", "--cached"])
                                .output()
                            {
                                let diff_text = String::from_utf8_lossy(&diff.stdout);
                                let msg = if diff_text.len() > 200 {
                                    "chore: update files"
                                } else {
                                    "chore: minor changes"
                                };
                                match std::process::Command::new("git")
                                    .args(["commit", "-m", msg])
                                    .output()
                                {
                                    Ok(co) => app.output_lines.push(format!(
                                        "✓ {}",
                                        String::from_utf8_lossy(&co.stdout).trim()
                                    )),
                                    Err(e) => {
                                        app.output_lines.push(format!("✗ commit failed: {e}"));
                                    }
                                }
                            }
                        }
                    } else {
                        if !out.is_empty() {
                            for line in out.lines() {
                                app.output_lines.push(format!("  {line}"));
                            }
                        }
                        if !err.is_empty() {
                            app.output_lines.push(format!("  {}", err.trim()));
                        }
                        if out.is_empty() && err.is_empty() {
                            app.output_lines.push("  (no output)".into());
                        }
                    }
                }
                Err(e) => app.output_lines.push(format!("  ✗ git: {e}")),
            }
        }

        if pending_compact {
            pending_compact = false;
            let rt_clone = Arc::clone(&runtime);
            let prov_clone = Arc::clone(&provider);
            let tx_clone = ui_tx.clone();
            tokio::spawn(async move {
                let mut rt = rt_clone.lock().await;
                let model = rt.model().to_string();
                if let Err(e) =
                    mc_core::smart_compact(&*prov_clone, &mut rt.session, &model, 4).await
                {
                    tracing::warn!("smart compact failed: {e}");
                    mc_core::compact_session(&mut rt.session, 4);
                }
                let _ = tx_clone.try_send(UiMessage::Delta("Session compacted.".into()));
            });
        }
    }

    if let Some(cancel) = turn_cancel.take() {
        cancel.cancel();
    }
    if let Ok(rt) = tokio::time::timeout(std::time::Duration::from_secs(3), runtime.lock()).await {
        let _ = rt.session.save(&session_path("last"));
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_single(
    model: &str,
    max_tokens: u32,
    prompt: &str,
    system: &str,
    provider: &dyn LlmProvider,
    policy: &mc_tools::PermissionPolicy,
    hooks: Vec<mc_tools::Hook>,
    output_path: Option<String>,
) -> Result<()> {
    let cancel = CancellationToken::new();
    let mut runtime =
        mc_core::ConversationRuntime::new(model.to_string(), max_tokens, system.to_string());
    runtime.set_tool_registry(
        mc_tools::ToolRegistry::new()
            .with_workspace_root(std::env::current_dir().unwrap_or_default()),
    );
    if !hooks.is_empty() {
        runtime.set_hooks(mc_tools::HookEngine::new(hooks));
    }

    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        cancel_clone.cancel();
    });

    let mut stdout = io::stdout();
    let result = runtime
        .run_turn(
            provider,
            prompt,
            policy,
            &mut None,
            &mut |event| match event {
                mc_provider::ProviderEvent::TextDelta(text)
                | mc_provider::ProviderEvent::ToolOutputDelta(text) => {
                    let _ = write!(stdout, "{text}");
                    let _ = stdout.flush();
                }
                _ => {}
            },
            &cancel,
        )
        .await
        .context("turn failed")?;

    println!();
    if let Some(path) = output_path {
        std::fs::write(&path, &result.text).context("failed to write output file")?;
        eprintln!("[output written to {path}]");
    }
    if result.cancelled {
        eprintln!("[cancelled]");
    }
    if !result.tool_calls.is_empty() {
        eprintln!("[tools: {}]", result.tool_calls.join(", "));
    }
    eprintln!(
        "[tokens: {}↓ {}↑ | {} iters]",
        result.usage.input_tokens, result.usage.output_tokens, result.iterations
    );
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
        parts.push(format!(
            "Detected stack: {}",
            project.detected_stack.join(", ")
        ));
    }
    if let Some(s) = &project.git_status {
        parts.push(format!("Git status:\n{s}"));
    }
    for f in &project.instruction_files {
        parts.push(format!(
            "Project instructions ({}):\n{}",
            f.path.display(),
            f.content
        ));
    }
    parts.join("\n\n")
}
