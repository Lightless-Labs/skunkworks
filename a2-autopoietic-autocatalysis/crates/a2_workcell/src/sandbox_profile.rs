use a2_core::protocol::NetworkPolicy;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};

const BLOCKED_PUBLIC_SOLUTION_HOSTS: [&str; 3] =
    ["github.com", "githubusercontent.com", "github.io"];
const SANDBOX_EXEC_PROGRAM: &str = "/usr/bin/sandbox-exec";

/// Stops `pi` from loading the operator's ambient settings packages, skills, and prompt
/// templates. Settings packages such as Foundry register extensions that spawn child
/// agents; A2-launched pi must not inherit them. Context files (AGENTS.md) stay enabled
/// because they are part of the task worktree.
pub const PI_AMBIENT_RESOURCE_ISOLATION_ARGS: [&str; 3] =
    ["--no-extensions", "--no-skills", "--no-prompt-templates"];

/// Builds the argv for every A2-owned `codex exec` launch.
///
/// Inside an A2 Seatbelt profile (`outer_sandboxed`), Codex's own sandbox is bypassed: Seatbelt
/// profiles cannot nest, and the outer profile already confines the whole process tree. Without
/// an outer sandbox, Codex keeps its own workspace-write sandbox.
pub fn codex_provider_args(
    model_id: &str,
    prompt: &str,
    outer_sandboxed: bool,
    reasoning_effort: &str,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["exec", prompt, "-m", model_id, "-c"]
        .into_iter()
        .map(OsString::from)
        .collect();
    args.push(OsString::from(format!(
        "model_reasoning_effort=\"{reasoning_effort}\""
    )));
    args.push(OsString::from(if outer_sandboxed {
        "--dangerously-bypass-approvals-and-sandbox"
    } else {
        "--approve-for-me"
    }));
    args.extend(["--ephemeral", "--skip-git-repo-check", "--json"].map(OsString::from));
    args
}

/// Environment variable selecting the reasoning effort for A2-launched Codex.
pub const CODEX_REASONING_EFFORT_ENV: &str = "A2_CODEX_REASONING_EFFORT";
const CODEX_REASONING_EFFORTS: [&str; 7] =
    ["none", "minimal", "low", "medium", "high", "xhigh", "max"];

/// Validates an `A2_CODEX_REASONING_EFFORT` value; unset means `high`.
pub fn codex_reasoning_effort_from_env_value(value: Option<&str>) -> Result<&str, String> {
    match value {
        None => Ok("high"),
        Some(level) if CODEX_REASONING_EFFORTS.contains(&level) => Ok(level),
        Some(other) => Err(format!(
            "{CODEX_REASONING_EFFORT_ENV}={other:?} is not one of {CODEX_REASONING_EFFORTS:?}"
        )),
    }
}

/// Builds the argv for every A2-owned `pi` provider launch.
pub fn pi_provider_args(model_id: &str, system: Option<&str>, prompt: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--model",
        model_id,
        "--no-session",
        "--mode",
        "json",
        "--print",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.extend(
        PI_AMBIENT_RESOURCE_ISOLATION_ARGS
            .iter()
            .map(OsString::from),
    );
    if let Some(system) = system {
        args.push(OsString::from("--append-system-prompt"));
        args.push(OsString::from(system));
    }
    args.push(OsString::from(prompt));
    args
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxProfile {
    pub engine: &'static str,
    pub profile_lines: Vec<String>,
    pub profile_sha256: String,
}

impl SandboxProfile {
    pub fn text(&self) -> String {
        let mut text = self.profile_lines.join("\n");
        text.push('\n');
        text
    }
}

pub fn sandbox_profile_for_network_policy(
    policy: &NetworkPolicy,
) -> Result<SandboxProfile, String> {
    sandbox_profile_for_network_policy_with_egress_proxy(policy, None)
}

pub fn sandbox_profile_for_network_policy_with_egress_proxy(
    policy: &NetworkPolicy,
    egress_proxy_port: Option<u16>,
) -> Result<SandboxProfile, String> {
    sandbox_profile_for_network_policy_with_denied_paths_and_egress_proxy(
        policy,
        &[],
        egress_proxy_port,
    )
}

pub fn sandbox_profile_for_network_policy_with_denied_paths(
    policy: &NetworkPolicy,
    denied_paths: &[PathBuf],
) -> Result<SandboxProfile, String> {
    sandbox_profile_for_network_policy_with_denied_paths_and_egress_proxy(
        policy,
        denied_paths,
        None,
    )
}

pub fn sandbox_profile_for_network_policy_with_denied_paths_and_egress_proxy(
    policy: &NetworkPolicy,
    denied_paths: &[PathBuf],
    egress_proxy_port: Option<u16>,
) -> Result<SandboxProfile, String> {
    let mut profile = match policy {
        NetworkPolicy::Open => {
            if egress_proxy_port.is_some() {
                return Err("network_policy=Open cannot use an egress proxy port".into());
            }
            return Err(
                "network_policy=Open does not need a sandbox profile; unrestricted provider launch must remain explicit"
                    .into(),
            );
        }
        NetworkPolicy::Isolated => {
            if egress_proxy_port.is_some() {
                return Err("network_policy=Isolated cannot use an egress proxy port".into());
            }
            profile_from_lines(vec![
                "(version 1)".into(),
                "(allow default)".into(),
                "(deny network*)".into(),
            ])
        }
        NetworkPolicy::AllowList(endpoints) => {
            let port = egress_proxy_port.ok_or_else(|| {
                "Seatbelt cannot filter hostnames; an egress proxy is required for network_policy=AllowList"
                    .to_string()
            })?;
            provider_allowlist_profile(endpoints, port)?
        }
    };

    if denied_paths.is_empty() {
        return Ok(profile);
    }

    let deny_lines = denied_paths
        .iter()
        .map(|path| {
            let path = path
                .to_str()
                .ok_or_else(|| "deny-read path must be valid UTF-8".to_string())?;
            if path.contains('"') || path.contains('\\') {
                return Err("deny-read path must not contain `\"` or `\\`".into());
            }
            Ok(format!(
                "(deny file-read* file-write* (subpath \"{path}\"))"
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    profile.profile_lines.splice(2..2, deny_lines);
    Ok(profile_from_lines(profile.profile_lines))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxExecCommandMaterialization {
    pub profile: SandboxProfile,
    pub profile_path: PathBuf,
    pub program: OsString,
    pub args: Vec<OsString>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderCommandMaterialization {
    pub profile: Option<SandboxProfile>,
    pub profile_path: Option<PathBuf>,
    pub program: OsString,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub sandboxed: bool,
}

pub fn materialize_provider_command_for_network_policy<I, S>(
    policy: Option<&NetworkPolicy>,
    profile_dir: &Path,
    child_program: impl Into<OsString>,
    child_args: I,
) -> Result<ProviderCommandMaterialization, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    materialize_provider_command_for_network_policy_with_denied_paths_and_egress_proxy(
        policy,
        None,
        profile_dir,
        child_program,
        child_args,
        &[],
    )
}

pub fn materialize_provider_command_for_network_policy_with_egress_proxy<I, S>(
    policy: Option<&NetworkPolicy>,
    egress_proxy_port: Option<u16>,
    profile_dir: &Path,
    child_program: impl Into<OsString>,
    child_args: I,
) -> Result<ProviderCommandMaterialization, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    materialize_provider_command_for_network_policy_with_denied_paths_and_egress_proxy(
        policy,
        egress_proxy_port,
        profile_dir,
        child_program,
        child_args,
        &[],
    )
}

pub fn materialize_provider_command_for_network_policy_with_denied_paths<I, S>(
    policy: Option<&NetworkPolicy>,
    profile_dir: &Path,
    child_program: impl Into<OsString>,
    child_args: I,
    denied_paths: &[PathBuf],
) -> Result<ProviderCommandMaterialization, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    materialize_provider_command_for_network_policy_with_denied_paths_and_egress_proxy(
        policy,
        None,
        profile_dir,
        child_program,
        child_args,
        denied_paths,
    )
}

pub fn materialize_provider_command_for_network_policy_with_denied_paths_and_egress_proxy<I, S>(
    policy: Option<&NetworkPolicy>,
    egress_proxy_port: Option<u16>,
    profile_dir: &Path,
    child_program: impl Into<OsString>,
    child_args: I,
    denied_paths: &[PathBuf],
) -> Result<ProviderCommandMaterialization, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match policy {
        Some(restricted @ (NetworkPolicy::Isolated | NetworkPolicy::AllowList(_))) => {
            let materialized = materialize_sandbox_exec_command_with_denied_paths_and_egress_proxy(
                restricted,
                egress_proxy_port,
                profile_dir,
                child_program,
                child_args,
                denied_paths,
            )?;
            Ok(ProviderCommandMaterialization {
                profile: Some(materialized.profile),
                profile_path: Some(materialized.profile_path),
                program: materialized.program,
                args: materialized.args,
                env: provider_proxy_environment(restricted, egress_proxy_port)?,
                sandboxed: true,
            })
        }
        Some(NetworkPolicy::Open) | None => {
            if egress_proxy_port.is_some() {
                return Err(
                    "egress proxy port requires network_policy=AllowList; open provider launches do not use it"
                        .into(),
                );
            }
            if !denied_paths.is_empty() {
                return Err(
                    "deny-read paths require an isolated or allowlist network policy sandbox"
                        .into(),
                );
            }
            Ok(ProviderCommandMaterialization {
                profile: None,
                profile_path: None,
                program: child_program.into(),
                args: child_args.into_iter().map(Into::into).collect(),
                env: vec![],
                sandboxed: false,
            })
        }
    }
}

pub fn materialize_sandbox_exec_command<I, S>(
    policy: &NetworkPolicy,
    profile_dir: &Path,
    child_program: impl Into<OsString>,
    child_args: I,
) -> Result<SandboxExecCommandMaterialization, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    materialize_sandbox_exec_command_with_denied_paths_and_egress_proxy(
        policy,
        None,
        profile_dir,
        child_program,
        child_args,
        &[],
    )
}

pub fn materialize_sandbox_exec_command_with_egress_proxy<I, S>(
    policy: &NetworkPolicy,
    egress_proxy_port: Option<u16>,
    profile_dir: &Path,
    child_program: impl Into<OsString>,
    child_args: I,
) -> Result<SandboxExecCommandMaterialization, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    materialize_sandbox_exec_command_with_denied_paths_and_egress_proxy(
        policy,
        egress_proxy_port,
        profile_dir,
        child_program,
        child_args,
        &[],
    )
}

pub fn materialize_sandbox_exec_command_with_denied_paths<I, S>(
    policy: &NetworkPolicy,
    profile_dir: &Path,
    child_program: impl Into<OsString>,
    child_args: I,
    denied_paths: &[PathBuf],
) -> Result<SandboxExecCommandMaterialization, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    materialize_sandbox_exec_command_with_denied_paths_and_egress_proxy(
        policy,
        None,
        profile_dir,
        child_program,
        child_args,
        denied_paths,
    )
}

pub fn materialize_sandbox_exec_command_with_denied_paths_and_egress_proxy<I, S>(
    policy: &NetworkPolicy,
    egress_proxy_port: Option<u16>,
    profile_dir: &Path,
    child_program: impl Into<OsString>,
    child_args: I,
    denied_paths: &[PathBuf],
) -> Result<SandboxExecCommandMaterialization, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let profile = sandbox_profile_for_network_policy_with_denied_paths_and_egress_proxy(
        policy,
        denied_paths,
        egress_proxy_port,
    )?;
    let profile_dir = create_and_canonicalize_profile_dir(profile_dir)?;
    let profile_path = profile_dir.join(format!("a2-sandbox-{}.sb", profile.profile_sha256));
    write_profile_without_following_symlinks(&profile_path, &profile.text())?;

    let mut args = vec![OsString::from("-f"), profile_path.clone().into_os_string()];
    args.push(child_program.into());
    args.extend(child_args.into_iter().map(Into::into));

    Ok(SandboxExecCommandMaterialization {
        profile,
        profile_path,
        program: OsString::from(SANDBOX_EXEC_PROGRAM),
        args,
    })
}

fn create_and_canonicalize_profile_dir(path: &Path) -> Result<PathBuf, String> {
    reject_profile_dir_symlink(path)?;
    std::fs::create_dir_all(path)
        .map_err(|error| format!("create sandbox profile directory: {error}"))?;
    let direct_metadata = profile_dir_metadata_no_final_symlink(path)?;
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("canonicalize sandbox profile directory: {error}"))?;
    verify_same_profile_dir_after_canonicalize(&direct_metadata, &canonical)?;
    Ok(canonical)
}

fn reject_profile_dir_symlink(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("refusing sandbox profile directory symlink".into())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect sandbox profile directory: {error}")),
    }
}

fn profile_dir_metadata_no_final_symlink(path: &Path) -> Result<std::fs::Metadata, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("inspect sandbox profile directory: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("refusing sandbox profile directory symlink".into());
    }
    if !metadata.is_dir() {
        return Err("sandbox profile path exists but is not a directory".into());
    }
    Ok(metadata)
}

#[cfg(unix)]
fn verify_same_profile_dir_after_canonicalize(
    direct_metadata: &std::fs::Metadata,
    canonical: &Path,
) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;

    let canonical_metadata = std::fs::metadata(canonical)
        .map_err(|error| format!("inspect canonical sandbox profile directory: {error}"))?;
    if direct_metadata.dev() != canonical_metadata.dev()
        || direct_metadata.ino() != canonical_metadata.ino()
    {
        return Err("sandbox profile directory changed while canonicalizing".into());
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_same_profile_dir_after_canonicalize(
    _direct_metadata: &std::fs::Metadata,
    canonical: &Path,
) -> Result<(), String> {
    let canonical_metadata = std::fs::metadata(canonical)
        .map_err(|error| format!("inspect canonical sandbox profile directory: {error}"))?;
    if !canonical_metadata.is_dir() {
        return Err("canonical sandbox profile path is not a directory".into());
    }
    Ok(())
}

fn write_profile_without_following_symlinks(path: &Path, text: &str) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("refusing to write sandbox profile through symlink".into());
        }
        Ok(metadata) if metadata.is_file() => {
            let mut file = open_existing_profile_no_follow(path)?;
            let mut existing = String::new();
            file.read_to_string(&mut existing)
                .map_err(|error| format!("read existing sandbox profile: {error}"))?;
            if existing == text {
                return Ok(());
            }
            return Err("existing sandbox profile path has different contents".into());
        }
        Ok(_) => return Err("sandbox profile path exists but is not a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("inspect sandbox profile path: {error}")),
    }

    let mut file = create_new_profile_no_follow(path)?;
    file.write_all(text.as_bytes())
        .map_err(|error| format!("write sandbox profile: {error}"))?;
    Ok(())
}

#[cfg(unix)]
fn open_existing_profile_no_follow(path: &Path) -> Result<std::fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| {
            format!("open existing sandbox profile without following symlinks: {error}")
        })?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("inspect opened sandbox profile: {error}"))?;
    if !metadata.is_file() {
        return Err("opened sandbox profile path is not a regular file".into());
    }
    Ok(file)
}

#[cfg(not(unix))]
fn open_existing_profile_no_follow(path: &Path) -> Result<std::fs::File, String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|error| format!("open existing sandbox profile: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("inspect opened sandbox profile: {error}"))?;
    if !metadata.is_file() {
        return Err("opened sandbox profile path is not a regular file".into());
    }
    Ok(file)
}

#[cfg(unix)]
fn create_new_profile_no_follow(path: &Path) -> Result<std::fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| format!("create sandbox profile without following symlinks: {error}"))
}

#[cfg(not(unix))]
fn create_new_profile_no_follow(path: &Path) -> Result<std::fs::File, String> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create sandbox profile: {error}"))
}

pub fn sandbox_exec_available_on_this_platform() -> bool {
    sandbox_exec_available_for_os_and_program(std::env::consts::OS, Path::new(SANDBOX_EXEC_PROGRAM))
}

fn sandbox_exec_available_for_os_and_program(os: &str, program: &Path) -> bool {
    if os != "macos" {
        return false;
    }
    sandbox_exec_path_is_executable(program)
}

fn sandbox_exec_path_is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn provider_allowlist_profile(
    endpoints: &[String],
    egress_proxy_port: u16,
) -> Result<SandboxProfile, String> {
    if endpoints.is_empty() {
        return Err("provider allowlist sandbox profile requires at least one endpoint".into());
    }
    if egress_proxy_port == 0 {
        return Err("egress proxy port must be greater than zero".into());
    }

    let mut seen = BTreeSet::new();
    for endpoint in endpoints {
        let (host, port) = normalize_provider_endpoint(endpoint)?;
        seen.insert((host, port));
    }
    let mut lines = vec![
        "(version 1)".into(),
        "(allow default)".into(),
        "(deny network*)".into(),
        format!("(allow network-outbound (remote tcp \"localhost:{egress_proxy_port}\"))"),
    ];
    lines.extend(
        seen.into_iter()
            .map(|(host, port)| format!("; a2-allowlist-endpoint {host}:{port}")),
    );
    Ok(profile_from_lines(lines))
}

fn provider_proxy_environment(
    policy: &NetworkPolicy,
    egress_proxy_port: Option<u16>,
) -> Result<Vec<(OsString, OsString)>, String> {
    match policy {
        NetworkPolicy::AllowList(_) => {
            let port = egress_proxy_port.ok_or_else(|| {
                "Seatbelt cannot filter hostnames; an egress proxy is required for network_policy=AllowList"
                    .to_string()
            })?;
            if port == 0 {
                return Err("egress proxy port must be greater than zero".into());
            }
            let proxy = OsString::from(format!("http://127.0.0.1:{port}"));
            let mut env = [
                "HTTPS_PROXY",
                "HTTP_PROXY",
                "ALL_PROXY",
                "https_proxy",
                "http_proxy",
                "all_proxy",
            ]
            .into_iter()
            .map(|key| (OsString::from(key), proxy.clone()))
            .collect::<Vec<_>>();
            env.push((OsString::from("NODE_USE_ENV_PROXY"), OsString::from("1")));
            env.push((OsString::from("NO_PROXY"), OsString::new()));
            env.push((OsString::from("no_proxy"), OsString::new()));
            Ok(env)
        }
        NetworkPolicy::Isolated => {
            if egress_proxy_port.is_some() {
                Err("network_policy=Isolated cannot use an egress proxy port".into())
            } else {
                Ok(vec![])
            }
        }
        NetworkPolicy::Open => Err("network_policy=Open does not use a proxy environment".into()),
    }
}

fn profile_from_lines(profile_lines: Vec<String>) -> SandboxProfile {
    let text = {
        let mut text = profile_lines.join("\n");
        text.push('\n');
        text
    };
    let profile_sha256 = format!("{:x}", Sha256::digest(text.as_bytes()));
    SandboxProfile {
        engine: "sandbox-exec",
        profile_lines,
        profile_sha256,
    }
}

fn normalize_provider_endpoint(endpoint: &str) -> Result<(String, u16), String> {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return Err("provider allowlist endpoint must not be empty".into());
    }
    if trimmed.contains(char::is_whitespace) {
        return Err("provider allowlist endpoint must not contain whitespace".into());
    }

    let without_scheme = if let Some(rest) = trimmed.strip_prefix("https://") {
        rest
    } else if trimmed.contains("://") {
        return Err("provider allowlist endpoint must use https:// or a bare DNS host".into());
    } else {
        trimmed
    };
    if without_scheme.contains('/') || without_scheme.contains('?') || without_scheme.contains('#')
    {
        return Err(
            "provider allowlist endpoint must be a host or https://host without a path".into(),
        );
    }

    let (host, port) = split_host_port(without_scheme)?;
    validate_provider_host(&host)?;
    Ok((host, port))
}

fn split_host_port(value: &str) -> Result<(String, u16), String> {
    if value.is_empty() {
        return Err("provider allowlist endpoint must include a host".into());
    }
    if value.starts_with('[') || value.contains(']') {
        return Err("provider allowlist endpoint must use a DNS host, not an IP literal".into());
    }
    let mut parts = value.rsplitn(2, ':');
    let last = parts.next().unwrap_or_default();
    let first = parts.next();
    if let Some(host_part) = first {
        if host_part.contains(':') {
            return Err(
                "provider allowlist endpoint must use a DNS host, not an IP literal".into(),
            );
        }
        let port = last
            .parse::<u16>()
            .map_err(|_| "provider allowlist endpoint port must be a valid TCP port".to_string())?;
        if port == 0 {
            return Err("provider allowlist endpoint port must be a valid TCP port".into());
        }
        Ok((normalize_host(host_part), port))
    } else {
        Ok((normalize_host(last), 443))
    }
}

fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn validate_provider_host(host: &str) -> Result<(), String> {
    if host.is_empty() || host == "*" {
        return Err("provider allowlist endpoint must be an exact DNS host".into());
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return Err("provider allowlist endpoint must not be local".into());
    }
    if host == "0.0.0.0" || host == "::" || host.parse::<IpAddr>().is_ok() {
        return Err("provider allowlist endpoint must be a DNS host, not an IP literal".into());
    }
    if BLOCKED_PUBLIC_SOLUTION_HOSTS
        .iter()
        .any(|blocked| host == *blocked || host.ends_with(&format!(".{blocked}")))
    {
        return Err("provider allowlist profile cannot allow public solution hosts".into());
    }
    if host == "example.com"
        || host == "example.net"
        || host == "example.org"
        || host.ends_with(".example")
        || host.ends_with(".example.com")
        || host.ends_with(".example.net")
        || host.ends_with(".example.org")
        || host.ends_with(".invalid")
        || host.ends_with(".test")
    {
        return Err("provider allowlist endpoint must not be synthetic/example".into());
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return Err("provider allowlist endpoint must be a fully qualified provider host".into());
    }
    if labels.iter().any(|label| {
        label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
    }) {
        return Err("provider allowlist endpoint must be a valid DNS host".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn sha256_text(value: &str) -> String {
        format!("{:x}", Sha256::digest(value.as_bytes()))
    }

    #[test]
    fn pi_provider_args_disable_ambient_pi_resources_before_prompt() {
        let args = pi_provider_args("zai/glm-5.1", Some("system text"), "do the task");
        let args: Vec<&str> = args.iter().map(|arg| arg.to_str().unwrap()).collect();

        for flag in PI_AMBIENT_RESOURCE_ISOLATION_ARGS {
            let index = args.iter().position(|arg| *arg == flag);
            assert!(index.is_some(), "missing {flag} in {args:?}");
            assert!(
                index.unwrap() < args.len() - 1,
                "{flag} must precede the prompt"
            );
        }
        assert_eq!(
            &args[..6],
            [
                "--model",
                "zai/glm-5.1",
                "--no-session",
                "--mode",
                "json",
                "--print"
            ]
        );
        let system_index = args
            .iter()
            .position(|arg| *arg == "--append-system-prompt")
            .unwrap();
        assert_eq!(args[system_index + 1], "system text");
        assert_eq!(args.last(), Some(&"do the task"));
    }

    #[test]
    fn codex_provider_args_carry_requested_reasoning_effort() {
        let args: Vec<String> = codex_provider_args("gpt-6-luna", "do it", true, "low")
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"model_reasoning_effort=\"low\"".to_string()));
    }

    #[test]
    fn codex_reasoning_effort_from_env_value_validates_and_defaults_to_high() {
        assert_eq!(codex_reasoning_effort_from_env_value(None).unwrap(), "high");
        for level in ["none", "minimal", "low", "medium", "high", "xhigh", "max"] {
            assert_eq!(
                codex_reasoning_effort_from_env_value(Some(level)).unwrap(),
                level
            );
        }
        assert!(codex_reasoning_effort_from_env_value(Some("extreme")).is_err());
        assert!(codex_reasoning_effort_from_env_value(Some("high\" -c x=1")).is_err());
    }

    #[test]
    fn codex_provider_args_bypass_inner_sandbox_only_inside_a2_sandbox() {
        let inside: Vec<String> = codex_provider_args("gpt-6-luna", "do it", true, "high")
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(&inside[..4], ["exec", "do it", "-m", "gpt-6-luna"]);
        assert!(inside.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert!(!inside.contains(&"--approve-for-me".to_string()));
        for flag in ["--ephemeral", "--skip-git-repo-check", "--json"] {
            assert!(inside.contains(&flag.to_string()), "missing {flag}");
        }
        assert!(!inside.contains(&"--full-auto".to_string()));

        let outside: Vec<String> = codex_provider_args("gpt-6-luna", "do it", false, "high")
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(outside.contains(&"--approve-for-me".to_string()));
        assert!(!outside.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
    }

    #[test]
    fn pi_provider_args_omit_system_prompt_when_absent() {
        let args = pi_provider_args("zai/glm-5.1", None, "do the task");

        assert!(!args.iter().any(|arg| arg == "--append-system-prompt"));
        assert_eq!(
            args.last().map(OsString::as_os_str),
            Some(OsStr::new("do the task"))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn isolated_profile_denies_network_to_grandchild_processes() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let probe = format!(
            "sh -c 'python3 -c \"import socket; socket.create_connection((\\\"127.0.0.1\\\", {port}), timeout=3)\"'"
        );
        let control = std::process::Command::new("sh")
            .args(["-c", &probe])
            .output()
            .unwrap();
        assert!(
            control.status.success(),
            "unsandboxed control probe must connect: {}",
            String::from_utf8_lossy(&control.stderr)
        );

        let profile_dir = std::env::temp_dir().join(format!(
            "a2-sandbox-grandchild-inheritance-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let materialized = materialize_sandbox_exec_command(
            &NetworkPolicy::Isolated,
            &profile_dir,
            "sh",
            ["-c", probe.as_str()],
        )
        .unwrap();
        let sandboxed = std::process::Command::new(&materialized.program)
            .args(&materialized.args)
            .output()
            .unwrap();
        std::fs::remove_dir_all(&profile_dir).ok();

        assert!(
            !sandboxed.status.success(),
            "sandboxed grandchild must not reach the network"
        );
        assert!(
            String::from_utf8_lossy(&sandboxed.stderr).contains("Operation not permitted"),
            "expected a sandbox denial, got: {}",
            String::from_utf8_lossy(&sandboxed.stderr)
        );
    }

    #[test]
    fn isolated_profile_denies_network_by_default_and_hashes_exact_lines() {
        let profile = sandbox_profile_for_network_policy(&NetworkPolicy::Isolated).unwrap();

        assert_eq!(profile.engine, "sandbox-exec");
        assert_eq!(
            profile.profile_lines,
            vec!["(version 1)", "(allow default)", "(deny network*)"]
        );
        assert_eq!(
            profile.text(),
            "(version 1)\n(allow default)\n(deny network*)\n"
        );
        assert_eq!(profile.profile_sha256, sha256_text(&profile.text()));
    }

    #[test]
    fn denied_subpaths_are_added_after_allow_default_and_change_profile_hash() {
        let denied = PathBuf::from("/private/tmp/external-secret");
        let original = sandbox_profile_for_network_policy(&NetworkPolicy::Isolated).unwrap();
        let profile = sandbox_profile_for_network_policy_with_denied_paths(
            &NetworkPolicy::Isolated,
            std::slice::from_ref(&denied),
        )
        .unwrap();

        assert_eq!(
            profile.profile_lines,
            vec![
                "(version 1)",
                "(allow default)",
                "(deny file-read* file-write* (subpath \"/private/tmp/external-secret\"))",
                "(deny network*)",
            ]
        );
        assert_ne!(profile.profile_sha256, original.profile_sha256);
        assert_eq!(profile.profile_sha256, sha256_text(&profile.text()));
    }

    #[test]
    fn denied_subpaths_reject_sandbox_string_escape_characters() {
        for denied in [
            PathBuf::from("/tmp/quote\"path"),
            PathBuf::from("/tmp/back\\slash"),
        ] {
            let error = sandbox_profile_for_network_policy_with_denied_paths(
                &NetworkPolicy::Isolated,
                &[denied],
            )
            .unwrap_err();
            assert!(
                error.contains("must not contain"),
                "unexpected error: {error}"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn denied_subpath_blocks_file_reads_but_allows_outside_reads() {
        let temp_dir = PathBuf::from("/private/tmp").join(format!(
            "a2-sandbox-deny-read-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let denied_dir = temp_dir.join("denied");
        let allowed_dir = temp_dir.join("allowed");
        std::fs::create_dir_all(&denied_dir).unwrap();
        std::fs::create_dir_all(&allowed_dir).unwrap();
        let denied_file = denied_dir.join("secret.txt");
        let allowed_file = allowed_dir.join("public.txt");
        std::fs::write(&denied_file, "secret").unwrap();
        std::fs::write(&allowed_file, "public").unwrap();
        let denied_dir = denied_dir.canonicalize().unwrap();

        let probe = materialize_sandbox_exec_command_with_denied_paths(
            &NetworkPolicy::Isolated,
            &temp_dir.join("profiles"),
            "sh",
            [
                OsStr::new("-c"),
                OsStr::new(
                    "cat \"$1\"; denied_status=$?; cat \"$2\"; allowed_status=$?; test \"$denied_status\" -ne 0 && test \"$allowed_status\" -eq 0",
                ),
                OsStr::new("sandbox-file-probe"),
                denied_file.as_os_str(),
                allowed_file.as_os_str(),
            ],
            std::slice::from_ref(&denied_dir),
        )
        .unwrap();
        let output = std::process::Command::new(&probe.program)
            .args(&probe.args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "sandbox file probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Operation not permitted"),
            "expected file-read denial, got: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "public");

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn allowlist_profile_emits_exact_proxy_rule_and_sorted_endpoint_comments() {
        let profile = sandbox_profile_for_network_policy_with_egress_proxy(
            &NetworkPolicy::AllowList(vec![
                "https://api.openai.com".into(),
                "api.anthropic.com:8443".into(),
            ]),
            Some(43123),
        )
        .unwrap();

        assert_eq!(profile.engine, "sandbox-exec");
        assert_eq!(
            profile.profile_lines,
            vec![
                "(version 1)",
                "(allow default)",
                "(deny network*)",
                "(allow network-outbound (remote tcp \"localhost:43123\"))",
                "; a2-allowlist-endpoint api.anthropic.com:8443",
                "; a2-allowlist-endpoint api.openai.com:443",
            ]
        );
        assert_eq!(
            profile.text(),
            "(version 1)\n(allow default)\n(deny network*)\n(allow network-outbound (remote tcp \"localhost:43123\"))\n; a2-allowlist-endpoint api.anthropic.com:8443\n; a2-allowlist-endpoint api.openai.com:443\n"
        );
        assert_eq!(
            profile.profile_sha256,
            "760d90c4bab60db7ade72ae8f658a5e0cc408256af3e7e790ab4a50d65651802"
        );
    }

    #[test]
    fn allowlist_profile_without_egress_proxy_fails_closed() {
        let error = sandbox_profile_for_network_policy(&NetworkPolicy::AllowList(vec![
            "https://api.openai.com".into(),
        ]))
        .unwrap_err();

        assert!(error.contains("Seatbelt cannot filter hostnames"));
        assert!(error.contains("egress proxy is required"));
    }

    #[test]
    fn isolated_profile_rejects_egress_proxy_port() {
        let error = sandbox_profile_for_network_policy_with_egress_proxy(
            &NetworkPolicy::Isolated,
            Some(43123),
        )
        .unwrap_err();

        assert!(error.contains("Isolated"));
        assert!(error.contains("egress proxy"));
    }

    #[test]
    fn allowlist_profile_rejects_public_solution_hosts() {
        for endpoint in [
            "github.com",
            "api.github.com",
            "raw.githubusercontent.com",
            "docs.github.io",
        ] {
            let err = sandbox_profile_for_network_policy_with_egress_proxy(
                &NetworkPolicy::AllowList(vec![endpoint.into()]),
                Some(43123),
            )
            .unwrap_err();
            assert!(
                err.contains("public solution hosts"),
                "unexpected error for {endpoint}: {err}"
            );
        }
    }

    #[test]
    fn allowlist_profile_rejects_local_synthetic_ip_and_path_endpoints() {
        for endpoint in [
            "localhost",
            "127.0.0.1",
            "api.example-provider.invalid",
            "https://api.openai.com/v1",
            "http://api.openai.com",
            "singlelabel",
        ] {
            assert!(
                sandbox_profile_for_network_policy_with_egress_proxy(
                    &NetworkPolicy::AllowList(vec![endpoint.into()]),
                    Some(43123),
                )
                .is_err(),
                "endpoint should be rejected: {endpoint}"
            );
        }
    }

    #[test]
    fn open_policy_does_not_generate_sandbox_profile() {
        let err = sandbox_profile_for_network_policy(&NetworkPolicy::Open).unwrap_err();
        assert!(err.contains("network_policy=Open does not need"));
    }

    #[test]
    fn materializes_isolated_sandbox_exec_command_without_executing_child() {
        let temp_dir = std::env::temp_dir().join(format!(
            "a2-sandbox-materialize-isolated-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let child = temp_dir.join("fake-provider");
        let marker = temp_dir.join("provider-was-executed");
        std::fs::write(&child, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();

        let materialized = materialize_sandbox_exec_command(
            &NetworkPolicy::Isolated,
            &temp_dir.join("profiles"),
            child.as_os_str(),
            [OsStr::new("--model"), OsStr::new("demo")],
        )
        .unwrap();

        assert_eq!(materialized.program, OsString::from(SANDBOX_EXEC_PROGRAM));
        assert_eq!(materialized.profile.engine, "sandbox-exec");
        assert_eq!(
            std::fs::read_to_string(&materialized.profile_path).unwrap(),
            materialized.profile.text()
        );
        assert_eq!(materialized.args[0], OsString::from("-f"));
        assert_eq!(
            materialized.args[1],
            materialized.profile_path.clone().into_os_string()
        );
        assert_eq!(materialized.args[2], child.clone().into_os_string());
        assert_eq!(materialized.args[3], OsString::from("--model"));
        assert_eq!(materialized.args[4], OsString::from("demo"));
        assert!(
            !marker.exists(),
            "sandbox command materialization must not execute the provider child"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn materializes_allowlist_profile_with_normalized_deterministic_filename() {
        let temp_dir = std::env::temp_dir().join(format!(
            "a2-sandbox-materialize-allowlist-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);

        let materialized = materialize_sandbox_exec_command_with_egress_proxy(
            &NetworkPolicy::AllowList(vec![
                "https://api.openai.com".into(),
                "API.OPENAI.COM:443".into(),
            ]),
            Some(43123),
            &temp_dir,
            "provider-cli",
            ["--json"],
        )
        .unwrap();

        assert_eq!(
            materialized
                .profile_path
                .file_name()
                .and_then(OsStr::to_str),
            Some(format!("a2-sandbox-{}.sb", materialized.profile.profile_sha256).as_str())
        );
        assert_eq!(
            materialized
                .profile
                .profile_lines
                .iter()
                .filter(|line| line.contains("api.openai.com:443"))
                .count(),
            1
        );
        assert_eq!(
            std::fs::read_to_string(&materialized.profile_path).unwrap(),
            materialized.profile.text()
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn allowlist_materialization_sets_proxy_environment_and_isolated_omits_it() {
        let temp_dir = std::env::temp_dir().join(format!(
            "a2-sandbox-proxy-env-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let allowlist = materialize_provider_command_for_network_policy_with_egress_proxy(
            Some(&NetworkPolicy::AllowList(vec![
                "https://api.openai.com".into(),
            ])),
            Some(43123),
            &temp_dir,
            "provider-cli",
            ["--json"],
        )
        .unwrap();
        let expected_proxy = OsString::from("http://127.0.0.1:43123");
        for key in [
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "ALL_PROXY",
            "https_proxy",
            "http_proxy",
            "all_proxy",
        ] {
            assert!(
                allowlist
                    .env
                    .contains(&(OsString::from(key), expected_proxy.clone())),
                "missing {key} in {:?}",
                allowlist.env
            );
        }
        assert!(
            allowlist
                .env
                .contains(&(OsString::from("NODE_USE_ENV_PROXY"), OsString::from("1")))
        );
        for key in ["NO_PROXY", "no_proxy"] {
            assert!(
                allowlist
                    .env
                    .contains(&(OsString::from(key), OsString::new()))
            );
        }

        let isolated = materialize_provider_command_for_network_policy(
            Some(&NetworkPolicy::Isolated),
            &temp_dir,
            "provider-cli",
            ["--json"],
        )
        .unwrap();
        assert!(isolated.env.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn allowlist_profile_is_accepted_and_only_proxy_listener_is_reachable() {
        let proxy_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_port = proxy_listener.local_addr().unwrap().port();
        let blocked_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let blocked_port = blocked_listener.local_addr().unwrap().port();
        let temp_dir = std::env::temp_dir().join(format!(
            "a2-sandbox-allowlist-runtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let accepted = materialize_sandbox_exec_command_with_egress_proxy(
            &NetworkPolicy::AllowList(vec!["https://api.openai.com".into()]),
            Some(proxy_port),
            &temp_dir,
            "/usr/bin/true",
            std::iter::empty::<&str>(),
        )
        .unwrap();
        let output = std::process::Command::new(&accepted.program)
            .args(&accepted.args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "sandbox-exec rejected profile: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let allowed = materialize_sandbox_exec_command_with_egress_proxy(
            &NetworkPolicy::AllowList(vec!["https://api.openai.com".into()]),
            Some(proxy_port),
            &temp_dir,
            "/usr/bin/nc",
            ["-z", "127.0.0.1", &proxy_port.to_string()],
        )
        .unwrap();
        assert!(
            std::process::Command::new(&allowed.program)
                .args(&allowed.args)
                .status()
                .unwrap()
                .success(),
            "sandbox must allow connecting to the configured proxy port"
        );

        let blocked = materialize_sandbox_exec_command_with_egress_proxy(
            &NetworkPolicy::AllowList(vec!["https://api.openai.com".into()]),
            Some(proxy_port),
            &temp_dir,
            "/usr/bin/nc",
            ["-z", "127.0.0.1", &blocked_port.to_string()],
        )
        .unwrap();
        assert!(
            !std::process::Command::new(&blocked.program)
                .args(&blocked.args)
                .status()
                .unwrap()
                .success(),
            "sandbox must deny connecting to any other local port"
        );

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn materialization_rejects_profile_symlink_without_writing_target() {
        let temp_dir = std::env::temp_dir().join(format!(
            "a2-sandbox-materialize-symlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let profile = sandbox_profile_for_network_policy(&NetworkPolicy::Isolated).unwrap();
        let profile_path = temp_dir.join(format!("a2-sandbox-{}.sb", profile.profile_sha256));
        let symlink_target = temp_dir.join("attacker-controlled-profile-target");
        std::fs::write(&symlink_target, profile.text()).unwrap();
        std::os::unix::fs::symlink(&symlink_target, &profile_path).unwrap();

        let err = materialize_sandbox_exec_command(
            &NetworkPolicy::Isolated,
            &temp_dir,
            "provider-cli",
            ["--blocked"],
        )
        .unwrap_err();

        assert!(err.contains("symlink"));
        assert_eq!(
            std::fs::read_to_string(&symlink_target).unwrap(),
            profile.text(),
            "materialization must reject even a symlink to matching profile text instead of canonicalizing through it"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[cfg(unix)]
    #[test]
    fn materialization_rejects_symlinked_profile_directory_before_writing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "a2-sandbox-materialize-dir-symlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let real_dir = temp_dir.join("real-profile-dir");
        let linked_dir = temp_dir.join("linked-profile-dir");
        std::fs::create_dir_all(&real_dir).unwrap();
        std::os::unix::fs::symlink(&real_dir, &linked_dir).unwrap();

        let err = materialize_sandbox_exec_command(
            &NetworkPolicy::Isolated,
            &linked_dir,
            "provider-cli",
            ["--blocked"],
        )
        .unwrap_err();

        assert!(err.contains("directory symlink"));
        assert!(
            std::fs::read_dir(&real_dir).unwrap().next().is_none(),
            "materialization must not write profiles through a symlinked profile directory"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn materialization_reuses_matching_profile_and_rejects_mismatched_profile_path() {
        let temp_dir = std::env::temp_dir().join(format!(
            "a2-sandbox-materialize-existing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);

        let first = materialize_sandbox_exec_command(
            &NetworkPolicy::Isolated,
            &temp_dir,
            "provider-cli",
            ["--once"],
        )
        .unwrap();
        let second = materialize_sandbox_exec_command(
            &NetworkPolicy::Isolated,
            &temp_dir,
            "provider-cli",
            ["--twice"],
        )
        .unwrap();
        assert_eq!(first.profile_path, second.profile_path);
        assert_eq!(
            std::fs::read_to_string(&first.profile_path).unwrap(),
            first.profile.text()
        );

        std::fs::write(&first.profile_path, "tampered profile\n").unwrap();
        let err = materialize_sandbox_exec_command(
            &NetworkPolicy::Isolated,
            &temp_dir,
            "provider-cli",
            ["--blocked"],
        )
        .unwrap_err();
        assert!(err.contains("different contents"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn materialization_rejects_open_policy_and_does_not_write_profile() {
        let temp_dir = std::env::temp_dir().join(format!(
            "a2-sandbox-materialize-open-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);

        let err =
            materialize_sandbox_exec_command(&NetworkPolicy::Open, &temp_dir, "provider", ["arg"])
                .unwrap_err();

        assert!(err.contains("network_policy=Open does not need"));
        assert!(
            !temp_dir.exists(),
            "rejecting open policy should not create a sandbox profile directory"
        );
    }

    #[test]
    fn sandbox_exec_availability_requires_macos_and_exact_program_path() {
        let temp_dir = std::env::temp_dir().join(format!(
            "a2-sandbox-exec-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let fake_sandbox_exec = temp_dir.join("sandbox-exec");
        let missing_sandbox_exec = temp_dir.join("missing-sandbox-exec");
        let execution_marker = temp_dir.join("should-not-be-created-by-availability-check");
        std::fs::write(
            &fake_sandbox_exec,
            format!(
                "#!/bin/sh\ntouch '{}'\nexit 0\n",
                execution_marker.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&fake_sandbox_exec).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&fake_sandbox_exec, permissions).unwrap();
        }
        assert!(sandbox_exec_available_for_os_and_program(
            "macos",
            &fake_sandbox_exec
        ));
        assert!(
            !execution_marker.exists(),
            "availability check must not execute a PATH-sourced sandbox-exec binary"
        );
        assert!(!sandbox_exec_available_for_os_and_program(
            "linux",
            &fake_sandbox_exec
        ));
        assert!(!sandbox_exec_available_for_os_and_program(
            "macos",
            &missing_sandbox_exec
        ));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&fake_sandbox_exec).unwrap().permissions();
            permissions.set_mode(0o644);
            std::fs::set_permissions(&fake_sandbox_exec, permissions).unwrap();
            assert!(!sandbox_exec_available_for_os_and_program(
                "macos",
                &fake_sandbox_exec
            ));
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
