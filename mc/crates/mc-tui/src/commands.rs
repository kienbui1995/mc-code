use crate::app::{App, EffortLevel, PendingCommand};

/// Dispatch a slash command. All I/O-heavy commands route through
/// `PendingCommand::RunShell` so main.rs can execute them async.
pub fn handle(app: &mut App, cmd: &str) {
    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
    let arg = parts.get(1).copied().unwrap_or("");

    match parts[0] {
        "/h" | "/?" => handle(app, "/help"),
        "/q" | "/exit" => handle(app, "/quit"),
        "/new" | "/reset" => handle(app, "/clear"),
        "/continue" => handle(app, "/resume"),
        "/v" => handle(app, "/version"),
        "/settings" => handle(app, "/config"),
        "/allowed-tools" => handle(app, "/permissions"),
        "/upgrade" => handle(app, "/update"),
        "/checkpoint" => handle(app, "/rewind"),
        "/help" => app.push("Commands: /help /quit /status /cost /plan /compact /undo /update /save /load /image /memory /thinking /fork /branches /switch /diff /log /commit /stash /clear /export /model /init /summary /search /doctor /template /review /retry /pin /theme /copy /version /history /tokens /context /alias /run /grep /tree /head /tail /cat /files /wc /todo /recent /test /ship /open /pwd /env /size /models /time /whoami /tip /last /spec /vim /effort /rewind /debug /btw /loop /cron /config /add /sessions /permissions /dry-run"),
        "/quit" => app.should_quit = true,
        "/status" => cmd_status(app),
        "/plan" => cmd_plan(app),
        "/clear" => cmd_clear(app),
        "/cost" => cmd_cost(app, arg),
        "/save" => { let n = if arg.is_empty() { "default" } else { arg }; app.push(&format!("Session save requested: {n}")); app.pending_command = Some(PendingCommand::Save(n.into())); }
        "/load" => { let n = if arg.is_empty() { "default" } else { arg }; app.push(&format!("Session load requested: {n}")); app.pending_command = Some(PendingCommand::Load(n.into())); }
        "/compact" => { app.push("Compaction requested."); app.pending_command = Some(PendingCommand::Compact); }
        "/undo" => app.pending_command = Some(PendingCommand::Undo),
        "/update" => { app.push("Checking for updates..."); app.pending_command = Some(PendingCommand::RunShell("curl -sf https://api.github.com/repos/kienbui1995/mc-code/releases/latest | grep -o '\"tag_name\":\"[^\"]*' | cut -d'\"' -f4".into())); },
        "/image" => cmd_image(app, arg),
        "/memory" => app.pending_command = Some(PendingCommand::Memory(if arg.is_empty() { "list".into() } else { arg.into() })),
        "/thinking" => app.pending_command = Some(PendingCommand::ThinkingToggle),
        "/fork" => app.pending_command = Some(PendingCommand::Branch("fork".into())),
        "/branches" => app.pending_command = Some(PendingCommand::Branch("list".into())),
        "/switch" => if arg.is_empty() { app.push("Usage: /switch <branch-name>"); } else { app.pending_command = Some(PendingCommand::Branch(format!("switch {arg}"))); },
        "/branch" => if arg.is_empty() { app.push("Usage: /branch delete <name>"); } else { app.pending_command = Some(PendingCommand::Branch(arg.into())); },
        "/diff" => app.pending_command = Some(PendingCommand::Git("diff".into())),
        "/log" => app.pending_command = Some(PendingCommand::Git("log".into())),
        "/commit" => app.pending_command = Some(PendingCommand::Git("commit".into())),
        "/stash" => app.pending_command = Some(PendingCommand::Git(if arg == "pop" { "stash_pop" } else { "stash" }.into())),
        "/export" => {
            let fmt = if arg == "json" { "json" } else { "md" };
            app.pending_command = Some(PendingCommand::Export(fmt.into()));
        }
        "/model" => cmd_model(app, arg),
        "/init" => app.pending_command = Some(PendingCommand::Init),
        "/summary" => app.pending_command = Some(PendingCommand::Summary),
        "/search" => if arg.is_empty() { app.push("Usage: /search <keyword>"); } else { app.pending_command = Some(PendingCommand::Search(arg.into())); },
        "/doctor" => app.pending_command = Some(PendingCommand::Doctor),
        "/review" => app.pending_command = Some(PendingCommand::Review),
        "/security-review" => {
            let prompt = "Perform a security audit of the recent code changes. Check for:\n1. Injection vulnerabilities (SQL, command, path traversal)\n2. Authentication/authorization issues\n3. Credential exposure or hardcoded secrets\n4. Unsafe data handling (unsanitized input, buffer issues)\n5. Dependency vulnerabilities\n6. Race conditions and TOCTOU bugs\n\nBe specific about file paths and line numbers. Rate each finding by severity (Critical/High/Medium/Low).";
            app.input.set(prompt);
            app.push("🔒 Security review mode: AI will audit for vulnerabilities");
        },
        "/tokens" => app.pending_command = Some(PendingCommand::Tokens),
        "/context" => app.pending_command = Some(PendingCommand::Context),
        "/debug" => app.pending_command = Some(PendingCommand::Debug),
        "/dry-run" => cmd_dryrun(app),
        "/retry" => cmd_retry(app),
        "/pin" => { let idx = app.output_lines.len().saturating_sub(1); app.pinned_messages.push(idx); app.push(&format!("📌 Pinned message at line {idx}")); }
        "/theme" => { app.theme = if app.theme == "dark" { "light".into() } else { "dark".into() }; app.push(&format!("Theme: {}", app.theme)); }
        "/copy" => cmd_copy(app),
        "/version" => app.push(&format!("magic-code v{} ({} {})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH)),
        "/history" => cmd_history(app),
        "/alias" => cmd_alias(app, arg),
        "/time" => { let e = app.session_start.elapsed(); app.push(&format!("Session time: {}m {}s", e.as_secs() / 60, e.as_secs() % 60)); }
        "/whoami" => app.push(&format!("Model: {} | Plan: {} | Dry-run: {} | Theme: {}", app.model, if app.plan_mode { "ON" } else { "OFF" }, if app.dry_run { "ON" } else { "OFF" }, app.theme)),
        "/tip" => app.push(&format!("💡 {}", random_tip())),
        "/last" => cmd_last(app),
        "/models" => { app.push("Known models:"); for m in [
                    "claude-sonnet-4-20250514", "claude-haiku", "gpt-4o", "gpt-4o-mini",
                    "gemini-2.5-flash", "gemini-2.5-pro",
                    "llama-3.3-70b-versatile", "deepseek-chat", "mistral-large-latest",
                    "grok-2", "sonar-pro", "command-r-plus",
                ] { app.push(&format!("  {m}")); } }
        "/providers" => cmd_providers(app),
        "/pwd" => app.push(&format!("  {}", std::env::current_dir().unwrap_or_default().display())),
        "/env" => cmd_env(app),
        "/vim" => { app.vim_mode = if app.vim_mode.is_some() { app.push("Vim mode: OFF"); None } else { app.push("Vim mode: ON (Esc=Normal, i=Insert)"); Some(crate::input::VimMode::Insert) }; }
        "/effort" => cmd_effort(app, arg),
        "/template" => cmd_template(app, arg),
        "/spec" => cmd_spec(app, arg),
        "/config" => cmd_config(app),
        "/permissions" => cmd_permissions(app, arg),
        "/btw" => if arg.is_empty() { app.push("Usage: /btw <question>"); } else { app.pending_command = Some(PendingCommand::Btw(arg.into())); },
        "/loop" => cmd_loop(app, arg),
        "/cron" => cmd_cron(app, arg),
        "/rewind" => if let Ok(n) = arg.parse::<usize>() { app.pending_command = Some(PendingCommand::Rewind(n)); } else { app.push("Usage: /rewind <n>"); },

        // --- Async shell commands (non-blocking via PendingCommand::RunShell) ---
        "/run" => if arg.is_empty() { app.push("Usage: /run <command>"); } else { app.push(&format!("$ {arg}")); app.pending_command = Some(PendingCommand::RunShell(arg.into())); },
        "/grep" => cmd_grep(app, arg),
        "/cat" => if arg.is_empty() { app.push("Usage: /cat <file>"); } else { app.pending_command = Some(PendingCommand::RunShell(format!("head -100 {arg}"))); },
        "/head" => cmd_head(app, arg),
        "/tail" => cmd_tail(app, arg),
        "/files" => { let p = if arg.is_empty() { "." } else { arg }; app.pending_command = Some(PendingCommand::RunShell(format!("ls -la {p}"))); },
        "/tree" => { let d = arg.parse::<u8>().unwrap_or(2); app.pending_command = Some(PendingCommand::RunShell(format!("find . -maxdepth {d} -not -path '*/target/*' -not -path '*/.git/*' -not -path '*/node_modules/*' | sort | head -80"))); },
        "/wc" => app.pending_command = Some(PendingCommand::RunShell("find . -name '*.rs' -o -name '*.py' -o -name '*.ts' -o -name '*.go' | head -500 | xargs wc -l 2>/dev/null | tail -1".into())),
        "/todo" => app.pending_command = Some(PendingCommand::RunShell("grep -rn --color=never 'TODO\\|FIXME\\|HACK\\|XXX' . --include='*.rs' --include='*.py' --include='*.ts' --include='*.js' --include='*.go' 2>/dev/null | grep -v target/ | head -30".into())),
        "/recent" => app.pending_command = Some(PendingCommand::RunShell("find . -name '*.rs' -o -name '*.py' -o -name '*.ts' -o -name '*.js' -o -name '*.go' -o -name '*.toml' -o -name '*.md' | xargs ls -lt 2>/dev/null | head -15".into())),
        "/test" => cmd_test(app),
        "/ship" => { app.push("Staging all changes..."); app.pending_command = Some(PendingCommand::Git("ship".into())); },
        "/open" => if arg.is_empty() { app.push("Usage: /open <file>"); } else { let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into()); app.push(&format!("Opening {arg} in {editor}...")); let _ = std::process::Command::new(&editor).arg(arg).status(); },
        "/size" => if arg.is_empty() { app.push("Usage: /size <file>"); } else { app.pending_command = Some(PendingCommand::RunShell(format!("stat --printf='%s bytes' {arg} 2>/dev/null || stat -f '%z bytes' {arg}"))); },
        "/add" => cmd_add(app, arg),
        "/sessions" => cmd_sessions(app, arg),
        "/connect" => cmd_connect(app, arg),
        "/tasks" => { app.push("Listing tasks..."); app.pending_command = Some(PendingCommand::Btw("__tasks_list__".into())); },
        "/agents" => { app.push("Sub-agents: managed by runtime (use subagent tool to spawn)"); app.pending_command = Some(PendingCommand::Btw("__agents_list__".into())); },
        "/resume" => cmd_resume(app, arg),

        _ => cmd_unknown(app, cmd, parts[0]),
    }
}

// --- Helper functions ---

fn cmd_status(app: &mut App) {
    app.push(&format!(
        "Model: {} | Tokens: {}↓ {}↑ | Messages: {} | Plan mode: {}",
        app.model,
        app.total_input_tokens,
        app.total_output_tokens,
        app.output_lines.len(),
        app.plan_mode
    ));
}

fn cmd_plan(app: &mut App) {
    app.plan_mode = !app.plan_mode;
    app.push(&format!(
        "Plan mode: {}",
        if app.plan_mode {
            "ON (LLM will plan, not execute)"
        } else {
            "OFF"
        }
    ));
}

fn cmd_clear(app: &mut App) {
    app.output_lines.clear();
    app.push("Output cleared. Session history preserved.");
    app.scroll_offset = 0;
}

fn cmd_cost(app: &mut App, arg: &str) {
    if arg == "--total" {
        app.pending_command = Some(PendingCommand::CostTotal);
    } else {
        app.push(&format!(
            "Session cost: ${:.4} ({} input + {} output tokens)",
            app.session_cost, app.total_input_tokens, app.total_output_tokens
        ));
    }
}

fn cmd_image(app: &mut App, arg: &str) {
    if arg.is_empty() {
        app.push("Usage: /image <path> [prompt]");
    } else {
        app.push(&format!("  🖼 image: {arg}"));
        app.pending_command = Some(PendingCommand::ImageAttach(arg.into()));
    }
}

fn cmd_model(app: &mut App, arg: &str) {
    const MODELS: &[(&str, &str, &str)] = &[
        ("claude-sonnet-4-20250514", "anthropic", "200K ctx, $3/$15"),
        (
            "claude-haiku-3-5-20241022",
            "anthropic",
            "200K ctx, $0.8/$4",
        ),
        ("gpt-4o", "openai", "128K ctx, $2.5/$10"),
        ("gpt-4o-mini", "openai", "128K ctx, $0.15/$0.6"),
        ("gemini-2.5-flash", "gemini", "1M ctx, $0.15/$0.6"),
        ("gemini-2.5-pro", "gemini", "1M ctx, $1.25/$10"),
        ("llama-3.3-70b-versatile", "groq", "128K ctx, fast"),
        ("deepseek-chat", "deepseek", "128K ctx, $0.14/$0.28"),
        ("mistral-large-latest", "mistral", "128K ctx, $2/$6"),
        ("grok-2", "xai", "131K ctx, $2/$10"),
        ("sonar-pro", "perplexity", "200K ctx, search-augmented"),
        ("command-r-plus", "cohere", "128K ctx, $2.5/$10"),
    ];
    if arg.is_empty() {
        app.push(&format!(
            "Current: {} | Select by number or name:",
            app.model
        ));
        for (i, (name, provider, info)) in MODELS.iter().enumerate() {
            let marker = if *name == app.model { " ←" } else { "" };
            app.push(&format!(
                "  {:>2}. {:<32} {:<12} {}{}",
                i + 1,
                name,
                provider,
                info,
                marker
            ));
        }
        app.push("  Usage: /model <number> or /model <name>");
    } else if let Ok(n) = arg.parse::<usize>() {
        if n >= 1 && n <= MODELS.len() {
            app.pending_command = Some(PendingCommand::ModelSwitch(MODELS[n - 1].0.to_string()));
        } else {
            app.push(&format!("Invalid number. Use 1-{}", MODELS.len()));
        }
    } else {
        app.pending_command = Some(PendingCommand::ModelSwitch(arg.into()));
    }
}

fn cmd_dryrun(app: &mut App) {
    app.dry_run = !app.dry_run;
    app.push(&format!(
        "Dry-run mode: {}",
        if app.dry_run {
            "ON (tools shown but not executed)"
        } else {
            "OFF"
        }
    ));
}

fn cmd_retry(app: &mut App) {
    if app.last_user_input.is_some() {
        app.push("⟳ Retrying...");
        app.pending_command = Some(PendingCommand::Retry);
    } else {
        app.push("Nothing to retry.");
    }
}

fn cmd_copy(app: &mut App) {
    let last_response: String = app
        .output_lines
        .iter()
        .rev()
        .take_while(|l| !l.starts_with('›'))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    app.pending_command = Some(PendingCommand::CopyToClipboard(last_response));
    app.push("📋 Copied to clipboard.");
}

fn cmd_history(app: &mut App) {
    app.push("Input history:");
    let entries: Vec<String> = app
        .history
        .entries()
        .iter()
        .rev()
        .take(20)
        .enumerate()
        .map(|(i, e)| format!("  {}: {e}", i + 1))
        .collect();
    for line in entries {
        app.push(&line);
    }
}

fn cmd_alias(app: &mut App, arg: &str) {
    if arg.is_empty() {
        if app.aliases.is_empty() {
            app.push("No aliases. Usage: /alias <name> <expansion>");
        } else {
            let lines: Vec<String> = app
                .aliases
                .iter()
                .map(|(k, v)| format!("  {k} → {v}"))
                .collect();
            for line in lines {
                app.push(&line);
            }
        }
    } else if let Some((name, expansion)) = arg.split_once(' ') {
        app.aliases
            .insert(format!("/{name}"), expansion.to_string());
        app.push(&format!("Alias: /{name} → {expansion}"));
    } else {
        app.push("Usage: /alias <name> <expansion>");
    }
}

fn cmd_last(app: &mut App) {
    if let Some(out) = app.last_tool_output.clone() {
        for line in out.lines().take(50) {
            app.push(&format!("  {line}"));
        }
    } else {
        app.push("No tool output yet.");
    }
}

fn cmd_env(app: &mut App) {
    for var in [
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "GEMINI_API_KEY",
        "EDITOR",
        "SHELL",
        "HOME",
    ] {
        let val = std::env::var(var).unwrap_or_else(|_| "(not set)".into());
        let masked = if var.contains("KEY") && val.len() > 8 {
            format!("...{}", &val[val.len() - 4..])
        } else {
            val
        };
        app.push(&format!("  {var}={masked}"));
    }
}

fn cmd_effort(app: &mut App, arg: &str) {
    if arg.is_empty() {
        app.effort = app.effort.next();
        app.push(&format!("Effort: {} {:?}", app.effort.symbol(), app.effort));
    } else {
        app.effort = match arg {
            "low" | "l" => EffortLevel::Low,
            "medium" | "med" | "m" => EffortLevel::Medium,
            "high" | "h" => EffortLevel::High,
            _ => {
                app.push("Usage: /effort low|medium|high");
                return;
            }
        };
        app.push(&format!("Effort: {} {:?}", app.effort.symbol(), app.effort));
    }
}

fn cmd_template(app: &mut App, arg: &str) {
    if arg.is_empty() {
        app.push("Templates: review, refactor, test, explain, document, optimize, security");
        app.push("Usage: /template <name>");
        return;
    }
    let prompt = match arg {
        "review" => "Review the recent code changes. Check for bugs, security issues, performance problems, and style. Be specific about line numbers.",
        "refactor" => "Refactor the code I'm about to show you. Improve readability, reduce duplication, and follow best practices. Show the changes as diffs.",
        "test" => "Write comprehensive tests for the code I'm about to show you. Cover edge cases, error paths, and happy paths.",
        "explain" => "Explain this code in detail. What does it do, how does it work, and what are the key design decisions?",
        "document" => "Add documentation to this code. Include doc comments, inline comments for complex logic, and a module-level overview.",
        "optimize" => "Analyze this code for performance. Identify bottlenecks and suggest optimizations with benchmarks.",
        "security" => "Audit this code for security vulnerabilities. Check for injection, auth issues, data leaks, and unsafe patterns.",
        _ => { app.push(&format!("Unknown template: {arg}")); return; }
    };
    app.push(&format!("📋 Template: {arg}"));
    app.input.set(prompt);
}

fn cmd_spec(app: &mut App, arg: &str) {
    let prompt = if arg.is_empty() {
        "Before writing any code, create a brief technical specification:\n1. Requirements\n2. Approach\n3. Files to modify\n4. Edge cases\n5. Testing strategy\n\nThen ask me to confirm before proceeding.".to_string()
    } else {
        format!("Before implementing: {arg}\n\nCreate a brief technical specification:\n1. Requirements\n2. Approach\n3. Files to modify\n4. Edge cases\n5. Testing strategy\n\nThen ask me to confirm before proceeding.")
    };
    app.input.set(&prompt);
    app.push("📋 Spec mode: AI will plan before coding");
}

fn cmd_config(app: &mut App) {
    app.push("Current config:");
    app.push(&format!("  model: {}", app.model));
    app.push(&format!("  plan_mode: {}", app.plan_mode));
    app.push(&format!("  dry_run: {}", app.dry_run));
    app.push(&format!("  theme: {}", app.theme));
    app.push(&format!(
        "  session_time: {}s",
        app.session_start.elapsed().as_secs()
    ));
    app.push(&format!(
        "  tokens: {}↓ {}↑",
        app.total_input_tokens, app.total_output_tokens
    ));
    app.push(&format!("  cost: ${:.4}", app.session_cost));
    app.push(&format!("  context: {}%", app.context_usage_pct));
    app.push("  Edit: .magic-code/config.toml");
}

fn cmd_permissions(app: &mut App, arg: &str) {
    if arg.is_empty() {
        app.push(&format!(
            "Permission mode: {} | Dry-run: {}",
            if app.dry_run { "dry-run" } else { "active" },
            if app.dry_run { "ON" } else { "OFF" }
        ));
        app.push("  Modes: read-only, workspace-write, full-access");
        app.push("  Toggle dry-run: /dry-run");
    } else {
        app.push(&format!("Permission mode → {arg} (restart to apply)"));
    }
}

fn cmd_loop(app: &mut App, arg: &str) {
    if arg == "stop" {
        app.pending_command = Some(PendingCommand::LoopStop);
    } else {
        let parts: Vec<&str> = arg.splitn(2, ' ').collect();
        if parts.len() == 2 {
            let secs = parse_interval(parts[0]);
            if secs > 0 {
                app.pending_command = Some(PendingCommand::Loop {
                    interval_secs: secs,
                    prompt: parts[1].into(),
                });
                app.push(&format!("🔄 Loop: every {secs}s"));
            } else {
                app.push("Invalid interval. Use: /loop 5m <prompt>");
            }
        } else {
            app.push("Usage: /loop <interval> <prompt> | /loop stop");
        }
    }
}

fn cmd_cron(app: &mut App, arg: &str) {
    if arg.is_empty() {
        app.push("Usage: /cron add <name> <interval> <prompt>");
        app.push("       /cron remove <name>");
        app.push("       /cron list");
        app.push("Intervals: 30s, 5m, 1h");
    } else if arg == "list" {
        app.push("Cron triggers: (managed by runtime)");
        app.pending_command = Some(PendingCommand::Btw("__cron_list__".into()));
    } else if arg.starts_with("remove ") {
        let name = arg.strip_prefix("remove ").unwrap_or("").trim();
        app.push(&format!("Removing cron trigger: {name}"));
        app.pending_command = Some(PendingCommand::Btw(format!("__cron_remove__{name}")));
    } else if arg.starts_with("add ") {
        let parts: Vec<&str> = arg
            .strip_prefix("add ")
            .unwrap_or("")
            .splitn(3, ' ')
            .collect();
        if parts.len() >= 3 {
            let (name, interval, prompt) = (parts[0], parts[1], parts[2]);
            let secs = parse_interval(interval);
            if secs > 0 {
                app.pending_command = Some(PendingCommand::Loop {
                    interval_secs: secs,
                    prompt: format!("__cron_add__{name}__{prompt}"),
                });
                app.push(&format!("Cron trigger '{name}' added: every {secs}s"));
            } else {
                app.push("Invalid interval. Use: 30s, 5m, 1h");
            }
        } else {
            app.push("Usage: /cron add <name> <interval> <prompt>");
        }
    } else {
        app.push("Unknown cron subcommand. Use: add, remove, list");
    }
}

fn cmd_grep(app: &mut App, arg: &str) {
    if arg.is_empty() {
        app.push("Usage: /grep <pattern> [path]");
    } else {
        let parts: Vec<&str> = arg.splitn(2, ' ').collect();
        let (pattern, path) = (parts[0], parts.get(1).unwrap_or(&"."));
        // Shell-escape pattern to prevent injection
        let escaped = pattern.replace('\'', "'\\''");
        app.pending_command = Some(PendingCommand::RunShell(format!(
            "grep -rn --color=never '{escaped}' {path} | head -30"
        )));
    }
}

fn cmd_head(app: &mut App, arg: &str) {
    if arg.is_empty() {
        app.push("Usage: /head <file> [lines]");
    } else {
        let parts: Vec<&str> = arg.splitn(2, ' ').collect();
        let n = parts
            .get(1)
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(20);
        app.pending_command = Some(PendingCommand::RunShell(format!(
            "head -n {n} {}",
            parts[0]
        )));
    }
}

fn cmd_tail(app: &mut App, arg: &str) {
    if arg.is_empty() {
        app.push("Usage: /tail <file> [lines]");
    } else {
        let parts: Vec<&str> = arg.splitn(2, ' ').collect();
        let n = parts
            .get(1)
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(20);
        app.pending_command = Some(PendingCommand::RunShell(format!(
            "tail -n {n} {}",
            parts[0]
        )));
    }
}

fn cmd_test(app: &mut App) {
    let cmd = if std::path::Path::new("Cargo.toml").exists()
        || std::path::Path::new("mc/Cargo.toml").exists()
    {
        "cargo test --workspace 2>&1 | tail -20"
    } else if std::path::Path::new("package.json").exists() {
        "npm test 2>&1 | tail -20"
    } else if std::path::Path::new("pytest.ini").exists()
        || std::path::Path::new("setup.py").exists()
    {
        "python -m pytest 2>&1 | tail -20"
    } else if std::path::Path::new("go.mod").exists() {
        "go test ./... 2>&1 | tail -20"
    } else if std::path::Path::new("Makefile").exists() {
        "make test 2>&1 | tail -20"
    } else {
        app.push("No test runner detected.");
        return;
    };
    app.push(&format!(
        "Running: {}",
        cmd.split(" 2>&1").next().unwrap_or(cmd)
    ));
    app.pending_command = Some(PendingCommand::RunShell(cmd.into()));
}

fn cmd_add(app: &mut App, arg: &str) {
    if arg.is_empty() {
        app.push("Usage: /add <file|dir>");
        return;
    }
    let p = std::path::Path::new(arg);
    if p.is_file() {
        match std::fs::read_to_string(p) {
            Ok(content) => {
                let lines = content.lines().count();
                let current = app.input.as_str().to_string();
                let prefix = if current.is_empty() {
                    String::new()
                } else {
                    format!("{current}\n\n")
                };
                app.input.set(&format!(
                    "{prefix}[{arg} ({lines} lines)]:\n```\n{content}\n```"
                ));
                app.push(&format!("📎 Added {arg} ({lines} lines) to input"));
            }
            Err(e) => app.push(&format!("  ✗ {e}")),
        }
    } else if p.is_dir() {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(p) {
            for e in entries.flatten().take(20) {
                if e.path().is_file() {
                    files.push(e.path().display().to_string());
                }
            }
        }
        app.push(&format!("📁 Dir {arg}: {} files", files.len()));
        for f in &files {
            app.push(&format!("  {f}"));
        }
    } else {
        app.push(&format!("  ✗ not found: {arg}"));
    }
}

fn cmd_sessions(app: &mut App, arg: &str) {
    if arg.starts_with("delete ") {
        let name = &arg[7..];
        app.pending_command = Some(PendingCommand::Search(format!("__delete__{name}")));
        app.push(&format!("Deleting session: {name}"));
    } else {
        app.pending_command = Some(PendingCommand::Search("__list__".into()));
    }
}

fn cmd_resume(app: &mut App, arg: &str) {
    if arg.is_empty() {
        app.pending_command = Some(PendingCommand::Search("__list__".into()));
    } else {
        app.push(&format!("Searching sessions for: {arg}"));
        app.pending_command = Some(PendingCommand::Load(arg.into()));
    }
}

fn cmd_unknown(app: &mut App, cmd: &str, cmd_name: &str) {
    let name = cmd_name.trim_start_matches('/');
    if let Some((_, content)) = app.custom_commands.iter().find(|(n, _)| n == name) {
        let args = cmd.splitn(2, ' ').nth(1).unwrap_or("");
        let prompt = content.replace("$ARGUMENTS", args);
        app.input.set(&prompt);
        app.push(&format!("📋 Command: {name}"));
    } else {
        app.push(&format!("Unknown command: {cmd}"));
    }
}

fn parse_interval(s: &str) -> u64 {
    let s = s.trim();
    if let Some(n) = s.strip_suffix('s') {
        n.parse::<u64>().unwrap_or(0)
    } else if let Some(n) = s.strip_suffix('m') {
        n.parse::<u64>().unwrap_or(0) * 60
    } else if let Some(n) = s.strip_suffix('h') {
        n.parse::<u64>().unwrap_or(0) * 3600
    } else {
        s.parse::<u64>().unwrap_or(0)
    }
}

fn cmd_providers(app: &mut App) {
    app.push("Configured providers (use /model <name> to switch):");
    let providers = [
        ("anthropic", "ANTHROPIC_API_KEY", "claude-sonnet-4-20250514"),
        ("openai", "OPENAI_API_KEY", "gpt-4o"),
        ("gemini", "GEMINI_API_KEY", "gemini-2.5-flash"),
        ("groq", "GROQ_API_KEY", "llama-3.3-70b-versatile"),
        ("deepseek", "DEEPSEEK_API_KEY", "deepseek-chat"),
        ("mistral", "MISTRAL_API_KEY", "mistral-large-latest"),
        ("xai", "XAI_API_KEY", "grok-2"),
        (
            "openrouter",
            "OPENROUTER_API_KEY",
            "anthropic/claude-sonnet-4",
        ),
        (
            "together",
            "TOGETHER_API_KEY",
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        ),
        ("perplexity", "PERPLEXITY_API_KEY", "sonar-pro"),
        ("cohere", "COHERE_API_KEY", "command-r-plus"),
        ("cerebras", "CEREBRAS_API_KEY", "llama3.1-70b"),
        ("ollama", "(local)", "llama3"),
        ("lmstudio", "(local)", "default"),
        ("llamacpp", "(local)", "default"),
    ];
    for (name, env_var, default_model) in providers {
        let status = if env_var.starts_with('(') {
            "local".to_string()
        } else if std::env::var(env_var)
            .ok()
            .filter(|k| !k.is_empty())
            .is_some()
        {
            "✓ configured".to_string()
        } else {
            "✗ no key".to_string()
        };
        app.push(&format!(
            "  {name:<12} {status:<14} default: {default_model}"
        ));
    }
}

fn cmd_connect(app: &mut App, arg: &str) {
    if arg.is_empty() {
        app.push("Usage: /connect <provider>");
        app.push("Available: anthropic, openai, gemini, groq, deepseek, mistral, xai, openrouter, together, perplexity, cohere, cerebras, ollama, lmstudio, llamacpp");
        app.push("Example: /connect groq");
        return;
    }
    let (env_var, url) = match arg.trim() {
        "anthropic" => ("ANTHROPIC_API_KEY", "https://console.anthropic.com/"),
        "openai" => ("OPENAI_API_KEY", "https://platform.openai.com/api-keys"),
        "gemini" => ("GEMINI_API_KEY", "https://aistudio.google.com/apikey"),
        "groq" => ("GROQ_API_KEY", "https://console.groq.com/keys"),
        "deepseek" => ("DEEPSEEK_API_KEY", "https://platform.deepseek.com/api_keys"),
        "mistral" => ("MISTRAL_API_KEY", "https://console.mistral.ai/api-keys"),
        "xai" => ("XAI_API_KEY", "https://console.x.ai/"),
        "openrouter" => ("OPENROUTER_API_KEY", "https://openrouter.ai/keys"),
        "together" => (
            "TOGETHER_API_KEY",
            "https://api.together.xyz/settings/api-keys",
        ),
        "perplexity" => (
            "PERPLEXITY_API_KEY",
            "https://www.perplexity.ai/settings/api",
        ),
        "cohere" => ("COHERE_API_KEY", "https://dashboard.cohere.com/api-keys"),
        "cerebras" => ("CEREBRAS_API_KEY", "https://cloud.cerebras.ai/"),
        "ollama" => {
            app.push("Ollama: no key needed. Install from https://ollama.ai and run `ollama pull llama3`");
            app.push("Then: /model llama3");
            return;
        }
        "lmstudio" => {
            app.push("LM Studio: no key needed. Download from https://lmstudio.ai");
            app.push("Start server, then: /model default --provider lmstudio");
            return;
        }
        "llamacpp" => {
            app.push("llama.cpp: no key needed. Run `llama-server -m model.gguf`");
            app.push("Then: /model default --provider llamacpp");
            return;
        }
        other => {
            app.push(&format!(
                "Unknown provider: {other}. Run /connect for list."
            ));
            return;
        }
    };
    let has_key = std::env::var(env_var)
        .ok()
        .filter(|k| !k.is_empty())
        .is_some();
    if has_key {
        app.push(&format!("✓ {arg} already configured ({env_var} is set)"));
        app.push(&format!("  Switch with: /model <model-name>"));
    } else {
        app.push(&format!("To connect {arg}:"));
        app.push(&format!("  1. Get API key: {url}"));
        app.push(&format!("  2. Set: export {env_var}=<your-key>"));
        app.push(&format!("  3. Switch: /model <model-name>"));
    }
}

pub fn random_tip() -> &'static str {
    const TIPS: &[&str] = &[
        "Use @filename to auto-include file content in your prompt",
        "Press Tab to auto-complete slash commands",
        "/compact shrinks context when you're running low",
        "/undo reverts the last turn's file changes",
        "/template review — quick code review prompt",
        "/effort high — enable deep thinking for complex tasks",
        "/ship — stage all + LLM commit message in one command",
        "/test — auto-detect and run your test suite",
    ];
    TIPS[std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize
        % TIPS.len()]
}
