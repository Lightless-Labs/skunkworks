use a2_core::protocol::NetworkPolicy;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};

const BLOCKED_PUBLIC_SOLUTION_HOSTS: [&str; 3] =
    ["github.com", "githubusercontent.com", "github.io"];
const SANDBOX_EXEC_PROGRAM: &str = "/usr/bin/sandbox-exec";

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
    match policy {
        NetworkPolicy::Open => Err(
            "network_policy=Open does not need a sandbox profile; unrestricted provider launch must remain explicit"
                .into(),
        ),
        NetworkPolicy::Isolated => Ok(profile_from_lines(vec![
            "(version 1)".into(),
            "(allow default)".into(),
            "(deny network*)".into(),
        ])),
        NetworkPolicy::AllowList(endpoints) => provider_allowlist_profile(endpoints),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxExecCommandMaterialization {
    pub profile: SandboxProfile,
    pub profile_path: PathBuf,
    pub program: OsString,
    pub args: Vec<OsString>,
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
    let profile = sandbox_profile_for_network_policy(policy)?;
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
    sandbox_exec_available_for_os_and_path(
        std::env::consts::OS,
        std::env::var_os("PATH").as_deref(),
    )
}

fn sandbox_exec_available_for_os_and_path(os: &str, path: Option<&OsStr>) -> bool {
    if os != "macos" {
        return false;
    }
    let Some(path) = path else {
        return false;
    };
    std::env::split_paths(path)
        .any(|dir| sandbox_exec_path_is_executable(&dir.join("sandbox-exec")))
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

fn provider_allowlist_profile(endpoints: &[String]) -> Result<SandboxProfile, String> {
    if endpoints.is_empty() {
        return Err("provider allowlist sandbox profile requires at least one endpoint".into());
    }

    let mut seen = BTreeSet::new();
    let mut lines = vec![
        "(version 1)".into(),
        "(allow default)".into(),
        "(deny network*)".into(),
    ];
    for endpoint in endpoints {
        let (host, port) = normalize_provider_endpoint(endpoint)?;
        if seen.insert((host.clone(), port)) {
            lines.push(format!(
                "(allow network-outbound (remote tcp \"{host}:{port}\"))"
            ));
        }
    }
    Ok(profile_from_lines(lines))
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

    fn sha256_text(value: &str) -> String {
        format!("{:x}", Sha256::digest(value.as_bytes()))
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
    fn allowlist_profile_emits_exact_provider_host_port_rules() {
        let profile = sandbox_profile_for_network_policy(&NetworkPolicy::AllowList(vec![
            "https://api.openai.com".into(),
            "api.anthropic.com:8443".into(),
        ]))
        .unwrap();

        assert_eq!(profile.engine, "sandbox-exec");
        assert!(profile.profile_lines.contains(&"(deny network*)".into()));
        assert!(profile
            .profile_lines
            .contains(&"(allow network-outbound (remote tcp \"api.openai.com:443\"))".into()));
        assert!(profile
            .profile_lines
            .contains(&"(allow network-outbound (remote tcp \"api.anthropic.com:8443\"))".into()));
        assert_eq!(profile.profile_sha256, sha256_text(&profile.text()));
    }

    #[test]
    fn allowlist_profile_rejects_public_solution_hosts() {
        for endpoint in [
            "github.com",
            "api.github.com",
            "raw.githubusercontent.com",
            "docs.github.io",
        ] {
            let err =
                sandbox_profile_for_network_policy(&NetworkPolicy::AllowList(
                    vec![endpoint.into()],
                ))
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
                sandbox_profile_for_network_policy(&NetworkPolicy::AllowList(
                    vec![endpoint.into()]
                ))
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

        let materialized = materialize_sandbox_exec_command(
            &NetworkPolicy::AllowList(vec![
                "https://api.openai.com".into(),
                "API.OPENAI.COM:443".into(),
            ]),
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
    fn sandbox_exec_availability_requires_macos_and_binary_on_path() {
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
        let path = std::env::join_paths([temp_dir.as_os_str()]).unwrap();

        assert!(sandbox_exec_available_for_os_and_path("macos", Some(&path)));
        assert!(
            !execution_marker.exists(),
            "availability check must not execute a PATH-sourced sandbox-exec binary"
        );
        assert!(!sandbox_exec_available_for_os_and_path(
            "linux",
            Some(&path)
        ));
        assert!(!sandbox_exec_available_for_os_and_path("macos", None));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&fake_sandbox_exec).unwrap().permissions();
            permissions.set_mode(0o644);
            std::fs::set_permissions(&fake_sandbox_exec, permissions).unwrap();
            assert!(!sandbox_exec_available_for_os_and_path(
                "macos",
                Some(&path)
            ));
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
