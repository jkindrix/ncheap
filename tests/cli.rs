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
        .env_remove("NCHEAP_PROFILE")
        .env_remove("NCHEAP_ENDPOINT");
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

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The one test that proves the differentiating contract end-to-end: a
/// successful --json envelope, exit 0, and POST-with-no-query-credentials
/// through the REAL binary. Uses the debug-build-only NCHEAP_ENDPOINT
/// override against a localhost mock; release builds compile the override
/// out and can only reach the two Namecheap hosts.
#[test]
fn success_envelope_through_real_binary() {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().expect("accept");
        let mut req = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = sock.read(&mut buf).expect("read");
            if n == 0 {
                break;
            }
            req.extend_from_slice(&buf[..n]);
            if let Some(pos) = find_subsequence(&req, b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&req[..pos]).to_ascii_lowercase();
                let content_length: usize = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
                if req.len() >= pos + 4 + content_length {
                    break;
                }
            }
        }
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ApiResponse xmlns="http://api.namecheap.com/xml.response" Status="OK">
  <Errors />
  <RequestedCommand>namecheap.domains.check</RequestedCommand>
  <CommandResponse Type="namecheap.domains.check">
    <DomainCheckResult Domain="zq9probe.com" Available="true" ErrorNo="0" Description="" IsPremiumName="false" PremiumRegistrationPrice="0" PremiumRenewalPrice="0" PremiumRestorePrice="0" PremiumTransferPrice="0" IcannFee="0" EapFee="0"/>
  </CommandResponse>
  <Server>MOCK</Server>
</ApiResponse>"#;
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        sock.write_all(resp.as_bytes()).expect("write");
        String::from_utf8_lossy(&req).into_owned()
    });

    let dir = temp_dir("e2e-success");
    let mut cmd = ncheap(&dir);
    fake_creds(&mut cmd);
    cmd.env("NCHEAP_ENDPOINT", format!("http://{addr}/xml.response"));
    let output = cmd
        .args(["domains", "check", "zq9probe.com", "--json"])
        .output()
        .expect("run");
    let request = server.join().expect("server thread");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON envelope");
    assert_eq!(v["ok"], true);
    assert_eq!(v["schema"], 3);
    assert_eq!(v["command"], "domains.check");
    assert_eq!(v["data"][0]["domain"], "zq9probe.com");
    assert_eq!(v["data"][0]["available"], true);
    assert_eq!(v["meta"]["api_calls"], 1);
    assert_eq!(v["meta"]["version"], env!("CARGO_PKG_VERSION"));

    let request_line = request.lines().next().expect("request line");
    assert!(
        request_line.starts_with("POST "),
        "must POST: {request_line}"
    );
    assert!(
        !request_line.contains("ApiKey"),
        "credentials must not travel in the URL: {request_line}"
    );
    assert!(request.contains("ApiKey=k"), "key travels in the form body");
}
