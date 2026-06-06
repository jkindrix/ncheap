use serde::Serialize;
use serde_json::{Value, json};

use crate::api::Error;
use crate::config::Profile;

/// Build the success envelope; separated from printing so the schema —
/// the machine contract — is unit-testable.
pub fn success_envelope<T: Serialize>(
    command: &str,
    data: &T,
    profile: &Profile,
    api_calls: u32,
) -> Value {
    json!({
        "ok": true,
        "command": command,
        "data": data,
        "error": null,
        "meta": {
            "profile": profile.name,
            "sandbox": profile.sandbox,
            "api_calls": api_calls,
        },
    })
}

/// Build the failure envelope; `meta` is null because a failure may occur
/// before a profile resolves.
pub fn failure_envelope(command: &str, err: &Error) -> Value {
    json!({
        "ok": false,
        "command": command,
        "data": null,
        "error": {
            "kind": err.kind(),
            "code": err.code(),
            "message": err.to_string(),
        },
        "meta": null,
    })
}

/// Emit the success envelope (JSON mode) or invoke the human renderer.
pub fn success<T: Serialize>(
    json_mode: bool,
    command: &str,
    data: &T,
    profile: &Profile,
    api_calls: u32,
    human: impl FnOnce(),
) {
    if json_mode {
        println!("{}", success_envelope(command, data, profile, api_calls));
    } else {
        human();
    }
}

/// Emit the failure envelope on stdout (JSON mode) or a line on stderr.
/// The process exit code is handled by the caller via Error::exit_code.
pub fn failure(json_mode: bool, command: &str, err: &Error) {
    if json_mode {
        println!("{}", failure_envelope(command, err));
    } else {
        eprintln!("error: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Secret;

    fn profile() -> Profile {
        Profile {
            name: "test".into(),
            api_user: "u".into(),
            api_key: Secret::new("k".into()),
            username: "u".into(),
            client_ip: "192.0.2.1".into(),
            sandbox: true,
            allow_production_mutations: false,
        }
    }

    #[test]
    fn success_envelope_schema() {
        let v = success_envelope("domains.list", &vec!["a", "b"], &profile(), 2);
        assert_eq!(v["ok"], true);
        assert_eq!(v["command"], "domains.list");
        assert_eq!(v["data"].as_array().unwrap().len(), 2);
        assert!(v["error"].is_null());
        assert_eq!(v["meta"]["profile"], "test");
        assert_eq!(v["meta"]["sandbox"], true);
        assert_eq!(v["meta"]["api_calls"], 2);
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["command", "data", "error", "meta", "ok"]);
    }

    #[test]
    fn failure_envelope_schema() {
        let err = Error::Api {
            code: "1011150".into(),
            message: "denied".into(),
        };
        let v = failure_envelope("domains.list", &err);
        assert_eq!(v["ok"], false);
        assert!(v["data"].is_null());
        assert!(v["meta"].is_null());
        assert_eq!(v["error"]["kind"], "api");
        assert_eq!(v["error"]["code"], "1011150");
        assert!(v["error"]["message"].as_str().unwrap().contains("1011150"));
    }

    #[test]
    fn failure_envelope_code_is_null_for_non_api_errors() {
        let v = failure_envelope("raw", &Error::Usage("bad".into()));
        assert_eq!(v["error"]["kind"], "usage");
        assert!(v["error"]["code"].is_null());
    }
}
