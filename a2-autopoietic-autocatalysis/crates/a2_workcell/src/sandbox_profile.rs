use a2_core::protocol::NetworkPolicy;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::net::IpAddr;
use std::path::Path;

const BLOCKED_PUBLIC_SOLUTION_HOSTS: [&str; 3] =
    ["github.com", "githubusercontent.com", "github.io"];

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
        assert!(
            profile
                .profile_lines
                .contains(&"(allow network-outbound (remote tcp \"api.openai.com:443\"))".into())
        );
        assert!(
            profile.profile_lines.contains(
                &"(allow network-outbound (remote tcp \"api.anthropic.com:8443\"))".into()
            )
        );
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
            let err = sandbox_profile_for_network_policy(&NetworkPolicy::AllowList(vec![
                endpoint.into(),
            ]))
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
                sandbox_profile_for_network_policy(&NetworkPolicy::AllowList(vec![
                    endpoint.into()
                ]))
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
