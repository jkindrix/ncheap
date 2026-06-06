use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

use super::Error;

#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    #[serde(rename = "@Status")]
    pub status: String,
    #[serde(rename = "Errors", default)]
    pub errors: ApiErrors,
    #[serde(rename = "CommandResponse")]
    pub command_response: Option<T>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ApiErrors {
    #[serde(rename = "Error", default)]
    pub errors: Vec<ApiError>,
}

#[derive(Debug, Deserialize)]
pub struct ApiError {
    #[serde(rename = "@Number", default)]
    pub number: String,
    #[serde(rename = "$text", default)]
    pub message: String,
}

/// Parse a raw XML body into the command-response payload, mapping the
/// `ApiResponse Status` envelope to Ok/Err.
///
/// Two passes: error responses can carry a junk CommandResponse element
/// (observed live: `<DomainDNSSetCustomResult Domain="" Updated="" />`
/// inside Status="ERROR"), so the typed payload parse must not run — and
/// fail, masking the real API error — before the status check.
pub fn parse<T: DeserializeOwned>(body: &str) -> Result<T, Error> {
    let envelope: ApiResponse<serde::de::IgnoredAny> =
        quick_xml::de::from_str(body).map_err(|e| Error::Parse(format!("malformed XML: {e}")))?;
    if envelope.status.eq_ignore_ascii_case("ok") {
        let resp: ApiResponse<T> = quick_xml::de::from_str(body)
            .map_err(|e| Error::Parse(format!("malformed XML: {e}")))?;
        resp.command_response
            .ok_or_else(|| Error::Parse("Status=OK but no CommandResponse element".into()))
    } else {
        let resp = envelope;
        let (code, mut message) = resp
            .errors
            .errors
            .first()
            .map(|e| (e.number.clone(), e.message.trim().to_owned()))
            .unwrap_or_else(|| ("unknown".into(), "no error detail in response".into()));
        if code == "1011150" {
            // Documented as "Parameter RequestIP is invalid"; in practice
            // this often accompanies IP-whitelist problems.
            message.push_str(
                " — this often accompanies IP-whitelist problems: check the client_ip \
                 config value, your current outbound IPv4, and the Namecheap whitelist",
            );
        }
        // The API rate-limits inside HTTP 200 (third-party-observed error
        // 500000, not in primary docs); map it to the rate_limit kind so
        // agents get a back-off signal instead of an ordinary API error.
        if code == "500000" {
            return Err(Error::RateLimited(format!("API error 500000: {message}")));
        }
        Err(Error::Api { code, message })
    }
}

/// Namecheap booleans arrive as text and are not consistently cased across
/// methods ("false", "True"); parse case-insensitively.
pub fn de_bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let s = String::deserialize(d)?;
    match s.to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        other => Err(serde::de::Error::custom(format!(
            "expected boolean, got {other:?}"
        ))),
    }
}
