use crate::api::xml;
use crate::api::{Client, Error, READ_ONLY_COMMANDS, Transport, canonical_command};

/// Global params the client sets itself; user-supplied copies are rejected
/// rather than appended (a duplicate Command param could bypass the
/// allowlist, a duplicate ApiKey could leak into history).
const RESERVED_PARAMS: &[&str] = &["apiuser", "apikey", "username", "command", "clientip"];

/// Parse repeated --param KEY=VALUE arguments.
pub fn parse_params(raw: &[String]) -> Result<Vec<(String, String)>, Error> {
    raw.iter()
        .map(|p| {
            let (k, v) = p
                .split_once('=')
                .ok_or_else(|| Error::Usage(format!("--param {p:?} is not KEY=VALUE")))?;
            if RESERVED_PARAMS.contains(&k.to_ascii_lowercase().as_str()) {
                return Err(Error::Usage(format!(
                    "--param {k} is reserved: authentication parameters are set from the profile"
                )));
            }
            Ok((k.to_owned(), v.to_owned()))
        })
        .collect()
}

/// Call an allowlisted read-only method and return the raw XML body.
/// An ApiResponse Status=ERROR still maps to Error::Api (exit 1).
pub fn call<T: Transport>(
    client: &Client<T>,
    command: &str,
    params: &[(String, String)],
) -> Result<String, Error> {
    // Client::call enforces the same list fail-closed; checking here too
    // gives raw's callers a usage error (exit 2) with the full list.
    let canonical = canonical_command(command);
    if !READ_ONLY_COMMANDS.contains(&canonical.as_str()) {
        return Err(Error::Usage(format!(
            "{command:?} is not on the read-only allowlist; allowed: {}",
            READ_ONLY_COMMANDS.join(", ")
        )));
    }
    let params: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let body = client.call(&canonical, &params)?;
    // Surface API errors as errors even though the payload is opaque here.
    let _: serde::de::IgnoredAny = xml::parse(&body)?;
    Ok(body)
}
