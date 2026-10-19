//! CLI-based providers: invoke models through their CLI tools.
//!
//! This wraps codex, gemini, and opencode CLIs as Provider implementations.
//! CLI providers are the simplest integration path — no SDK, just subprocess.

use a2d_core::observer::ToolEvent;
use a2d_core::provider::{
    InvocationRequest, InvocationResponse, Provider, ProviderError, TokenUsage,
};
use std::process::Command;

/// A provider that invokes a model via a CLI subprocess.
pub struct CliProvider {
    name: String,
    command: String,
    args_builder: Box<dyn Fn(&InvocationRequest) -> Vec<String> + Send + Sync>,
    output_parser: Box<dyn Fn(&str) -> (String, Option<String>) + Send + Sync>,
}

/// Codex features that give the model tools or extra context beyond producing
/// text (codex-cli 0.162: every stable, default-on tool feature; LOG C2-WEB and
/// C2-MCP). Web search is off via the `web_search` config key instead.
pub const CODEX_DISABLED_FEATURES: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "js_repl",
    "standalone_web_search",
    "browser_use",
    "browser_use_external",
    "browser_use_full_cdp_access",
    "browser_annotation_api",
    "computer_use",
    "apps",
    "plugins",
    "remote_plugin",
    "multi_agent",
    "skill_search",
    "skill_mcp_dependency_install",
    "tool_suggest",
    "image_generation",
    "in_app_browser",
    "view_image",
    "goals",
    "hooks",
    // Also default-on in 0.162 and not needed for text-only calls (confirmation-3
    // §0.13, live check 2026-10-19: a PONG call with all of these off emits only
    // thread/turn events and one agent_message). Not `code_mode_host`: disabling it
    // makes codex emit an `error` item at startup on every call.
    "sleep_tool",
    "workspace_dependencies",
    "worktrees",
    "unified_exec_tty",
    "shell_snapshot",
    "realtime_conversation",
    "tool_call_mcp_elicitation",
    "write_stdin_approval",
    "in_app_local_automation",
    "daemon_auto_start",
];

/// `--json` event item types a no-tools Codex call may produce. Anything else
/// (commands, file changes, MCP or web calls, errors, unknown types) counts as a
/// tool event (allowlist, fail closed; plan 2026-10-18-confirmation-3 §0.4).
pub const CODEX_ALLOWED_ITEMS: &[&str] = &["agent_message", "reasoning"];
const CODEX_ALLOWED_EVENTS: &[&str] = &[
    "thread.started",
    "turn.started",
    "turn.completed",
    "item.started",
    "item.updated",
    "item.completed",
];
/// Separates the `--json` event stream from Codex's stderr in a transcript.
pub const CODEX_STDERR_MARKER: &str = "\n--- codex stderr ---\n";
/// How long a killed child's pipes may take to close. A grandchild holding a pipe
/// must not hang the call; then no transcript is captured (fail closed).
const KILLED_PIPE_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

thread_local! {
    static FAILED_CALL_TRANSCRIPT: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// The transcript of this thread's last failed Codex call (timeout, non-zero exit,
/// no final message), taken once. `invoke` returns only an error for such a call, so
/// the caller that records it (the lift meter) takes its partial event stream and
/// stderr here, on the same thread, right after `invoke` (plan
/// 2026-10-18-confirmation-3 §0.13: failed calls are audited like any other).
pub fn take_failed_call_transcript() -> Option<String> {
    FAILED_CALL_TRANSCRIPT.with(|t| t.borrow_mut().take())
}

/// Store a failed call's transcript for `take_failed_call_transcript`.
pub fn set_failed_call_transcript(transcript: String) {
    FAILED_CALL_TRANSCRIPT.with(|t| *t.borrow_mut() = Some(transcript));
}

impl CliProvider {
    /// Create a Codex CLI provider.
    ///
    /// Models: "gpt-5.4" (with reasoning_effort), "gpt-5.4-mini", "gpt-5.3-spark".
    /// Empty string uses account default.
    /// reasoning_effort: none/minimal/low/medium/high/xhigh (only for gpt-5.4).
    pub fn codex(model: &str, reasoning_effort: Option<&str>) -> Self {
        let model = model.to_string();
        let reasoning = reasoning_effort.map(|r| r.to_string());
        let name = if model.is_empty() {
            "codex/default".to_string()
        } else {
            format!("codex/{model}")
        };
        Self {
            name,
            command: "codex".to_string(),
            args_builder: Box::new(move |req: &InvocationRequest| {
                let mut args = vec![
                    "exec".to_string(),
                    format!("{}\n\n{}", req.system, req.prompt),
                ];
                if !model.is_empty() {
                    args.push("-m".to_string());
                    args.push(model.clone());
                }
                if let Some(ref effort) = reasoning {
                    args.push("-c".to_string());
                    args.push(format!("model_reasoning_effort=\"{effort}\""));
                }
                args.extend([
                    "--sandbox".to_string(),
                    "read-only".to_string(),
                    "--skip-git-repo-check".to_string(),
                    "--ephemeral".to_string(),
                    "--ignore-user-config".to_string(),
                    "--ignore-rules".to_string(),
                ]);
                // Artifact-only role, like OpenCode's no-tools agent and Pi's
                // --no-tools: a read-only sandbox still lets the model read
                // any file (hidden tests, holdouts) or search the web.
                // Verified live 2026-10-14: with these disabled, a request to
                // run `ls /` gets no exec and the model reports it cannot.
                for feature in CODEX_DISABLED_FEATURES {
                    args.push("--disable".to_string());
                    args.push(feature.to_string());
                }
                // codex-cli 0.162 turns web search on by default and ignores the
                // deprecated `--disable web_search_request` (verified live
                // 2026-10-17, A²D LOG C2-WEB); the config key turns it off.
                args.push("-c".to_string());
                args.push("web_search=\"disabled\"".to_string());
                // Structured events on stdout (tool detection by allowlist) and the
                // final message in a file the provider reads back.
                args.push("--json".to_string());
                args.push("--output-last-message".to_string());
                args.push(CODEX_LAST_MESSAGE_FILE.to_string());
                args
            }),
            output_parser: Box::new(|output: &str| (output.to_string(), None)),
        }
    }

    /// Create a Gemini CLI provider.
    pub fn gemini(model: &str) -> Self {
        let model = model.to_string();
        Self {
            name: format!("gemini/{model}"),
            command: "gemini".to_string(),
            args_builder: Box::new(move |req: &InvocationRequest| {
                vec![
                    "-p".to_string(),
                    format!("{}\n\n{}", req.system, req.prompt),
                    "--model".to_string(),
                    model.clone(),
                    "--sandbox".to_string(),
                    "-o".to_string(),
                    "text".to_string(),
                ]
            }),
            output_parser: Box::new(|output: &str| (output.to_string(), None)),
        }
    }

    /// Create a Pi CLI provider.
    ///
    /// Pi is the repo-maintenance/coding-agent harness used for the outer
    /// project loop. Run it in non-interactive ephemeral mode and disable tools
    /// for typed artifact-production roles; A²D applies any proposed changes
    /// through its own patchset gates rather than allowing direct provider
    /// filesystem mutation.
    pub fn pi(model: Option<&str>) -> Self {
        let model = model.map(str::to_string);
        let name = model
            .as_ref()
            .map(|model| format!("pi/{model}"))
            .unwrap_or_else(|| "pi/default".to_string());
        Self {
            name,
            command: "pi".to_string(),
            args_builder: Box::new(move |req: &InvocationRequest| {
                let mut args = vec![
                    "--print".to_string(),
                    "--no-session".to_string(),
                    "--no-tools".to_string(),
                    "--no-context-files".to_string(),
                    "--no-extensions".to_string(),
                    "--no-skills".to_string(),
                    "--no-prompt-templates".to_string(),
                    "--no-themes".to_string(),
                    "--system-prompt".to_string(),
                    req.system.clone(),
                ];
                if let Some(model) = &model {
                    args.push("--model".to_string());
                    args.push(model.clone());
                }
                args.push(req.prompt.clone());
                args
            }),
            output_parser: Box::new(|output: &str| (output.to_string(), None)),
        }
    }

    /// Create an OpenCode CLI provider.
    ///
    /// Uses `--format json` to get clean NDJSON output without ANSI codes.
    /// Parses text events from the JSON stream.
    pub fn opencode(model: &str) -> Self {
        let model = model.to_string();
        Self {
            name: format!("opencode/{model}"),
            command: "opencode".to_string(),
            args_builder: Box::new(move |req: &InvocationRequest| {
                vec![
                    "run".to_string(),
                    "--model".to_string(),
                    model.clone(),
                    "--pure".to_string(),
                    "--agent".to_string(),
                    opencode_artifact_agent_name().to_string(),
                    "--format".to_string(),
                    "json".to_string(),
                    format!("SYSTEM: {}\n\nUSER: {}", req.system, req.prompt),
                ]
            }),
            output_parser: Box::new(|output: &str| (parse_opencode_output(output), None)),
        }
    }
}

fn parse_opencode_output(output: &str) -> String {
    // OpenCode's `--format json` emits NDJSON. The event shape has changed
    // across versions: current releases put assistant text at `/part/text`,
    // older examples used top-level `/text`, and tool calls report file writes
    // under `/part/state/input/content`. Collect the useful assistant channels
    // and fall back to written file content when the final message is only a
    // generic acknowledgement like "Done.".
    let mut text = String::new();
    let mut written_contents = Vec::new();

    for line in output.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        if v.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(t) = v.pointer("/part/text").and_then(|t| t.as_str()) {
                text.push_str(t);
            } else if let Some(t) = v.get("text").and_then(|t| t.as_str()) {
                text.push_str(t);
            }
        }

        let part_tool = v.pointer("/part/tool").and_then(|t| t.as_str());
        let top_level_tool = v.get("tool").and_then(|t| t.as_str());
        if (part_tool == Some("write") || top_level_tool == Some("write"))
            && let Some(content) = v
                .pointer("/part/state/input/content")
                .or_else(|| v.pointer("/state/input/content"))
                .and_then(|content| content.as_str())
        {
            written_contents.push(content.to_string());
        }
    }

    let trimmed = text.trim();
    if let Some(last_written) = written_contents.last()
        && (trimmed.is_empty() || is_generic_opencode_completion(trimmed))
    {
        return last_written.clone();
    }

    text
}

fn opencode_artifact_agent_name() -> &'static str {
    "a2d-artifact-no-tools"
}

fn opencode_tool_denial_config() -> String {
    serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "agent": {
            opencode_artifact_agent_name(): {
                "description": "A²D artifact-only provider role. All tools are denied; emit the requested JSON/text artifact in stdout.",
                "permission": {
                    "*": "deny"
                }
            }
        }
    })
    .to_string()
}

fn is_generic_opencode_completion(text: &str) -> bool {
    let normalized = text
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_ascii_lowercase();

    matches!(
        normalized.as_str(),
        "done"
            | "complete"
            | "completed"
            | "created"
            | "file created"
            | "wrote file"
            | "written"
            | "the file has been created"
            | "the file has been written"
    )
}

fn default_timeout_secs(provider_name: &str) -> u64 {
    if provider_name.contains("zai-coding-plan/glm-5.") {
        900
    } else {
        300
    }
}

pub fn provider_no_public_solution_search_env() -> [(&'static str, &'static str); 5] {
    // Policy-observability only: these flags make the benchmark no-public-
    // solution-search contract visible to provider subprocesses. They do not
    // enforce OS/network isolation.
    [
        ("A2D_PROVIDER_POLICY_ENV_SOURCE", "a2d-cli-provider"),
        ("A2D_GITHUB_SOLUTION_SEARCH_ALLOWED", "false"),
        ("A2D_PUBLIC_SOLUTION_SEARCH_FORBIDDEN", "true"),
        (
            "A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED",
            "false",
        ),
        (
            "A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN",
            "true",
        ),
    ]
}

pub fn network_configuration_env_vars() -> [&'static str; 16] {
    // Environment scrubbing is defense-in-depth only. It removes common proxy
    // and package-manager network configuration inherited from the parent
    // process, but it is not OS/network namespace isolation.
    a2d_core::process_env::network_configuration_env_vars()
}

pub fn remove_network_configuration_env(command: &mut Command) {
    a2d_core::process_env::remove_network_configuration_env(command);
}

fn isolated_provider_cwd(command: &str) -> Result<std::path::PathBuf, ProviderError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!(
        "a2d-provider-{command}-{}-{now}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| {
        ProviderError::InvocationFailed(format!(
            "failed to create isolated provider cwd {}: {e}",
            dir.display()
        ))
    })?;
    Ok(dir)
}

fn spawn_pipe_reader<R: std::io::Read + Send + 'static>(
    pipe: Option<R>,
) -> Option<std::thread::JoinHandle<Vec<u8>>> {
    pipe.map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = pipe.read_to_end(&mut buffer);
            buffer
        })
    })
}

fn join_pipe_reader(reader: Option<std::thread::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default()
}

/// Join a pipe reader only if it finishes within `grace`; otherwise leave it running.
fn join_pipe_reader_within(
    reader: Option<std::thread::JoinHandle<Vec<u8>>>,
    grace: std::time::Duration,
) -> Option<Vec<u8>> {
    let handle = reader?;
    let start = std::time::Instant::now();
    while !handle.is_finished() {
        if start.elapsed() > grace {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    handle.join().ok()
}

/// Kill a child spawned with `process_group(0)` together with everything in its group
/// (Codex helpers that would otherwise keep the output pipes open), then the child.
fn kill_group(child: &mut std::process::Child) {
    let _ = Command::new("/bin/kill")
        .args(["-KILL", &format!("-{}", child.id())])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = child.kill();
}

/// A killed call's transcript (event stream, marker, stderr), if both pipes closed in time.
fn killed_call_transcript(
    stdout: Option<std::thread::JoinHandle<Vec<u8>>>,
    stderr: Option<std::thread::JoinHandle<Vec<u8>>>,
    grace: std::time::Duration,
) -> Option<String> {
    let out = join_pipe_reader_within(stdout, grace)?;
    let err = join_pipe_reader_within(stderr, grace)?;
    Some(format!(
        "{}{CODEX_STDERR_MARKER}{}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err)
    ))
}

/// The Codex CLI ends its stderr log with `tokens used` and the total on the
/// next line (e.g. `7,477`).
fn codex_usage(stderr: &str) -> TokenUsage {
    let lines: Vec<&str> = stderr.lines().collect();
    let reported_total = lines
        .iter()
        .rposition(|line| line.trim() == "tokens used")
        .and_then(|index| lines.get(index + 1))
        .and_then(|line| line.trim().replace(',', "").parse().ok());
    TokenUsage {
        reported_total,
        ..TokenUsage::default()
    }
}

/// File (in the provider's isolated cwd) that receives Codex's final message.
pub const CODEX_LAST_MESSAGE_FILE: &str = "last-message.txt";

/// Tool events in a Codex transcript.
///
/// `--json` transcripts (event stream, then `CODEX_STDERR_MARKER`, then stderr):
/// every item whose type is not in `CODEX_ALLOWED_ITEMS`, every event type outside
/// the known lifecycle, every unparsable line, and a missing `turn.completed` each
/// count one (allowlist, fail closed).
///
/// Legacy text transcripts (codex stderr before 2026-10-18): lines `exec`,
/// `web search: …` and `mcp: …` (C2-WEB, C2-MCP).
pub fn codex_tool_calls(transcript: &str) -> usize {
    codex_tool_events(transcript, false)
}

/// Tool events in the transcript of a *failed* call (killed at the timeout, non-zero
/// exit, no final message): the same allowlist, except that a missing `turn.completed`
/// is expected there, not an event, because the call ended before its turn did. An
/// empty event stream has no events; a cut-off last line still counts (fail closed).
pub fn codex_tool_calls_failed(transcript: &str) -> usize {
    codex_tool_events(transcript, true)
}

fn codex_tool_events(transcript: &str, failed: bool) -> usize {
    // The marker, not the first character, selects the --json parser: noise before the
    // first event must not switch to the lenient legacy count (Sol review, 2026-10-19).
    let json = transcript
        .split_once(CODEX_STDERR_MARKER)
        .map(|(events, _)| events)
        .or_else(|| transcript.trim_start().starts_with('{').then_some(transcript));
    if failed && json.is_some_and(|events| events.trim().is_empty()) {
        return 0;
    }
    let Some(events) = json else {
        return transcript
            .lines()
            .filter(|line| {
                line.trim() == "exec" || line.starts_with("web search:") || line.starts_with("mcp: ")
            })
            .count();
    };
    let mut count = 0;
    let mut completed = false;
    for line in events.lines().filter(|l| !l.trim().is_empty()) {
        // Every unparsable line counts, a cut-off last line of a killed call included:
        // it could be a tool event (fail closed).
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            count += 1;
            continue;
        };
        let kind = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if !CODEX_ALLOWED_EVENTS.contains(&kind) {
            count += 1;
            continue;
        }
        if kind == "turn.completed" {
            completed = true;
        }
        if kind.starts_with("item.") {
            let item_type = event
                .get("item")
                .and_then(|i| i.get("type"))
                .and_then(|t| t.as_str());
            if !item_type.is_some_and(|t| CODEX_ALLOWED_ITEMS.contains(&t)) {
                count += 1;
            }
        }
    }
    if !completed && !failed {
        count += 1;
    }
    count
}

/// Token usage from a `--json` transcript's `turn.completed` event.
fn codex_json_usage(events: &str) -> Option<TokenUsage> {
    let usage = events
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|e| e.get("type").and_then(|t| t.as_str()) == Some("turn.completed"))
        .filter_map(|e| e.get("usage").cloned())
        .last()?;
    let get = |k: &str| usage.get(k).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let prompt = get("input_tokens");
    let completion = get("output_tokens");
    Some(TokenUsage {
        prompt_tokens: prompt,
        completion_tokens: completion,
        thinking_tokens: get("reasoning_output_tokens"),
        reported_total: Some(prompt + completion),
    })
}

/// A dedicated `CODEX_HOME` inside the provider's isolated cwd holding only a
/// link to the caller's `auth.json`: no plugins, skills, memories, AGENTS.md,
/// hooks or config from the user's home reach the model (C2-MCP), and the
/// environment cannot drift with the user's codex setup.
/// Where a call's private CODEX_HOME lives: under `root` (`A2D_CODEX_HOME_ROOT`) when set,
/// else inside the call's cwd. Confirmation-3 §0.10 wants CODEX_HOME under $A2D_HOLDOUT_DIR,
/// while the cwd stays under the temp root, whose ancestry holds no codex context.
fn codex_home_in(cwd: &std::path::Path, root: Option<std::ffi::OsString>) -> std::path::PathBuf {
    match root {
        Some(root) if !root.is_empty() => {
            std::path::PathBuf::from(root).join(cwd.file_name().unwrap_or_default())
        }
        _ => cwd.join(".codex_home"),
    }
}

fn codex_home_root() -> Option<std::ffi::OsString> {
    std::env::var_os("A2D_CODEX_HOME_ROOT")
}

/// Remove a call's cwd and, when it lives elsewhere, its private CODEX_HOME.
fn remove_call_dirs(cwd: &std::path::Path) {
    let _ = std::fs::remove_dir_all(cwd);
    let home = codex_home_in(cwd, codex_home_root());
    if !home.starts_with(cwd) {
        let _ = std::fs::remove_dir_all(home);
    }
}

fn isolated_codex_home(cwd: &std::path::Path) -> Result<std::path::PathBuf, ProviderError> {
    let home = codex_home_in(cwd, codex_home_root());
    std::fs::create_dir_all(&home).map_err(|e| {
        ProviderError::InvocationFailed(format!("failed to create isolated CODEX_HOME: {e}"))
    })?;
    let source = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".codex")))
        .map(|d| d.join("auth.json"))
        .ok_or_else(|| ProviderError::InvocationFailed("no codex auth.json".to_string()))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&source, home.join("auth.json")).map_err(|e| {
        ProviderError::InvocationFailed(format!("failed to link codex auth.json: {e}"))
    })?;
    Ok(home)
}

impl Provider for CliProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn invoke(&self, request: &InvocationRequest) -> Result<InvocationResponse, ProviderError> {
        let _ = take_failed_call_transcript();
        let args = (self.args_builder)(request);

        if std::env::var("A2D_TRACE").is_ok_and(|v| !v.is_empty() && v != "0") {
            let total: usize = args.iter().map(|a| a.len()).sum();
            eprintln!(
                "[a2d cli] spawning {} with {} args, total {} bytes",
                self.command,
                args.len(),
                total
            );
            for (i, a) in args.iter().enumerate() {
                let preview: String = a.chars().take(80).collect();
                eprintln!(
                    "[a2d cli]   arg[{}] ({} bytes): {:?}{}",
                    i,
                    a.len(),
                    preview,
                    if a.len() > 80 { "..." } else { "" }
                );
            }
        }

        // CLI coding tools can have filesystem-writing capabilities. Enzymes
        // must communicate artifacts through stdout only; system source changes
        // must go through the architect's SystemPatch + self-sandbox gate. Run
        // each provider in an empty temp cwd so a model cannot mutate the repo
        // directly as a side effect of generation.
        let sandbox_dir = isolated_provider_cwd(&self.command)?;
        if self.command == "opencode" {
            std::fs::write(
                sandbox_dir.join("opencode.json"),
                opencode_tool_denial_config(),
            )
            .map_err(|e| {
                remove_call_dirs(&sandbox_dir);
                ProviderError::InvocationFailed(format!(
                    "failed to write opencode tool-denial config: {e}"
                ))
            })?;
        }

        let mut command = Command::new(&self.command);
        command
            .args(&args)
            .envs(provider_no_public_solution_search_env())
            .current_dir(&sandbox_dir)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped());
        remove_network_configuration_env(&mut command);
        if self.command == "codex" {
            let home = isolated_codex_home(&sandbox_dir).inspect_err(|_| {
                remove_call_dirs(&sandbox_dir);
            })?;
            command.env("CODEX_HOME", home);
            // Its own process group, so a timeout kills Codex's helpers too and the
            // partial transcript can be read (plan 2026-10-18-confirmation-3 §0.13).
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                command.process_group(0);
            }
        }

        let mut child = command.spawn().map_err(|e| {
            remove_call_dirs(&sandbox_dir);
            ProviderError::InvocationFailed(format!(
                "{} not found or failed to start: {e}",
                self.command
            ))
        })?;

        // Drain both pipes while waiting: a child that fills the stderr pipe
        // buffer (Codex echoes the prompt and response there) would otherwise
        // block forever and surface as a timeout.
        let stdout_reader = spawn_pipe_reader(child.stdout.take());
        let stderr_reader = spawn_pipe_reader(child.stderr.take());

        // Provider timeout — no invocation should silently hang.
        // GLM is often slow-but-productive; give it a longer default window.
        // Override with A2D_PROVIDER_TIMEOUT_SECS env var for explicit experiments.
        let timeout_secs = std::env::var("A2D_PROVIDER_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or_else(|| default_timeout_secs(&self.name));
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let start = std::time::Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if start.elapsed() > timeout {
                        if self.command == "codex" {
                            kill_group(&mut child);
                        } else {
                            let _ = child.kill();
                        }
                        let _ = child.wait();
                        if self.command == "codex"
                            && let Some(transcript) =
                                killed_call_transcript(stdout_reader, stderr_reader, KILLED_PIPE_GRACE)
                        {
                            set_failed_call_transcript(transcript);
                        }
                        remove_call_dirs(&sandbox_dir);
                        return Err(ProviderError::InvocationFailed(format!(
                            "{} timed out after {}s",
                            self.command,
                            timeout.as_secs()
                        )));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                Err(e) => {
                    remove_call_dirs(&sandbox_dir);
                    return Err(ProviderError::InvocationFailed(format!(
                        "{} wait failed: {e}",
                        self.command
                    )));
                }
            }
        }

        let status = child.wait().map_err(|e| {
            remove_call_dirs(&sandbox_dir);
            ProviderError::InvocationFailed(format!("{} output read failed: {e}", self.command))
        })?;
        let stdout = join_pipe_reader(stdout_reader);
        let stderr = String::from_utf8_lossy(&join_pipe_reader(stderr_reader)).into_owned();
        let last_message = (self.command == "codex")
            .then(|| std::fs::read_to_string(sandbox_dir.join(CODEX_LAST_MESSAGE_FILE)).ok())
            .flatten();
        remove_call_dirs(&sandbox_dir);

        if !status.success() {
            if self.command == "codex" {
                set_failed_call_transcript(format!(
                    "{}{CODEX_STDERR_MARKER}{stderr}",
                    String::from_utf8_lossy(&stdout)
                ));
            }
            return Err(ProviderError::InvocationFailed(format!(
                "{} exited with {status}: {stderr}",
                self.command
            )));
        }

        let raw = String::from_utf8_lossy(&stdout).to_string();
        // Codex in `--json` mode: stdout is the event stream; the answer is the
        // final message file. A missing file is a failed call, not empty text.
        let (text, thinking) = if self.command == "codex" {
            match &last_message {
                Some(message) => (message.clone(), None),
                None => {
                    set_failed_call_transcript(format!("{raw}{CODEX_STDERR_MARKER}{stderr}"));
                    return Err(ProviderError::InvocationFailed(
                        "codex produced no final message".to_string(),
                    ));
                }
            }
        } else {
            (self.output_parser)(&raw)
        };

        // CLI providers don't give us fine-grained tool events, so the
        // observer sees a minimal constant trace: one Think + one Text.
        let tool_events = vec![ToolEvent::Think, ToolEvent::Text];
        let (usage, transcript) = if self.command == "codex" {
            let usage = codex_json_usage(&raw).unwrap_or_else(|| codex_usage(&stderr));
            (usage, Some(format!("{raw}{CODEX_STDERR_MARKER}{stderr}")))
        } else {
            (TokenUsage::default(), None)
        };

        Ok(InvocationResponse {
            text,
            raw_output: Some(raw),
            tool_events,
            transcript,
            thinking,
            usage,
        })
    }

    fn supports_thinking(&self) -> bool {
        false // CLI output doesn't include thinking blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_usage_reads_reported_total() {
        let stderr = "codex\nPONG\ntokens used\n48,034\n";
        assert_eq!(codex_usage(stderr).reported_total, Some(48_034));
        assert_eq!(codex_usage("no total").reported_total, None);
        assert_eq!(codex_tool_calls("exec\n/bin/zsh -lc ls\nexec\ncat x\n"), 2);
    }

    #[test]
    fn codex_json_transcript_allowlist_counts_every_non_text_item() {
        let clean = "{\"type\":\"thread.started\"}\n{\"type\":\"turn.started\"}\n\
{\"type\":\"item.completed\",\"item\":{\"type\":\"reasoning\"}}\n\
{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"PONG\"}}\n\
{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":2}}\n";
        assert_eq!(codex_tool_calls(&format!("{clean}{CODEX_STDERR_MARKER}exec\nstderr noise")), 0);
        for bad in ["command_execution", "mcp_tool_call", "web_search", "file_change", "error", "todo_list"] {
            let t = format!("{clean}{{\"type\":\"item.completed\",\"item\":{{\"type\":\"{bad}\"}}}}\n");
            assert_eq!(codex_tool_calls(&t), 1, "{bad} must count");
        }
        // unparsable line, unknown event type, item without a type, missing turn.completed
        assert_eq!(codex_tool_calls(&format!("{clean}not json\n")), 1);
        assert_eq!(codex_tool_calls(&format!("{clean}{{\"type\":\"weird\"}}\n")), 1);
        assert_eq!(codex_tool_calls(&format!("{clean}{{\"type\":\"item.started\"}}\n")), 1);
        let truncated = clean.replace("{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":2}}\n", "");
        assert_eq!(codex_tool_calls(&truncated), 1);
        let usage = codex_json_usage(clean).unwrap();
        assert_eq!((usage.prompt_tokens, usage.completion_tokens, usage.reported_total), (10, 2, Some(12)));
    }

    #[test]
    fn failed_call_transcripts_do_not_count_the_cut_off_turn() {
        // plan 2026-10-18-confirmation-3 §0.13: a killed call never completes its turn.
        let partial = "{\"type\":\"thread.started\"}\n{\"type\":\"turn.started\"}\n\
{\"type\":\"item.completed\",\"item\":{\"type\":\"reasoning\"}}\n";
        let t = |events: &str| format!("{events}{CODEX_STDERR_MARKER}stderr noise\n");
        assert_eq!(codex_tool_calls(&t(partial)), 1);
        assert_eq!(codex_tool_calls_failed(&t(partial)), 0);
        // A line cut off by the kill could have been a tool event: it counts. An empty
        // stream has no events.
        assert_eq!(codex_tool_calls_failed(&t(&format!("{partial}{{\"type\":\"item.compl"))), 1);
        assert_eq!(codex_tool_calls_failed(&t("")), 0);
        // Noise before the first event cannot switch to the lenient legacy count.
        let tool = "{\"type\":\"item.started\",\"item\":{\"type\":\"command_execution\"}}\n";
        assert_eq!(codex_tool_calls(&t(&format!("warning: noise\n{partial}{tool}"))), 3);
        // Everything else still counts, as for a completed call.
        let tool = "{\"type\":\"item.started\",\"item\":{\"type\":\"command_execution\"}}\n";
        assert_eq!(codex_tool_calls_failed(&t(&format!("{partial}{tool}"))), 1);
        assert_eq!(codex_tool_calls_failed(&t(&format!("{partial}not json\n{partial}"))), 1);
        assert_eq!(codex_tool_calls_failed(&t(&format!("{partial}{{\"type\":\"weird\"}}\n"))), 1);
    }

    #[test]
    fn a_killed_call_leaves_its_partial_transcript() {
        use std::os::unix::process::CommandExt;
        // `sleep` is a child of `sh` holding the pipes, like a Codex helper: only a
        // group kill closes them.
        let mut child = Command::new("sh")
            .args(["-c", "printf '{\"type\":\"thread.started\"}\\n'; echo noise >&2; sleep 30"])
            .process_group(0)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let out = spawn_pipe_reader(child.stdout.take());
        let err = spawn_pipe_reader(child.stderr.take());
        std::thread::sleep(std::time::Duration::from_secs(1));
        kill_group(&mut child);
        let _ = child.wait();
        let t = killed_call_transcript(out, err, std::time::Duration::from_secs(5)).expect("transcript");
        assert_eq!(t, format!("{{\"type\":\"thread.started\"}}\n{CODEX_STDERR_MARKER}noise\n"));
    }

    #[test]
    fn a_pipe_held_by_a_grandchild_does_not_hang_the_kill() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 30 & printf x; sleep 30"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let out = spawn_pipe_reader(child.stdout.take());
        let err = spawn_pipe_reader(child.stderr.take());
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = child.kill();
        let _ = child.wait();
        let start = std::time::Instant::now();
        assert!(killed_call_transcript(out, err, std::time::Duration::from_millis(500)).is_none());
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn codex_legacy_text_transcripts_count_mcp_calls() {
        assert_eq!(codex_tool_calls("codex\nmcp: codex/list_mcp_resources started\nPONG\n"), 1);
    }

    #[test]
    fn codex_args_use_json_events_and_disable_every_tool_feature() {
        let provider = CliProvider::codex("gpt-6-luna", Some("low"));
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("coder"),
            system: "s".to_string(),
            prompt: "p".to_string(),
            max_tokens: 10,
        };
        let args = (provider.args_builder)(&request);
        assert!(args.contains(&"--json".to_string()));
        assert!(args.windows(2).any(|w| w[0] == "--output-last-message" && w[1] == CODEX_LAST_MESSAGE_FILE));
        for feature in ["plugins", "remote_plugin", "multi_agent", "skill_search", "shell_tool", "apps"] {
            assert!(args.windows(2).any(|w| w[0] == "--disable" && w[1] == feature), "{feature}");
        }
        assert!(!args.iter().any(|a| a == "web_search_request"));
    }

    /// Live: one real call per lane (Luna coder, Sol judge) through the provider: the
    /// negative control for the positive controls below (run with `--ignored`).
    #[test]
    #[ignore]
    fn live_codex_call_returns_text_with_zero_tool_events() {
        let lanes = std::env::var("A2D_CONTROL_LANES")
            .unwrap_or_else(|_| "codex/gpt-6-luna:low,codex/gpt-6.1-sol:medium".to_string());
        for spec in lanes.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (model, effort) = spec.trim_start_matches("codex/").split_once(':').unwrap_or((spec, "low"));
            let provider = CliProvider::codex(model, Some(effort));
            let request = InvocationRequest {
                enzyme_id: a2d_core::types::EnzymeId::from("coder"),
                system: "Reply with exactly one word.".to_string(),
                prompt: "Reply with the single word PONG.".to_string(),
                max_tokens: 10,
            };
            let response = provider.invoke(&request).expect("live codex call");
            assert_eq!(response.text.trim(), "PONG", "{model}");
            let transcript = response.transcript.expect("transcript");
            assert_eq!(codex_tool_calls(&transcript), 0, "{model}: {transcript}");
            assert!(response.usage.total() > 0);
        }
    }

    /// Live: a real Luna call killed at a short timeout leaves its partial transcript,
    /// with no tool events (run alone: `cargo test -p a2d-providers live_codex_timeout -- --ignored`;
    /// it sets the process-wide timeout variable).
    #[test]
    #[ignore]
    fn live_codex_timeout_leaves_a_partial_transcript() {
        // SAFETY: run on its own (see above); no other thread reads the environment.
        unsafe { std::env::set_var("A2D_PROVIDER_TIMEOUT_SECS", "12") };
        let provider = CliProvider::codex("gpt-6-luna", Some("medium"));
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("coder"),
            system: "You are a careful writer.".to_string(),
            prompt: "Write a 4000-word essay on the history of double-entry bookkeeping.".to_string(),
            max_tokens: 10,
        };
        let error = provider.invoke(&request).expect_err("must time out");
        assert!(error.to_string().contains("timed out"), "{error}");
        // Usually `thread.started` and more; a cold start can be killed before the first
        // event (seen 1 in 3 runs on 2026-10-19), which is still a transcript.
        let transcript = take_failed_call_transcript().expect("partial transcript");
        assert!(transcript.contains(CODEX_STDERR_MARKER));
        assert_eq!(codex_tool_calls_failed(&transcript), 0, "{transcript}");
    }

    /// Live positive controls (confirmation-3 §0.4, step 3; Sol reviews 2026-10-19). Inventory of
    /// `codex exec --json` item types in 0.162 (from the binary): agent_message and reasoning
    /// (allowed), command_execution, file_change, mcp_tool_call, web_search, todo_list,
    /// collab_tool_call, error. On every lane in A2D_CONTROL_LANES (default: Luna low = coder,
    /// Sol medium = E002's judge), with one capability switched back on, a call asked to use it
    /// must SUCCEED and its transcript must contain that item type, counted by
    /// `codex_tool_calls`. `error` is produced by disabling `code_mode_host`. `todo_list` could
    /// not be triggered in exec mode even with every feature on (live, 2026-10-19), so the
    /// allowlist alone covers it. The negative control is
    /// `live_codex_call_returns_text_with_zero_tool_events`.
    /// Run alone: `cargo test -p a2d-providers live_codex_positive_controls -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_codex_positive_controls_count_each_tool_kind() {
        let mcp = format!("{}/tests/fixtures/mcp_echo.py", env!("CARGO_MANIFEST_DIR"));
        let lanes = std::env::var("A2D_CONTROL_LANES")
            .unwrap_or_else(|_| "codex/gpt-6-luna:low,codex/gpt-6.1-sol:medium".to_string());
        // (kind, features switched back on, extra args, expected item, prompt)
        let kinds: Vec<(&str, Vec<&str>, Vec<String>, &str, &str)> = vec![
            ("shell", vec!["shell_tool", "unified_exec"], vec![], "command_execution",
             "Run the shell command `echo probe-ok` and then reply with its output."),
            ("web", vec![], vec!["-c".into(), "web_search=\"live\"".into()], "web_search",
             "Use web search to find the URL of the Rust programming language's official website, then reply with it."),
            ("file", vec!["shell_tool", "unified_exec"], vec!["--sandbox".into(), "workspace-write".into()], "file_change",
             "Create a file named probe.txt containing the single line ok, using your file-editing (apply_patch) tool, then reply done."),
            ("mcp", vec![], vec!["-c".into(), "mcp_servers.probe.command=\"python3\"".into(),
                                  "-c".into(), format!("mcp_servers.probe.args=[\"-I\", \"{mcp}\"]")], "mcp_tool_call",
             "Call the MCP tool `echo` of the `probe` server with text probe-ok, then reply with its output."),
            ("collab", vec!["multi_agent"], vec![], "collab_tool_call",
             "Spawn one sub-agent to compute 2+2 and report its answer, then reply with it."),
            ("error", vec![], vec!["--disable".into(), "code_mode_host".into()], "error",
             "Reply with the single word PONG."),
        ];
        for spec in lanes.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (model, effort) = spec.trim_start_matches("codex/").split_once(':').unwrap_or((spec, "low"));
            for (kind, features, extra, item, prompt) in &kinds {
                let base = CliProvider::codex(model, Some(effort));
                let (features, extra) = (features.clone(), extra.clone());
                let provider = CliProvider {
                    name: base.name.clone(),
                    command: base.command.clone(),
                    output_parser: Box::new(|output: &str| (output.to_string(), None)),
                    args_builder: Box::new(move |req: &InvocationRequest| {
                        let mut args = (base.args_builder)(req);
                        let mut i = 0;
                        while i + 1 < args.len() {
                            if args[i] == "--disable" && features.contains(&args[i + 1].as_str()) {
                                args.drain(i..i + 2);
                            } else if args[i] == "--sandbox" && extra.contains(&"--sandbox".to_string()) {
                                args.drain(i..i + 2);
                            } else if args[i] == "-c" && args[i + 1] == "web_search=\"disabled\""
                                && extra.iter().any(|a| a.starts_with("web_search=")) {
                                args.drain(i..i + 2);
                            } else {
                                i += 1;
                            }
                        }
                        let at = args.iter().position(|a| a == "--json").unwrap_or(args.len());
                        for (k, a) in extra.iter().enumerate() {
                            args.insert(at + k, a.clone());
                        }
                        args
                    }),
                };
                let request = InvocationRequest {
                    enzyme_id: a2d_core::types::EnzymeId::from("coder"),
                    system: String::new(),
                    prompt: prompt.to_string(),
                    max_tokens: 10,
                };
                let response = provider
                    .invoke(&request)
                    .unwrap_or_else(|e| panic!("{model} {kind}: the control call failed: {e}"));
                let transcript = response.transcript.expect("transcript");
                let marker = format!("\"type\":\"{item}\"");
                let n = codex_tool_calls(&transcript);
                eprintln!("positive control {model}:{effort} {kind}: {item} present {}, {n} tool events",
                          transcript.contains(&marker));
                assert!(transcript.contains(&marker), "{model} {kind}: no {item} item");
                assert!(n >= 1, "{model} {kind}: the detector saw no tool event");
            }
        }
    }

    #[test]
    fn codex_home_goes_under_the_root_when_one_is_set() {
        let cwd = std::path::Path::new("/tmp/a2d-provider-codex-1-2");
        assert_eq!(codex_home_in(cwd, None), cwd.join(".codex_home"));
        assert_eq!(codex_home_in(cwd, Some("".into())), cwd.join(".codex_home"));
        assert_eq!(
            codex_home_in(cwd, Some("/h/root".into())),
            std::path::PathBuf::from("/h/root/a2d-provider-codex-1-2")
        );
    }

    #[test]
    fn isolated_codex_home_holds_only_an_auth_link() {
        let dir = std::env::temp_dir().join(format!("a2d-codex-home-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let home = isolated_codex_home(&dir).unwrap();
        let entries: Vec<_> = std::fs::read_dir(&home).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("auth.json")]);
        assert!(std::fs::symlink_metadata(home.join("auth.json")).unwrap().file_type().is_symlink());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn codex_tool_calls_counts_web_searches_but_not_the_deprecation_notice() {
        // codex-cli 0.162 prints a search as `web search: <query>`, never as `exec`.
        let stderr = "deprecated: `[features].web_search_request` is deprecated because web search is enabled by default.\n\
                      web search: \nweb search: site:blog.rust-lang.org release\ncodex\nanswer\nexec\nls\n";
        assert_eq!(codex_tool_calls(stderr), 3);
    }

    #[test]
    fn large_stderr_output_does_not_block_the_provider() {
        let provider = CliProvider {
            name: "test/noisy".to_string(),
            command: "sh".to_string(),
            args_builder: Box::new(|_| {
                vec![
                    "-c".to_string(),
                    "head -c 300000 /dev/zero | tr '\\0' x >&2; echo done".to_string(),
                ]
            }),
            output_parser: Box::new(|output: &str| (output.to_string(), None)),
        };
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("coder"),
            system: String::new(),
            prompt: String::new(),
            max_tokens: 10,
        };
        let start = std::time::Instant::now();
        let response = provider
            .invoke(&request)
            .expect("noisy provider should succeed");
        assert_eq!(response.text.trim(), "done");
        assert!(start.elapsed() < std::time::Duration::from_secs(30));
    }

    fn provider_args(provider: &CliProvider) -> Vec<String> {
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("coder"),
            system: "system".to_string(),
            prompt: "prompt".to_string(),
            max_tokens: 100,
        };
        (provider.args_builder)(&request)
    }

    #[test]
    fn codex_provider_uses_read_only_ephemeral_artifact_sandbox() {
        let provider = CliProvider::codex("gpt-5.4", Some("low"));
        let args = provider_args(&provider);

        let sandbox_arg = args
            .iter()
            .position(|arg| arg == "--sandbox")
            .expect("codex args should set an explicit sandbox");
        assert_eq!(
            args.get(sandbox_arg + 1).map(String::as_str),
            Some("read-only")
        );
        assert!(args.contains(&"--ephemeral".to_string()));
        assert!(args.contains(&"--skip-git-repo-check".to_string()));
        assert!(args.contains(&"--ignore-user-config".to_string()));
        assert!(args.contains(&"--ignore-rules".to_string()));
    }

    #[test]
    fn codex_provider_disables_shell_and_web_tools() {
        let provider = CliProvider::codex("gpt-6-luna", Some("low"));
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("coder"),
            system: String::new(),
            prompt: String::new(),
            max_tokens: 10,
        };
        let args = (provider.args_builder)(&request);
        for feature in ["shell_tool", "unified_exec"] {
            assert!(
                args.windows(2)
                    .any(|w| w[0] == "--disable" && w[1] == feature),
                "{feature} not disabled: {args:?}"
            );
        }
        // codex-cli 0.162 turns web search on by default and ignores the deprecated
        // `--disable web_search_request`; only the config key turns it off (C2-WEB).
        assert!(
            args.windows(2)
                .any(|w| w[0] == "-c" && w[1] == "web_search=\"disabled\""),
            "web search not disabled: {args:?}"
        );
        assert!(!args.iter().any(|a| a == "web_search_request"), "{args:?}");
    }

    #[test]
    fn codex_provider_does_not_request_full_auto_or_bypass_sandbox() {
        let provider = CliProvider::codex("gpt-5.4", Some("low"));
        let args = provider_args(&provider);

        assert!(!args.contains(&"--full-auto".to_string()));
        assert!(!args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert!(!args.contains(&"--dangerously-bypass-hook-trust".to_string()));
        assert!(!args.iter().any(|arg| arg == "--add-dir"));
    }

    #[test]
    fn glm_gets_longer_default_timeout() {
        assert_eq!(
            default_timeout_secs("opencode/zai-coding-plan/glm-5.1"),
            900
        );
        assert_eq!(
            default_timeout_secs("opencode/zai-coding-plan/glm-5.2"),
            900
        );
    }

    #[test]
    fn non_glm_providers_keep_shorter_default_timeout() {
        assert_eq!(default_timeout_secs("opencode/kimi-for-coding/k2p5"), 300);
        assert_eq!(default_timeout_secs("gemini/gemini-3-pro"), 300);
        assert_eq!(default_timeout_secs("pi/default"), 300);
    }

    #[test]
    fn cli_providers_export_no_public_solution_search_policy_env() {
        let env = provider_no_public_solution_search_env()
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();

        assert_eq!(
            env.get("A2D_PROVIDER_POLICY_ENV_SOURCE"),
            Some(&"a2d-cli-provider")
        );
        assert_eq!(
            env.get("A2D_GITHUB_SOLUTION_SEARCH_ALLOWED"),
            Some(&"false")
        );
        assert_eq!(
            env.get("A2D_PUBLIC_SOLUTION_SEARCH_FORBIDDEN"),
            Some(&"true")
        );
        assert_eq!(
            env.get("A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED"),
            Some(&"false")
        );
        assert_eq!(
            env.get("A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN"),
            Some(&"true")
        );
    }

    #[test]
    fn cli_provider_subprocess_receives_no_public_solution_search_policy_env() {
        let provider = CliProvider {
            name: "test/env-provider".to_string(),
            command: "sh".to_string(),
            args_builder: Box::new(|_req| {
                vec![
                    "-c".to_string(),
                    "printf '%s|%s|%s|%s|%s' \"$A2D_PROVIDER_POLICY_ENV_SOURCE\" \"$A2D_GITHUB_SOLUTION_SEARCH_ALLOWED\" \"$A2D_PUBLIC_SOLUTION_SEARCH_FORBIDDEN\" \"$A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED\" \"$A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN\"".to_string(),
                ]
            }),
            output_parser: Box::new(|output| (output.to_string(), None)),
        };
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("coder"),
            system: "system".to_string(),
            prompt: "prompt".to_string(),
            max_tokens: 100,
        };

        let response = provider.invoke(&request).expect("provider env probe");
        assert_eq!(response.text, "a2d-cli-provider|false|true|false|true");
    }

    #[test]
    fn remove_network_configuration_env_drops_explicit_network_env() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("env");
        for key in network_configuration_env_vars() {
            command.env(key, "http://example.invalid:9");
        }
        command.env("A2D_ENV_SCRUB_SENTINEL", "present");
        remove_network_configuration_env(&mut command);

        let output = command.output().expect("env scrub probe should run");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        for key in network_configuration_env_vars() {
            assert!(
                !stdout
                    .lines()
                    .any(|line| line.starts_with(&format!("{key}="))),
                "network configuration env var {key} leaked to subprocess: {stdout}"
            );
        }
        assert!(
            stdout
                .lines()
                .any(|line| line == "A2D_ENV_SCRUB_SENTINEL=present")
        );
    }

    #[test]
    fn network_env_scrub_preserves_no_public_solution_search_policy_env() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("env");
        for key in network_configuration_env_vars() {
            command.env(key, "http://example.invalid:9");
        }
        command.envs(provider_no_public_solution_search_env());
        remove_network_configuration_env(&mut command);

        let output = command
            .output()
            .expect("policy env plus network scrub probe should run");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        for key in network_configuration_env_vars() {
            assert!(
                !stdout
                    .lines()
                    .any(|line| line.starts_with(&format!("{key}="))),
                "network configuration env var {key} leaked to subprocess: {stdout}"
            );
        }
        for (key, value) in provider_no_public_solution_search_env() {
            assert!(
                stdout.lines().any(|line| line == format!("{key}={value}")),
                "no-public-solution-search policy env {key}={value} missing after network scrub: {stdout}"
            );
        }
    }

    #[test]
    fn pi_provider_is_ephemeral_no_tools_artifact_mode() {
        let provider = CliProvider::pi(None);
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("maintainer"),
            system: "system".to_string(),
            prompt: "prompt".to_string(),
            max_tokens: 100,
        };

        assert_eq!(provider.name(), "pi/default");
        let args = (provider.args_builder)(&request);
        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&"--no-session".to_string()));
        assert!(args.contains(&"--no-tools".to_string()));
        assert!(args.contains(&"--system-prompt".to_string()));
    }

    #[test]
    fn opencode_provider_uses_pure_mode_for_artifact_invocations() {
        let provider = CliProvider::opencode("test/provider");
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("architect"),
            system: "system".to_string(),
            prompt: "prompt".to_string(),
            max_tokens: 100,
        };

        let args = (provider.args_builder)(&request);
        assert!(args.contains(&"--pure".to_string()));
    }

    #[test]
    fn opencode_provider_selects_tool_denied_artifact_agent() {
        let provider = CliProvider::opencode("test/provider");
        let request = InvocationRequest {
            enzyme_id: a2d_core::types::EnzymeId::from("architect"),
            system: "system".to_string(),
            prompt: "prompt".to_string(),
            max_tokens: 100,
        };

        let args = (provider.args_builder)(&request);
        let agent_arg = args
            .iter()
            .position(|arg| arg == "--agent")
            .expect("opencode args should select the artifact agent");
        assert_eq!(
            args.get(agent_arg + 1).map(String::as_str),
            Some(opencode_artifact_agent_name())
        );
    }

    #[test]
    fn opencode_tool_denial_config_denies_all_tools_for_artifact_agent() {
        let config: serde_json::Value =
            serde_json::from_str(&opencode_tool_denial_config()).unwrap();
        assert_eq!(
            config
                .pointer("/agent/a2d-artifact-no-tools/permission/*")
                .and_then(|value| value.as_str()),
            Some("deny")
        );
    }

    #[test]
    fn opencode_parser_extracts_current_part_text_shape() {
        let ndjson = r#"{"type":"step_start","part":{"type":"step-start"}}
{"type":"text","part":{"type":"text","text":"{\"output\":\"ok\"}"}}
"#;

        assert_eq!(parse_opencode_output(ndjson), r#"{"output":"ok"}"#);
    }

    #[test]
    fn opencode_parser_extracts_legacy_top_level_text_shape() {
        let ndjson = r#"{"type":"text","text":"legacy text"}
"#;

        assert_eq!(parse_opencode_output(ndjson), "legacy text");
    }

    #[test]
    fn opencode_parser_uses_write_content_when_final_text_is_done() {
        let ndjson = r#"{"type":"tool_use","part":{"type":"tool","tool":"write","state":{"status":"completed","input":{"filePath":"/tmp/answer.txt","content":"{\"output\":\"ok\"}"}}}}
{"type":"text","part":{"type":"text","text":"Done."}}
"#;

        assert_eq!(parse_opencode_output(ndjson), r#"{"output":"ok"}"#);
    }

    #[test]
    fn opencode_parser_prefers_substantive_final_text_over_write_content() {
        let ndjson = r#"{"type":"tool_use","part":{"type":"tool","tool":"write","state":{"status":"completed","input":{"filePath":"/tmp/scratch.txt","content":"scratch"}}}}
{"type":"text","part":{"type":"text","text":"{\"output\":\"final\"}"}}
"#;

        assert_eq!(parse_opencode_output(ndjson), r#"{"output":"final"}"#);
    }
}
