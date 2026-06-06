use serde::Serialize;
use serde_json::{Value, json};

use crate::api::Error;
use crate::config::Profile;

/// Envelope schema version. v0.1.0 shipped an unversioned envelope
/// (retroactively schema 1); schema 2 added this field, meta.version,
/// meta on failures, and the parse error kind.
pub const SCHEMA: u32 = 2;

fn meta(profile: &Profile, api_calls: u32) -> Value {
    json!({
        "profile": profile.name,
        "sandbox": profile.sandbox,
        "api_calls": api_calls,
        "version": env!("CARGO_PKG_VERSION"),
    })
}

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
        "schema": SCHEMA,
        "command": command,
        "data": data,
        "error": null,
        "meta": meta(profile, api_calls),
    })
}

/// Build the failure envelope. `meta` is populated when the profile had
/// resolved before the failure (so a fleet operator can tell which
/// profile/sandbox a failed call targeted) and null otherwise.
pub fn failure_envelope(command: &str, err: &Error, resolved: Option<(&Profile, u32)>) -> Value {
    json!({
        "ok": false,
        "schema": SCHEMA,
        "command": command,
        "data": null,
        "error": {
            "kind": err.kind(),
            "code": err.code(),
            "message": err.to_string(),
        },
        "meta": resolved.map(|(p, calls)| meta(p, calls)),
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
pub fn failure(json_mode: bool, command: &str, err: &Error, resolved: Option<(&Profile, u32)>) {
    if json_mode {
        println!("{}", failure_envelope(command, err, resolved));
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
            endpoint_override: None,
        }
    }

    #[test]
    fn success_envelope_schema() {
        let v = success_envelope("domains.list", &vec!["a", "b"], &profile(), 2);
        assert_eq!(v["ok"], true);
        assert_eq!(v["schema"], SCHEMA);
        assert_eq!(v["command"], "domains.list");
        assert_eq!(v["data"].as_array().unwrap().len(), 2);
        assert!(v["error"].is_null());
        assert_eq!(v["meta"]["profile"], "test");
        assert_eq!(v["meta"]["sandbox"], true);
        assert_eq!(v["meta"]["api_calls"], 2);
        assert_eq!(v["meta"]["version"], env!("CARGO_PKG_VERSION"));
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["command", "data", "error", "meta", "ok", "schema"]);
    }

    #[test]
    fn failure_envelope_schema_with_and_without_meta() {
        let err = Error::Api {
            code: "1011150".into(),
            message: "denied".into(),
        };
        let p = profile();
        let v = failure_envelope("domains.list", &err, Some((&p, 3)));
        assert_eq!(v["ok"], false);
        assert_eq!(v["schema"], SCHEMA);
        assert!(v["data"].is_null());
        assert_eq!(v["error"]["kind"], "api");
        assert_eq!(v["error"]["code"], "1011150");
        assert_eq!(v["meta"]["profile"], "test", "meta present once resolved");
        assert_eq!(v["meta"]["api_calls"], 3);

        let v = failure_envelope("domains.list", &err, None);
        assert!(v["meta"].is_null(), "meta null before profile resolution");
    }

    #[test]
    fn parse_and_rate_limit_kinds_are_distinct() {
        let v = failure_envelope("raw", &Error::Parse("bad xml".into()), None);
        assert_eq!(v["error"]["kind"], "parse");
        let v = failure_envelope(
            "raw",
            &Error::RateLimited("API error 500000: Too many requests".into()),
            None,
        );
        assert_eq!(v["error"]["kind"], "rate_limit");
        assert!(v["error"]["code"].is_null());
    }
}
