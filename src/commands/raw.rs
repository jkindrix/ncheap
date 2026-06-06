use crate::api::xml;
use crate::api::{Client, Error, Transport};

/// The only methods `raw` will call: every read-only method Phase 1 wraps,
/// plus domains.getTldList. Mutating methods stay out until the sandbox-gated
/// write path exists. Lowercase canonical, no "namecheap." prefix.
const READ_ONLY_ALLOWLIST: &[&str] = &[
    "domains.getlist",
    "domains.check",
    "domains.getregistrarlock",
    "domains.getinfo",
    "domains.getcontacts",
    "domains.gettldlist",
    "domains.dns.getlist",
    "domains.dns.gethosts",
    "whoisguard.getlist",
    "users.getbalances",
    "users.getpricing",
];

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
    let canonical = command
        .strip_prefix("namecheap.")
        .or_else(|| command.strip_prefix("Namecheap."))
        .unwrap_or(command)
        .to_ascii_lowercase();
    if !READ_ONLY_ALLOWLIST.contains(&canonical.as_str()) {
        return Err(Error::Usage(format!(
            "{command:?} is not on the read-only allowlist; allowed: {}",
            READ_ONLY_ALLOWLIST.join(", ")
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
