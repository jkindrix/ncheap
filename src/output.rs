use serde::Serialize;
use serde_json::json;

use crate::api::Error;
use crate::config::Profile;

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
        let envelope = json!({
            "ok": true,
            "command": command,
            "data": data,
            "error": null,
            "meta": {
                "profile": profile.name,
                "sandbox": profile.sandbox,
                "api_calls": api_calls,
            },
        });
        println!("{envelope}");
    } else {
        human();
    }
}

/// Emit the failure envelope on stdout (JSON mode) or a line on stderr.
/// The process exit code is handled by the caller via Error::exit_code.
pub fn failure(json_mode: bool, command: &str, err: &Error) {
    if json_mode {
        let envelope = json!({
            "ok": false,
            "command": command,
            "data": null,
            "error": {
                "kind": err.kind(),
                "code": err.code(),
                "message": err.to_string(),
            },
            "meta": null,
        });
        println!("{envelope}");
    } else {
        eprintln!("error: {err}");
    }
}
