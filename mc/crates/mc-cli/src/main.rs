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
#[allow(clippy::struct_excessive_bools)]
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
    /// Output results as JSON (for automation/scripting).
    #[arg(long)]
    json: bool,
    /// Stop after spending this many USD.
    #[arg(long)]
    max_budget_usd: Option<f64>,
    /// Stop after this many model turns.
    #[arg(long)]
    max_turns: Option<u32>,
    /// Stop after generating this many output tokens total.
    #[arg(long)]
    max_tokens_total: Option<u64>,
    #[arg(long, hide = true)]
    completions: Option<String>,
    /// Grant access to additional directories outside workspace.
    #[arg(long, value_name = "DIR")]
    add_dir: Vec<String>,
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
            cli.max_budget_usd,
            cli.max_turns,
            cli.max_tokens_total,
            &cli.add_dir,
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
            cli.json,
            &cli.add_dir,
            &config.mcp_servers,
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
    cli_max_budget: Option<f64>,
    cli_max_turns: Option<u32>,
    cli_max_tokens_total: Option<u64>,
    extra_dirs: &[String],
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
    for dir in extra_dirs {
        let path = std::path::PathBuf::from(dir);
        if path.is_dir() {
            tool_registry = tool_registry.with_extra_root(path);
        }
    }
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
        // Load hierarchical instructions (CLAUDE.md, AGENTS.md from root to cwd)
        let cwd = std::env::current_dir().unwrap_or_default();
        let instructions = mc_config::load_hierarchical_instructions(&cwd);
        if !instructions.is_empty() {
            let combined: String = instructions
                .iter()
                .map(|(path, content)| {
                    let resolved = mc_config::resolve_includes(
                        path.parent().unwrap_or(std::path::Path::new(".")),
                        content,
                    );
                    format!("\n\n# Instructions from {}\n{}", path.display(), resolved)
                })
                .collect();
            rt.set_hierarchical_instructions(combined);
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
    let mut turn_count: u32 = 0;

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
                    let turn_cost = registry.estimate_cost(&app.model, input, output);
                    let turn_num = app.turn_costs.len() as u32 + 1;
                    app.turn_costs.push((turn_num, input, output, turn_cost, app.model.clone()));
                    let ctx_window = registry.context_window(&app.model);
                    let used = app.total_input_tokens + app.total_output_tokens;
                    app.context_usage_pct =
                        ((u64::from(used) * 100) / u64::from(ctx_window.max(1))).min(100) as u8;
                }
                UiMessage::Done { ttft_ms, total_ms } => {
                    app.handle_event(AppEvent::StreamDone);
                    app.ttft_ms = ttft_ms;
                    app.turn_time_ms = total_ms;
                    turn_cancel = None;
                    // Auto-save every 5 turns
                    turn_count += 1;
                    // Check budget/turn limits
                    if let Some(max_usd) = cli_max_budget {
                        if app.session_cost >= max_usd {
                            app.handle_event(AppEvent::Error(format!(
                                "Budget limit reached: ${:.2}",
                                max_usd
                            )));
                            break;
                        }
                    }
                    if let Some(max_t) = cli_max_turns {
                        if turn_count >= max_t {
                            app.handle_event(AppEvent::Error(format!(
                                "Turn limit reached: {}",
                                max_t
                            )));
                            break;
                        }
                    }
                    if let Some(max_tok) = cli_max_tokens_total {
                        if (app.total_input_tokens as u64 + app.total_output_tokens as u64)
                            >= max_tok
                        {
                            app.handle_event(AppEvent::Error(format!(
                                "Token limit reached: {}",
                                max_tok
                            )));
                            break;
                        }
                    }
                    if turn_count.is_multiple_of(5) {
                        if let Ok(rt) = runtime.try_lock() {
                            let _ = rt.session.save(&session_path("last"));
                        }
                    }
                    // Desktop notification + bell
                    print!("\x07");
                    #[cfg(target_os = "linux")]
                    {
                        let _ = std::process::Command::new("notify-send")
                            .args(["magic-code", "Turn complete"])
                            .spawn();
                    }
                    #[cfg(target_os = "macos")]
                    {
                        let _ = std::process::Command::new("osascript")
                            .args([
                                "-e",
                                "display notification \"Turn complete\" with title \"magic-code\"",
                            ])
                            .spawn();
                    }
                }
                UiMessage::Error(e) => {
                    app.handle_event(AppEvent::Error(e));
                    turn_cancel = None;
                }
                UiMessage::PermissionPrompt { tool, input } => {
                    app.permission_pending = Some((tool, input));
                    app.state = mc_tui::AgentState::WaitingPermission;
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
                                        let effort_budget = app.effort.thinking_budget();
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
                                            let turn_start = std::time::Instant::now();
                                            let mut first_token = true;
                                            let mut ttft_ms = 0u64;
                                            let result = {
                                                let mut runtime = rt.lock().await;
                                                runtime.set_thinking_budget(effort_budget);
                                                runtime.run_turn(&*prov, &text, &pol, &mut prompter, &mut |ev| {
                                                if first_token && matches!(ev, mc_provider::ProviderEvent::TextDelta(_)) {
                                                    ttft_ms = turn_start.elapsed().as_millis() as u64;
                                                    first_token = false;
                                                }
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
                                                    let total_ms =
                                                        turn_start.elapsed().as_millis() as u64;
                                                    let _ = tx.try_send(UiMessage::Done {
                                                        ttft_ms,
                                                        total_ms,
                                                    });
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
                            code: KeyCode::Char('o'),
                            modifiers,
                            ..
                        } if modifiers.contains(KeyModifiers::CONTROL) => {
                            app.transcript_mode = !app.transcript_mode;
                        }
                        event::KeyEvent {
                            code: KeyCode::Char('b'),
                            modifiers,
                            ..
                        } if modifiers.contains(KeyModifiers::CONTROL) => {
                            // Background current turn
                            app.output_lines
                                .push("  ⏎ Backgrounded current task".into());
                        }
                        event::KeyEvent {
                            code: KeyCode::Esc, ..
                        } => {
                            if let Some(ref mut mode) = app.vim_mode {
                                *mode = mc_tui::VimMode::Normal;
                            }
                        }
                        event::KeyEvent {
                            code: KeyCode::Char(c),
                            ..
                        } => {
                            // Vim normal mode handling
                            if app.vim_mode == Some(mc_tui::VimMode::Normal) {
                                match c {
                                    'i' => app.vim_mode = Some(mc_tui::VimMode::Insert),
                                    'a' => {
                                        app.input.move_right_for_append();
                                        app.vim_mode = Some(mc_tui::VimMode::Insert);
                                    }
                                    'h' => app.input.move_left(),
                                    'l' => app.input.move_right(),
                                    'w' => app.input.word_forward(),
                                    'b' => app.input.word_backward(),
                                    'x' => app.input.delete_char(),
                                    '0' => app.input.move_home(),
                                    '$' => app.input.move_end(),
                                    'd' => app.input.delete_line(), // simplified dd
                                    _ => {}
                                }
                            } else {
                                app.input.insert(c);
                            }
                        }
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
        // Process pending command from slash commands
        if let Some(cmd) = app.pending_command.take() {
            use mc_tui::PendingCommand;
            match cmd {
                PendingCommand::Compact => pending_compact = true,
                PendingCommand::Save(name) => pending_save = Some(name),
                PendingCommand::Load(name) => pending_load = Some(name),
                PendingCommand::Undo => {
                    let rt_clone = Arc::clone(&runtime);
                    let tx_clone = ui_tx.clone();
                    tokio::spawn(async move {
                        let mut rt = rt_clone.lock().await;
                        match rt.undo_last_turn() {
                            Ok(paths) if paths.is_empty() => {
                                let _ =
                                    tx_clone.try_send(UiMessage::Delta("Nothing to undo".into()));
                            }
                            Ok(paths) => {
                                let _ = tx_clone.try_send(UiMessage::Delta(format!(
                                    "↩ Reverted {} file(s): {}",
                                    paths.len(),
                                    paths.join(", ")
                                )));
                            }
                            Err(e) => {
                                let _ = tx_clone
                                    .try_send(UiMessage::Error(format!("Undo failed: {e}")));
                            }
                        }
                        let _ = tx_clone.try_send(UiMessage::Done {
                            ttft_ms: 0,
                            total_ms: 0,
                        });
                    });
                }
                PendingCommand::CostTotal => {
                    if let Ok(rt) = runtime.try_lock() {
                        let (i, o, c) = rt.cumulative_cost();
                        app.output_lines.push(format!(
                            "All-time cost: ${c:.4} ({i} input + {o} output tokens)"
                        ));
                    }
                }
                PendingCommand::ModelSwitch(name) => {
                    if let Ok(mut rt) = runtime.try_lock() {
                        let resolved = config.model_aliases.get(&name).cloned().unwrap_or(name);
                        rt.set_model(resolved.clone());
                        app.model.clone_from(&resolved);
                        app.output_lines
                            .push(format!("Switched to model: {resolved}"));
                    }
                }
                PendingCommand::Export => {
                    let path = session_path("export.md");
                    match std::fs::write(&path, app.output_lines.join("\n")) {
                        Ok(()) => app
                            .output_lines
                            .push(format!("Exported to {}", path.display())),
                        Err(e) => app.output_lines.push(format!("Export failed: {e}")),
                    }
                }
                PendingCommand::Init => {
                    let dir = std::env::current_dir()
                        .unwrap_or_default()
                        .join(".magic-code");
                    let conf = dir.join("config.toml");
                    if conf.exists() {
                        app.output_lines
                            .push(format!("Config exists: {}", conf.display()));
                    } else {
                        let _ = std::fs::create_dir_all(&dir);
                        let tmpl = "# magic-code project config\n# model = \"claude-sonnet-4-20250514\"\n# provider = \"anthropic\"\n";
                        if std::fs::write(&conf, tmpl).is_ok() {
                            let _ = std::fs::write(
                                dir.join("instructions.md"),
                                "# Project Instructions\n",
                            );
                            app.output_lines.push(format!("Created {}", conf.display()));
                        }
                    }
                }
                PendingCommand::Summary => {
                    app.output_lines.push(format!(
                        "Session: {} lines, {}↓ {}↑ tokens, ${:.4}, model: {}",
                        app.output_lines.len(),
                        app.total_input_tokens,
                        app.total_output_tokens,
                        app.session_cost,
                        app.model
                    ));
                }
                PendingCommand::Tokens => {
                    if let Ok(rt) = runtime.try_lock() {
                        let est = mc_core::estimate_tokens(&rt.session);
                        let ctx = mc_core::ModelRegistry::default().context_window(&app.model);
                        app.output_lines.push(format!(
                            "Tokens: ~{est} / {ctx} ({}%)",
                            (est as u64 * 100) / u64::from(ctx.max(1))
                        ));
                    }
                }
                PendingCommand::Context => {
                    if let Ok(rt) = runtime.try_lock() {
                        let est = mc_core::estimate_tokens(&rt.session);
                        app.output_lines.push(format!(
                            "Context: {} messages, ~{est} tokens, 11 tools",
                            rt.session.messages.len()
                        ));
                    }
                }
                PendingCommand::CopyToClipboard(text) => {
                    let cmd = if cfg!(target_os = "macos") {
                        "pbcopy"
                    } else {
                        "xclip -selection clipboard"
                    };
                    if let Ok(mut child) = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(cmd)
                        .stdin(std::process::Stdio::piped())
                        .spawn()
                    {
                        if let Some(ref mut stdin) = child.stdin {
                            use std::io::Write;
                            let _ = stdin.write_all(text.as_bytes());
                        }
                        let _ = child.wait();
                    }
                }
                PendingCommand::Review => {
                    if let Ok(o) = std::process::Command::new("git")
                        .args(["diff", "HEAD"])
                        .output()
                    {
                        let diff = String::from_utf8_lossy(&o.stdout);
                        if diff.is_empty() {
                            app.output_lines.push("No changes.".into());
                        } else {
                            for line in diff.lines() {
                                app.output_lines.push(format!("  {line}"));
                            }
                        }
                    }
                }
                PendingCommand::Retry => {
                    if let Some(ref text) = app.last_user_input.clone() {
                        app.handle_event(AppEvent::UserSubmit(text.clone()));
                    }
                }
                PendingCommand::Doctor => {
                    app.output_lines.push(format!(
                        "🩺 v{} | {} | {} | git: {}",
                        env!("CARGO_PKG_VERSION"),
                        app.model,
                        config.provider,
                        if std::process::Command::new("git")
                            .arg("--version")
                            .output()
                            .is_ok_and(|o| o.status.success())
                        {
                            "✓"
                        } else {
                            "✗"
                        }
                    ));
                }
                PendingCommand::Search(query) => {
                    let dir = session_path("")
                        .parent()
                        .unwrap_or(std::path::Path::new("."))
                        .to_path_buf();
                    // Handle /sessions list
                    if query == "__list__" {
                        let mut sessions = Vec::new();
                        if let Ok(entries) = std::fs::read_dir(&dir) {
                            for e in entries.flatten() {
                                if e.path().extension().is_some_and(|x| x == "json") {
                                    sessions.push(
                                        e.path()
                                            .file_stem()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .to_string(),
                                    );
                                }
                            }
                        }
                        if sessions.is_empty() {
                            app.output_lines.push("No saved sessions.".into());
                        } else {
                            app.output_lines
                                .push(format!("Sessions ({}):", sessions.len()));
                            for s in &sessions {
                                app.output_lines.push(format!("  {s}"));
                            }
                        }
                    } else if let Some(name) = query.strip_prefix("__delete__") {
                        let path = session_path(name);
                        match std::fs::remove_file(&path) {
                            Ok(()) => app.output_lines.push(format!("Deleted: {name}")),
                            Err(e) => app.output_lines.push(format!("  ✗ {e}")),
                        }
                    } else {
                        let mut found = Vec::new();
                        if let Ok(entries) = std::fs::read_dir(&dir) {
                            for e in entries.flatten() {
                                if e.path().extension().is_some_and(|x| x == "json") {
                                    if let Ok(c) = std::fs::read_to_string(e.path()) {
                                        if c.contains(&query) {
                                            found.push(
                                                e.path()
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
                        app.output_lines.push(if found.is_empty() {
                            format!("No sessions matching \"{query}\"")
                        } else {
                            format!("Found: {}", found.join(", "))
                        });
                    }
                }
                PendingCommand::Memory(cmd) => {
                    app.output_lines.push(format!("📌 memory: {cmd}"));
                }
                PendingCommand::ThinkingToggle => {
                    app.output_lines.push("💭 Thinking toggled".into());
                }
                PendingCommand::Branch(cmd) => {
                    app.output_lines.push(format!("🌿 branch: {cmd}"));
                }
                PendingCommand::ImageAttach(path) => {
                    if let Ok(mut rt) = runtime.try_lock() {
                        rt.attach_image(path, "image/png".into());
                    }
                }
                PendingCommand::Git(cmd) => {
                    // /ship = git add -A then commit
                    if cmd == "ship" {
                        let _ = std::process::Command::new("git")
                            .args(["add", "-A"])
                            .output();
                    }
                    let effective = if cmd == "ship" {
                        "commit".to_string()
                    } else {
                        cmd
                    };
                    let args: &[&str] = match effective.as_str() {
                        "diff" => &["diff"],
                        "log" => &["log", "--oneline", "-10"],
                        "stash" => &["stash"],
                        "stash_pop" => &["stash", "pop"],
                        "commit" => &["diff", "--cached", "--stat"],
                        _ => &["status"],
                    };
                    if let Ok(o) = std::process::Command::new("git").args(args).output() {
                        let out = String::from_utf8_lossy(&o.stdout);
                        if effective == "commit" {
                            if out.trim().is_empty() {
                                app.output_lines.push("Nothing staged.".into());
                            } else {
                                app.output_lines.push("Generating commit message...".into());
                                if let Ok(diff) = std::process::Command::new("git")
                                    .args(["diff", "--cached"])
                                    .output()
                                {
                                    let diff_text =
                                        String::from_utf8_lossy(&diff.stdout).to_string();
                                    let rt_c = Arc::clone(&runtime);
                                    let prov_c = Arc::clone(&provider);
                                    let tx_c = ui_tx.clone();
                                    tokio::spawn(async move {
                                        let rt = rt_c.lock().await;
                                        let msg =
                                            rt.generate_commit_message(&*prov_c, &diff_text).await;
                                        match std::process::Command::new("git")
                                            .args(["commit", "-m", &msg])
                                            .output()
                                        {
                                            Ok(co) => {
                                                let _ = tx_c.try_send(UiMessage::Delta(format!(
                                                    "✓ {}",
                                                    String::from_utf8_lossy(&co.stdout).trim()
                                                )));
                                            }
                                            Err(e) => {
                                                let _ = tx_c.try_send(UiMessage::Error(format!(
                                                    "commit: {e}"
                                                )));
                                            }
                                        }
                                        let _ = tx_c.try_send(UiMessage::Done {
                                            ttft_ms: 0,
                                            total_ms: 0,
                                        });
                                    });
                                }
                            }
                        } else {
                            for line in out.lines() {
                                app.output_lines.push(format!("  {line}"));
                            }
                        }
                    }
                }
                PendingCommand::Rewind(n) => {
                    if let Ok(mut rt) = runtime.try_lock() {
                        let msg_len = rt.session.messages.len();
                        let removed = (n * 2).min(msg_len);
                        rt.session.messages.truncate(msg_len - removed);
                        if let Ok(paths) = rt.undo_last_turn() {
                            if !paths.is_empty() {
                                app.output_lines
                                    .push(format!("↩ Reverted: {}", paths.join(", ")));
                            }
                        }
                        app.output_lines.push(format!("⏪ Rewound {n} turn(s)"));
                    }
                }
                PendingCommand::Debug => {
                    if let Ok(rt) = runtime.try_lock() {
                        let est = mc_core::estimate_tokens(&rt.session);
                        let ctx = mc_core::ModelRegistry::default().context_window(&app.model);
                        app.output_lines.push(format!(
                            "🔍 {} msgs, ~{est}/{ctx} tokens ({}%), ${:.4}, ttft {}ms",
                            rt.session.messages.len(),
                            (est as u64 * 100) / u64::from(ctx.max(1)),
                            app.session_cost,
                            app.ttft_ms,
                        ));
                    }
                }
                PendingCommand::Btw(question) => {
                    let rt_clone = Arc::clone(&runtime);
                    let prov_clone = Arc::clone(&provider);
                    let tx_clone = ui_tx.clone();
                    tokio::spawn(async move {
                        let rt = rt_clone.lock().await;
                        let request = mc_provider::CompletionRequest {
                            model: rt.model().to_string(),
                            max_tokens: 500,
                            system_prompt: Some("Answer briefly.".into()),
                            messages: vec![mc_provider::InputMessage {
                                role: mc_provider::types::MessageRole::User,
                                content: vec![mc_provider::types::ContentBlock::Text {
                                    text: question,
                                }],
                            }],
                            tools: Vec::new(),
                            tool_choice: None,
                            thinking_budget: None,
                        };
                        let mut stream = prov_clone.stream(&request);
                        let mut answer = String::new();
                        while let Some(Ok(ev)) = mc_core::next_event(&mut stream).await {
                            if let mc_provider::ProviderEvent::TextDelta(t) = ev {
                                answer.push_str(&t);
                            }
                        }
                        let _ = tx_clone
                            .try_send(UiMessage::Delta(format!("\n💬 btw: {}", answer.trim())));
                    });
                }
                PendingCommand::Loop {
                    interval_secs,
                    prompt,
                } => {
                    let tx_clone = ui_tx.clone();
                    app.output_lines
                        .push(format!("🔄 Loop: every {interval_secs}s"));
                    tokio::spawn(async move {
                        loop {
                            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
                            let _ = tx_clone.try_send(UiMessage::Delta(format!("\n🔄 {prompt}")));
                        }
                    });
                }
                PendingCommand::LoopStop => {
                    app.output_lines.push("🔄 Loop stopped".into());
                }
                PendingCommand::RunShell(cmd) => {
                    let tx_clone = ui_tx.clone();
                    tokio::spawn(async move {
                        let output = tokio::process::Command::new("sh")
                            .arg("-c")
                            .arg(&cmd)
                            .output()
                            .await;
                        match output {
                            Ok(o) => {
                                let stdout = String::from_utf8_lossy(&o.stdout);
                                let stderr = String::from_utf8_lossy(&o.stderr);
                                let mut result = String::new();
                                for line in stdout.lines() {
                                    result.push_str(&format!("  {line}\n"));
                                }
                                if !stderr.is_empty() {
                                    result.push_str(&format!("  STDERR: {}", stderr.trim()));
                                }
                                let _ = tx_clone.try_send(UiMessage::Delta(result));
                                let _ = tx_clone.try_send(UiMessage::Done {
                                    ttft_ms: 0,
                                    total_ms: 0,
                                });
                            }
                            Err(e) => {
                                let _ = tx_clone.try_send(UiMessage::Error(e.to_string()));
                            }
                        }
                    });
                }
                PendingCommand::AcceptEdit { path, diff } => {
                    app.output_lines.push(format!("📝 Edit preview: {path}"));
                    for line in diff.lines() {
                        app.output_lines.push(format!("  {line}"));
                    }
                    app.output_lines
                        .push("  [Y]es to apply, [N]o to reject".into());
                }
            }
        }

        // Handle deferred operations that need runtime lock
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
    json_output: bool,
    extra_dirs: &[String],
    mcp_servers: &[mc_config::McpServerConfig],
) -> Result<()> {
    let cancel = CancellationToken::new();
    let mut runtime =
        mc_core::ConversationRuntime::new(model.to_string(), max_tokens, system.to_string());
    let mut tool_registry = mc_tools::ToolRegistry::new()
        .with_workspace_root(std::env::current_dir().unwrap_or_default());
    for dir in extra_dirs {
        let path = std::path::PathBuf::from(dir);
        if path.is_dir() {
            tool_registry = tool_registry.with_extra_root(path);
        }
    }
    for mcp in mcp_servers {
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
    runtime.set_tool_registry(tool_registry);
    if !hooks.is_empty() {
        runtime.set_hooks(mc_tools::HookEngine::new(hooks));
    }
    // Load hierarchical instructions (CLAUDE.md, AGENTS.md from root to cwd)
    let cwd = std::env::current_dir().unwrap_or_default();
    let instructions = mc_config::load_hierarchical_instructions(&cwd);
    if !instructions.is_empty() {
        let combined: String = instructions
            .iter()
            .map(|(path, content)| {
                let resolved = mc_config::resolve_includes(
                    path.parent().unwrap_or(std::path::Path::new(".")),
                    content,
                );
                format!("\n\n# Instructions from {}\n{}", path.display(), resolved)
            })
            .collect();
        runtime.set_hierarchical_instructions(combined);
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
    if json_output {
        let json = serde_json::json!({
            "text": result.text,
            "tool_calls": result.tool_calls,
            "input_tokens": result.usage.input_tokens,
            "output_tokens": result.usage.output_tokens,
            "iterations": result.iterations,
            "cancelled": result.cancelled,
            "model": model,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&json).unwrap_or_default()
        );
        return Ok(());
    }
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
         - `bash`: Execute shell commands. Prefer short, targeted commands. Output streams in real-time.\n\
         - `read_file`: Read files with optional offset/limit for large files.\n\
         - `write_file`: Create or overwrite files. Always write complete file content.\n\
         - `edit_file`: Replace specific text in files. Use for surgical edits — always include enough context in old_string to match uniquely.\n\
         - `glob_search`: Find files by pattern. Use before reading to locate files.\n\
         - `grep_search`: Search file contents with regex. Use to find code references.\n\
         - `subagent`: Delegate independent subtasks to an isolated agent with its own context.\n\
         - `web_fetch`: Fetch content from a URL. Use to read documentation or API specs.\n\
         - `web_search`: Search the web for current information.\n\
         - `memory_read`/`memory_write`: Read/write persistent project facts across sessions.\n\n\
         ## Tool Usage Guidelines\n\
         - Always read a file before editing it.\n\
         - Use `edit_file` for small changes (< 20 lines), `write_file` for new files or major rewrites.\n\
         - Use `glob_search` first to find files, then `read_file` to examine them.\n\
         - Use `grep_search` to find specific patterns across the codebase.\n\
         - Run tests after changes: `bash` with the project's test command.\n\
         - For complex tasks with independent parts, use `subagent` to parallelize.\n\
         - Use `web_fetch` to read documentation when unsure about APIs or libraries.\n\n\
         ## Error Recovery\n\
         - If a tool call fails, read the error message carefully and try a different approach.\n\
         - If `edit_file` fails (old_string not found), `read_file` first to see current content.\n\
         - If `bash` times out, try breaking the command into smaller steps.\n\
         - If you're stuck, explain what you've tried and ask the user for guidance.\n\n\
         ## Output Format\n\
         - Be concise. Show code, not explanations of code.\n\
         - Use markdown for formatting.\n\
         - When showing file changes, mention the file path and what changed.\n\
         - After making changes, summarize what was done."
            .to_string(),
        format!("Working directory: {}", project.cwd.display()),
        format!("OS: {}, Arch: {}", std::env::consts::OS, std::env::consts::ARCH),
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
