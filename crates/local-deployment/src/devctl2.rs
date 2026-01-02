//! devctl2 integration for subdomain-based Caddy routing
//!
//! When a project has a `.devctl2rc.json` configuration file and devctl2 CLI
//! is available, this module enables subdomain-based routing for dev servers
//! (e.g., `feature-branch.myapp.localhost` instead of `localhost:5173`).

use serde::Deserialize;
use std::path::Path;
use tokio::process::Command;

/// Minimal devctl2 configuration - only the fields we need for routing
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevCtl2Config {
    pub project_name: String,
    pub base_domain: String,
    #[serde(default)]
    pub features: DevCtl2Features,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DevCtl2Features {
    #[serde(default)]
    pub caddy: bool,
}

impl DevCtl2Config {
    /// Load config from .devctl2rc.json in the given directory
    pub async fn load(project_dir: &Path) -> Option<Self> {
        let config_path = project_dir.join(".devctl2rc.json");
        if !config_path.exists() {
            return None;
        }

        match tokio::fs::read_to_string(&config_path).await {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(config) => Some(config),
                Err(e) => {
                    tracing::warn!("Failed to parse .devctl2rc.json: {}", e);
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read .devctl2rc.json: {}", e);
                None
            }
        }
    }
}

/// Check if devctl2 CLI is available in PATH
pub fn is_devctl2_available() -> bool {
    std::process::Command::new("devctl2")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Sanitize a branch name for use as a subdomain
/// - Converts to lowercase
/// - Replaces / and _ with -
/// - Removes non-alphanumeric characters (except -)
pub fn sanitize_branch_for_subdomain(branch: &str) -> String {
    branch
        .to_lowercase()
        .replace('/', "-")
        .replace('_', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// Run `devctl2 setup <subdomain>` in the given working directory
pub async fn run_devctl2_setup(workdir: &Path, subdomain: &str) -> Result<(), std::io::Error> {
    tracing::info!(
        "Running devctl2 setup {} in {:?}",
        subdomain,
        workdir
    );

    let output = Command::new("devctl2")
        .args(["setup", subdomain])
        .current_dir(workdir)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("devctl2 setup failed: {}", stderr);
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        tracing::info!("devctl2 setup output: {}", stdout);
    }

    Ok(())
}

/// Run `devctl2 remove <subdomain>` in the given working directory
pub async fn run_devctl2_remove(workdir: &Path, subdomain: &str) -> Result<(), std::io::Error> {
    tracing::info!(
        "Running devctl2 remove {} in {:?}",
        subdomain,
        workdir
    );

    let output = Command::new("devctl2")
        .args(["remove", subdomain])
        .current_dir(workdir)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("devctl2 remove failed: {}", stderr);
    }

    Ok(())
}

/// Extract subdomain from a devctl2 URL like "https://feature-branch.myapp.localhost"
pub fn extract_subdomain_from_url(url: &str) -> Option<String> {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .and_then(|rest| rest.split('.').next())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_branch_for_subdomain() {
        assert_eq!(sanitize_branch_for_subdomain("feature/auth"), "feature-auth");
        assert_eq!(sanitize_branch_for_subdomain("feature_auth"), "feature-auth");
        assert_eq!(sanitize_branch_for_subdomain("Feature/Auth"), "feature-auth");
        assert_eq!(sanitize_branch_for_subdomain("main"), "main");
        assert_eq!(sanitize_branch_for_subdomain("fix/bug-123"), "fix-bug-123");
        assert_eq!(sanitize_branch_for_subdomain("/leading-slash/"), "leading-slash");
    }

    #[test]
    fn test_extract_subdomain_from_url() {
        assert_eq!(
            extract_subdomain_from_url("https://feature-auth.myapp.localhost"),
            Some("feature-auth".to_string())
        );
        assert_eq!(
            extract_subdomain_from_url("http://main.example.com"),
            Some("main".to_string())
        );
        assert_eq!(
            extract_subdomain_from_url("invalid-url"),
            None
        );
    }
}
