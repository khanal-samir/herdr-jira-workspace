use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "jira-workspace-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_herdr-jira-workspace"))
}

#[test]
fn offline_demo_does_not_read_config_or_resolve_credentials() {
    let temp = Temp::new();
    let marker = temp.0.join("credentials-were-read");
    fs::write(temp.0.join("config.toml"),format!("[jira]\nbase_url='https://example.atlassian.net'\nemail='test@example.com'\napi_token_cmd='touch {}'\n",marker.display())).unwrap();
    let output = binary()
        .arg("--dump-demo")
        .env("HERDR_PLUGIN_CONFIG_DIR", &temp.0)
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();
    assert!(output.contains("DEMO-1"));
    assert!(output.contains("JIRA WORKSPACE"));
    assert!(!marker.exists());
}

#[test]
fn config_check_does_not_run_the_token_command() {
    let temp = Temp::new();
    let marker = temp.0.join("token-command");
    fs::write(
        temp.0.join("config.toml"),
        format!(
            "[jira]\nbase_url='https://example.atlassian.net'\napi_token_cmd='touch {}'\n",
            marker.display()
        ),
    )
    .unwrap();
    let output = binary()
        .arg("--check")
        .env("HERDR_PLUGIN_CONFIG_DIR", &temp.0)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!marker.exists());
}

#[test]
fn invalid_config_reports_failure_without_echoing_secret_source() {
    let temp = Temp::new();
    fs::write(
        temp.0.join("config.toml"),
        "[jira]\napi_token = private-secret-value",
    )
    .unwrap();
    let output = binary()
        .arg("--check")
        .env("HERDR_PLUGIN_CONFIG_DIR", &temp.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(!error.contains("private-secret-value"));
}

#[test]
fn unknown_flags_fail_before_starting_the_ui() {
    let output = binary().arg("--unknown").output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn help_is_available_without_configuration() {
    let output = binary().arg("--help").output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("--demo"));
}
