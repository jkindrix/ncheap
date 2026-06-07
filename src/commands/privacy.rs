use serde::{Deserialize, Serialize};

use crate::api::xml::{self, de_bool, de_date};
use crate::api::{Client, Error, Transport};

#[derive(Debug, Deserialize)]
pub struct GetListResponse {
    #[serde(rename = "WhoisguardGetListResult", default)]
    result: PrivacyListXml,
    #[serde(rename = "Paging")]
    paging: Paging,
}

#[derive(Debug, Default, Deserialize)]
struct PrivacyListXml {
    #[serde(rename = "Whoisguard", default)]
    subscriptions: Vec<PrivacySubscription>,
}

#[derive(Debug, Deserialize)]
struct Paging {
    #[serde(rename = "TotalItems")]
    total_items: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PrivacySubscription {
    #[serde(rename(deserialize = "@ID", serialize = "id"))]
    pub id: String,
    #[serde(
        rename(deserialize = "@DomainName", serialize = "domain_name"),
        default
    )]
    pub domain_name: String,
    #[serde(
        rename(deserialize = "@Created", serialize = "created"),
        default,
        deserialize_with = "de_date"
    )]
    pub created: String,
    #[serde(
        rename(deserialize = "@Expires", serialize = "expires"),
        default,
        deserialize_with = "de_date"
    )]
    pub expires: String,
    #[serde(rename(deserialize = "@Status", serialize = "status"), default)]
    pub status: String,
}

/// Fetch ALL privacy subscriptions, following pagination like domains::list.
pub fn list<T: Transport>(client: &Client<T>) -> Result<Vec<PrivacySubscription>, Error> {
    let mut subs: Vec<PrivacySubscription> = Vec::new();
    let mut page = 1usize;
    loop {
        let body = client.call(
            "whoisguard.getList",
            &[("Page", page.to_string().as_str()), ("PageSize", "100")],
        )?;
        let resp: GetListResponse = xml::parse(&body)?;
        let before = subs.len();
        subs.extend(resp.result.subscriptions);
        if subs.len() >= resp.paging.total_items {
            return Ok(subs);
        }
        if subs.len() == before {
            return Err(Error::Parse(format!(
                "pagination stalled: server reports {} total items but page {page} added none",
                resp.paging.total_items
            )));
        }
        // Same bound as domains::list: TotalItems is server-controlled.
        if page >= 100 {
            return Err(Error::Parse(
                "pagination overflow: 100 pages fetched without completing the listing".into(),
            ));
        }
        page += 1;
    }
}

/// The enable/disable API takes a WhoisguardID, not a domain; resolve it
/// from the subscription list so callers can speak in domains.
fn resolve_id<T: Transport>(client: &Client<T>, domain: &str) -> Result<String, Error> {
    let normalized = crate::domain::normalize(domain)?;
    let subs = list(client)?;
    subs.iter()
        .find(|s| s.domain_name.eq_ignore_ascii_case(&normalized))
        .map(|s| s.id.clone())
        .ok_or_else(|| {
            Error::Usage(format!(
                "no domain privacy subscription is associated with {normalized}"
            ))
        })
}

#[derive(Debug, Deserialize)]
pub struct EnableResponse {
    #[serde(rename = "WhoisguardEnableResult")]
    result: ToggleXml,
}

#[derive(Debug, Deserialize)]
pub struct DisableResponse {
    #[serde(rename = "WhoisguardDisableResult")]
    result: ToggleXml,
}

#[derive(Debug, Deserialize)]
struct ToggleXml {
    #[serde(rename = "@DomainName", default)]
    domain_name: String,
    #[serde(rename = "@IsSuccess", deserialize_with = "de_bool", default)]
    is_success: bool,
}

#[derive(Debug, Serialize)]
pub struct ToggleResult {
    pub domain: String,
    pub privacy_id: String,
    pub enabled: bool,
    pub is_success: bool,
}

/// Enable privacy (mutating). `forward_to` is required by the API and
/// deliberately never defaulted by ncheap.
pub fn enable<T: Transport>(
    client: &Client<T>,
    domain: &str,
    forward_to: &str,
) -> Result<ToggleResult, Error> {
    client.require_mutations_permitted()?;
    let id = resolve_id(client, domain)?;
    let body = client.call_mut(
        "whoisguard.enable",
        &[
            ("WhoisguardID", id.as_str()),
            ("ForwardedToEmail", forward_to),
        ],
    )?;
    let resp: EnableResponse = xml::parse(&body)?;
    Ok(ToggleResult {
        domain: resp.result.domain_name,
        privacy_id: id,
        enabled: true,
        is_success: resp.result.is_success,
    })
}

/// Disable privacy (mutating).
pub fn disable<T: Transport>(client: &Client<T>, domain: &str) -> Result<ToggleResult, Error> {
    client.require_mutations_permitted()?;
    let id = resolve_id(client, domain)?;
    let body = client.call_mut("whoisguard.disable", &[("WhoisguardID", id.as_str())])?;
    let resp: DisableResponse = xml::parse(&body)?;
    Ok(ToggleResult {
        domain: resp.result.domain_name,
        privacy_id: id,
        enabled: false,
        is_success: resp.result.is_success,
    })
}

pub fn render_toggle(result: &ToggleResult) {
    println!(
        "{}: privacy {} ({})",
        result.domain,
        if result.enabled {
            "enabled"
        } else {
            "disabled"
        },
        if result.is_success {
            "success"
        } else {
            "NOT successful"
        },
    );
}

pub fn render_table(subs: &[PrivacySubscription]) {
    println!(
        "{:<12} {:<40} {:<12} {:<12} STATUS",
        "ID", "DOMAIN", "CREATED", "EXPIRES"
    );
    for s in subs {
        println!(
            "{:<12} {:<40} {:<12} {:<12} {}",
            s.id,
            if s.domain_name.is_empty() {
                "-"
            } else {
                &s.domain_name
            },
            s.created,
            s.expires,
            s.status,
        );
    }
}
