use serde::Deserialize;
use std::{path::PathBuf, process::Command};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    pub jira: JiraConfig,
    #[serde(default)]
    pub board_id: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JiraConfig {
    pub base_url: String,
    #[serde(default = "default_auth")]
    pub auth: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub api_token: String,
    #[serde(default)]
    pub api_token_cmd: String,
    #[serde(default = "page_size")]
    pub max_results: u32,
}

impl Default for JiraConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            auth: default_auth(),
            email: String::new(),
            api_token: String::new(),
            api_token_cmd: String::new(),
            max_results: page_size(),
        }
    }
}

fn default_auth() -> String {
    "basic".into()
}
fn page_size() -> u32 {
    100
}

pub fn config_path() -> PathBuf {
    std::env::var("HERDR_PLUGIN_CONFIG_DIR")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config/herdr/plugins/config/herdr-jira-workspace"))
        .join("config.toml")
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let requested = config_path();
        // Reuse existing credentials without copying or changing the old plugin.
        let path = if requested.exists() {
            requested
        } else {
            home().join(".config/herdr/plugins/config/herdr-jira/config.toml")
        };
        let raw = std::fs::read_to_string(&path).map_err(|e| {
            format!(
                "Cannot read {}: {e}. Copy config.example.toml to {}.",
                path.display(),
                config_path().display()
            )
        })?;
        Self::parse(&raw)
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        // Do not include TOML's source snippet: it can contain credentials.
        let mut cfg: Self =
            toml::from_str(raw).map_err(|_| "Invalid config.toml syntax".to_string())?;
        cfg.jira.base_url = cfg.jira.base_url.trim().trim_end_matches('/').to_string();
        let url = &cfg.jira.base_url;
        if !(url.starts_with("https://")
            || url.starts_with("http://localhost:")
            || url.starts_with("http://127.0.0.1:"))
        {
            return Err(
                "Jira base_url must use HTTPS (HTTP is allowed only for localhost testing)".into(),
            );
        }
        if !["basic", "bearer"].contains(&cfg.jira.auth.as_str()) {
            return Err("Jira auth must be basic or bearer".into());
        }
        cfg.jira.max_results = cfg.jira.max_results.clamp(1, 100);
        Ok(cfg)
    }

    pub fn resolve_token(&self) -> Result<String, String> {
        if !self.jira.api_token.trim().is_empty() {
            return Ok(self.jira.api_token.trim().into());
        }
        if self.jira.api_token_cmd.trim().is_empty() {
            return Err("Set jira.api_token_cmd in config.toml".into());
        }
        let out = Command::new("sh")
            .args(["-c", &self.jira.api_token_cmd])
            .output()
            .map_err(|_| "Could not start api_token_cmd".to_string())?;
        if !out.status.success() {
            return Err("api_token_cmd failed; check your credential store".into());
        }
        let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if token.is_empty() {
            return Err("api_token_cmd returned no token".into());
        }
        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_existing_config_and_validates_transport() {
        let cfg = Config::parse("[jira]\nbase_url='https://example.atlassian.net/'\napi_token_cmd='pass jira'\n[[filters]]\nname='Old'\njql='x'").unwrap();
        assert_eq!(cfg.jira.base_url, "https://example.atlassian.net");
        assert!(Config::parse("[jira]\nbase_url='http://example.com'").is_err());
    }
    #[test]
    fn invalid_config_does_not_echo_secrets() {
        let error = Config::parse("[jira]\napi_token = super-secret").unwrap_err();
        assert!(!error.contains("super-secret"));
    }
}
