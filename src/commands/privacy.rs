use serde::{Deserialize, Serialize};

use crate::api::xml;
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
    #[serde(rename(deserialize = "@Created", serialize = "created"), default)]
    pub created: String,
    #[serde(rename(deserialize = "@Expires", serialize = "expires"), default)]
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
        page += 1;
    }
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
