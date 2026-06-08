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

/// Namecheap dates arrive as "MM/DD/YYYY" (sometimes single-digit fields,
/// sometimes a trailing time) — a format agents string-sort wrong. Schema 3
/// normalizes envelope dates to ISO-8601 (YYYY-MM-DD). Unrecognized values
/// pass through verbatim: dates are display data, not safety data, so an
/// odd format must not fail a whole listing.
pub fn de_date<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    Ok(to_iso_date(&s))
}

pub fn to_iso_date(s: &str) -> String {
    let date_part = s.split_whitespace().next().unwrap_or(s);
    let parts: Vec<&str> = date_part.split('/').collect();
    if parts.len() == 3
        && let (Ok(m), Ok(d), Ok(y)) = (
            parts[0].parse::<u32>(),
            parts[1].parse::<u32>(),
            parts[2].parse::<u32>(),
        )
        && (1..=12).contains(&m)
        && y >= 1000
        && (1..=days_in_month(y, m)).contains(&d)
    {
        return format!("{y:04}-{m:02}-{d:02}");
    }
    s.to_owned()
}

/// Days in a Gregorian month, so an impossible day (02/30) passes through
/// verbatim like any other unrecognized value rather than being "normalized"
/// into a fictitious ISO date. `month` is range-checked by the caller.
fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) => {
            29
        }
        2 => 28,
        _ => 0,
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

#[cfg(test)]
mod tests {
    use super::to_iso_date;

    #[test]
    fn dates_normalize_to_iso_8601() {
        assert_eq!(to_iso_date("08/20/2026"), "2026-08-20");
        assert_eq!(to_iso_date("4/30/2021"), "2021-04-30");
        assert_eq!(to_iso_date("4/30/2021 11:31:13 AM"), "2021-04-30");
    }

    #[test]
    fn unrecognized_dates_pass_through_verbatim() {
        for odd in [
            "",
            "2026-08-20",
            "13/40/2026",
            "soon",
            "1/2",
            "02/30/2026",
            "02/29/2025",
        ] {
            assert_eq!(to_iso_date(odd), odd, "must not mangle {odd:?}");
        }
        // A real leap day still normalizes.
        assert_eq!(to_iso_date("02/29/2024"), "2024-02-29");
    }
}
