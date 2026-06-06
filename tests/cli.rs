//! End-to-end contract tests: the documented exit codes and JSON envelope
//! must emerge from the real binary, not just the library. Every test
//! isolates config via XDG_CONFIG_HOME and scrubs NCHEAP_* env so the
//! developer's real credentials are never read, and no test makes a network
//! call (all paths fail before the transport layer).

use assert_cmd::Command;

fn ncheap(temp_config: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("ncheap").expect("binary builds");
    cmd.env("XDG_CONFIG_HOME", temp_config)
        .env("HOME", temp_config)
        .env_remove("NCHEAP_API_USER")
        .env_remove("NCHEAP_API_KEY")
        .env_remove("NCHEAP_USERNAME")
        .env_remove("NCHEAP_CLIENT_IP")
        .env_remove("NCHEAP_SANDBOX")
        .env_remove("NCHEAP_PROFILE");
    cmd
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ncheap-cli-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn fake_creds(cmd: &mut Command) {
    cmd.env("NCHEAP_API_USER", "u")
        .env("NCHEAP_API_KEY", "k")
        .env("NCHEAP_CLIENT_IP", "192.0.2.1");
}

#[test]
fn missing_config_exits_3_with_failure_envelope() {
    let dir = temp_dir("noconfig");
    let output = ncheap(&dir)
        .args(["domains", "list", "--json"])
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(3));
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON envelope");
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "domains.list");
    assert!(v["data"].is_null());
    assert_eq!(v["error"]["kind"], "config");
    assert!(v["error"]["code"].is_null());
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("missing credentials")
    );
}

#[test]
fn missing_config_human_mode_uses_stderr() {
    let dir = temp_dir("noconfig-human");
    let output = ncheap(&dir)
        .args(["domains", "list"])
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty(), "human errors go to stderr");
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("error:"));
}

#[test]
fn clap_usage_error_exits_2() {
    let dir = temp_dir("usage");
    let output = ncheap(&dir).arg("no-such-command").output().expect("run");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn subdomain_rejection_exits_2_with_usage_envelope() {
    let dir = temp_dir("subdomain");
    let mut cmd = ncheap(&dir);
    fake_creds(&mut cmd);
    let output = cmd
        .args(["dns", "get", "www.example.com", "--json"])
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON envelope");
    assert_eq!(v["ok"], false);
    assert_eq!(v["command"], "dns.get");
    assert_eq!(v["error"]["kind"], "usage");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("example.com")
    );
}

#[test]
fn raw_allowlist_rejection_exits_2_before_any_network() {
    let dir = temp_dir("rawreject");
    let mut cmd = ncheap(&dir);
    fake_creds(&mut cmd);
    let output = cmd
        .args(["raw", "domains.dns.setCustom", "--json"])
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON envelope");
    assert_eq!(v["error"]["kind"], "usage");
    assert_eq!(v["command"], "raw");
}

#[test]
fn check_over_50_domains_exits_2_before_any_network() {
    let dir = temp_dir("check50");
    let mut cmd = ncheap(&dir);
    fake_creds(&mut cmd);
    let domains: Vec<String> = (0..51).map(|i| format!("d{i}.example")).collect();
    let output = cmd
        .args(["domains", "check"])
        .args(&domains)
        .arg("--json")
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON envelope");
    assert_eq!(v["error"]["kind"], "usage");
    assert!(v["error"]["message"].as_str().unwrap().contains("50"));
}

#[test]
fn clap_error_with_json_flag_emits_failure_envelope() {
    let dir = temp_dir("clapjson");
    let output = ncheap(&dir)
        .args(["--json", "domains", "nosuchsub"])
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON envelope");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["kind"], "usage");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("nosuchsub")
    );
}

#[test]
fn help_still_exits_0_with_usage_on_stdout() {
    let dir = temp_dir("help");
    let output = ncheap(&dir).arg("--help").output().expect("run");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
}

#[test]
fn dns_set_without_yes_is_refused_non_interactively_before_network() {
    let dir = temp_dir("setnoyes");
    let mut cmd = ncheap(&dir);
    fake_creds(&mut cmd);
    let output = cmd
        .args([
            "dns",
            "set",
            "example.com",
            "ns1.example.net",
            "ns2.example.net",
            "--json",
        ])
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON envelope");
    assert_eq!(v["error"]["kind"], "usage");
    assert!(v["error"]["message"].as_str().unwrap().contains("--yes"));
}

#[test]
fn dns_set_against_production_is_refused_even_with_yes() {
    let dir = temp_dir("setprod");
    let mut cmd = ncheap(&dir);
    fake_creds(&mut cmd); // production: no NCHEAP_SANDBOX, no config opt-in
    let output = cmd
        .args([
            "dns",
            "set",
            "example.com",
            "ns1.example.net",
            "ns2.example.net",
            "--yes",
            "--json",
        ])
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(3));
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON envelope");
    assert_eq!(v["error"]["kind"], "config");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("allow_production_mutations")
    );
}
