//! Process environment helpers for child process boundaries.
//!
//! These helpers are defense-in-depth for subprocesses that execute model- or
//! benchmark-adjacent code. They make the no-public-solution-search policy
//! observable to children and strip common inherited network configuration.
//! This is not OS/network namespace isolation or no-egress proof.

use std::process::Command;

/// Common proxy/package-manager network configuration variables that should not
/// leak from the parent shell into A²D child subprocesses.
pub fn network_configuration_env_vars() -> [&'static str; 16] {
    [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "FTP_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "ftp_proxy",
        "no_proxy",
        "GIT_PROXY_COMMAND",
        "CARGO_HTTP_PROXY",
        "CARGO_HTTP_CAINFO",
        "CARGO_HTTP_CHECK_REVOKE",
        "RUSTUP_DIST_SERVER",
        "RUSTUP_UPDATE_ROOT",
    ]
}

/// Generic no-public-solution-search policy environment for subprocesses that
/// may execute model-produced artifacts or benchmark-adjacent commands.
pub fn no_public_solution_search_env() -> [(&'static str, &'static str); 3] {
    [
        ("A2D_SANDBOX_POLICY_ENV_SOURCE", "a2d-core-sandbox"),
        ("A2D_GITHUB_SOLUTION_SEARCH_ALLOWED", "false"),
        ("A2D_PUBLIC_SOLUTION_SEARCH_FORBIDDEN", "true"),
    ]
}

/// Apply the generic no-public-solution-search policy env to a child command.
pub fn apply_no_public_solution_search_env(command: &mut Command) {
    command.envs(no_public_solution_search_env());
}

/// Remove inherited network configuration from a child command.
pub fn remove_network_configuration_env(command: &mut Command) {
    for key in network_configuration_env_vars() {
        command.env_remove(key);
    }
}
