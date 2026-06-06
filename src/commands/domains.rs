use serde::{Deserialize, Serialize};

use crate::api::xml::{self, de_bool};
use crate::api::{Client, Error, Transport};

#[derive(Debug, Deserialize)]
pub struct GetListResponse {
    #[serde(rename = "DomainGetListResult", default)]
    result: DomainListResult,
    #[serde(rename = "Paging")]
    paging: Paging,
}

#[derive(Debug, Default, Deserialize)]
struct DomainListResult {
    #[serde(rename = "Domain", default)]
    domains: Vec<Domain>,
}

#[derive(Debug, Deserialize)]
struct Paging {
    #[serde(rename = "TotalItems")]
    total_items: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Domain {
    #[serde(rename(deserialize = "@ID", serialize = "id"))]
    pub id: String,
    #[serde(rename(deserialize = "@Name", serialize = "name"))]
    pub name: String,
    #[serde(rename(deserialize = "@User", serialize = "user"))]
    pub user: String,
    #[serde(rename(deserialize = "@Created", serialize = "created"))]
    pub created: String,
    #[serde(rename(deserialize = "@Expires", serialize = "expires"))]
    pub expires: String,
    #[serde(
        rename(deserialize = "@IsExpired", serialize = "is_expired"),
        deserialize_with = "de_bool"
    )]
    pub is_expired: bool,
    #[serde(
        rename(deserialize = "@IsLocked", serialize = "is_locked"),
        deserialize_with = "de_bool"
    )]
    pub is_locked: bool,
    #[serde(
        rename(deserialize = "@AutoRenew", serialize = "auto_renew"),
        deserialize_with = "de_bool"
    )]
    pub auto_renew: bool,
    #[serde(rename(deserialize = "@WhoisGuard", serialize = "privacy"))]
    pub privacy: String,
    #[serde(
        rename(deserialize = "@IsPremium", serialize = "is_premium"),
        deserialize_with = "de_bool"
    )]
    pub is_premium: bool,
    #[serde(
        rename(deserialize = "@IsOurDNS", serialize = "is_our_dns"),
        deserialize_with = "de_bool"
    )]
    pub is_our_dns: bool,
}

/// Fetch ALL domains, following pagination. The API's default PageSize of 20
/// silently truncates accounts with more domains — request 100 and follow
/// Paging/TotalItems until complete.
pub fn list<T: Transport>(client: &Client<T>) -> Result<Vec<Domain>, Error> {
    let mut domains: Vec<Domain> = Vec::new();
    let mut page = 1usize;
    loop {
        let body = client.call(
            "domains.getList",
            &[
                ("Page", page.to_string().as_str()),
                ("PageSize", "100"),
                ("SortBy", "NAME"),
            ],
        )?;
        let resp: GetListResponse = xml::parse(&body)?;
        let before = domains.len();
        domains.extend(resp.result.domains);
        if domains.len() >= resp.paging.total_items {
            return Ok(domains);
        }
        if domains.len() == before {
            return Err(Error::Parse(format!(
                "pagination stalled: server reports {} total items but page {page} added none",
                resp.paging.total_items
            )));
        }
        page += 1;
    }
}

#[derive(Debug, Deserialize)]
pub struct CheckResponse {
    #[serde(rename = "DomainCheckResult", default)]
    results: Vec<CheckResult>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CheckResult {
    #[serde(rename(deserialize = "@Domain", serialize = "domain"))]
    pub domain: String,
    #[serde(
        rename(deserialize = "@Available", serialize = "available"),
        deserialize_with = "de_bool"
    )]
    pub available: bool,
    #[serde(
        rename(deserialize = "@IsPremiumName", serialize = "is_premium"),
        deserialize_with = "de_bool",
        default
    )]
    pub is_premium: bool,
    #[serde(
        rename(
            deserialize = "@PremiumRegistrationPrice",
            serialize = "premium_registration_price"
        ),
        default
    )]
    pub premium_registration_price: String,
    #[serde(
        rename(
            deserialize = "@PremiumRenewalPrice",
            serialize = "premium_renewal_price"
        ),
        default
    )]
    pub premium_renewal_price: String,
    #[serde(
        rename(
            deserialize = "@PremiumRestorePrice",
            serialize = "premium_restore_price"
        ),
        default
    )]
    pub premium_restore_price: String,
    #[serde(
        rename(
            deserialize = "@PremiumTransferPrice",
            serialize = "premium_transfer_price"
        ),
        default
    )]
    pub premium_transfer_price: String,
    #[serde(rename(deserialize = "@IcannFee", serialize = "icann_fee"), default)]
    pub icann_fee: String,
    #[serde(rename(deserialize = "@EapFee", serialize = "eap_fee"), default)]
    pub eap_fee: String,
    #[serde(rename(deserialize = "@ErrorNo", serialize = "error_no"), default)]
    pub error_no: String,
    #[serde(
        rename(deserialize = "@Description", serialize = "description"),
        default
    )]
    pub description: String,
}

/// Check availability. Unavailable domains are data, not errors. The API
/// caps a single call at 50 domains (error 2011169); not chunked here.
pub fn check<T: Transport>(
    client: &Client<T>,
    domains: &[String],
) -> Result<Vec<CheckResult>, Error> {
    let list = domains.join(",");
    let body = client.call("domains.check", &[("DomainList", &list)])?;
    let resp: CheckResponse = xml::parse(&body)?;
    Ok(resp.results)
}

#[derive(Debug, Deserialize)]
pub struct LockResponse {
    #[serde(rename = "DomainGetRegistrarLockResult")]
    result: LockStatus,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LockStatus {
    #[serde(rename(deserialize = "@Domain", serialize = "domain"))]
    pub domain: String,
    #[serde(
        rename(deserialize = "@RegistrarLockStatus", serialize = "locked"),
        deserialize_with = "de_bool"
    )]
    pub locked: bool,
}

/// Read-only registrar lock status (namecheap.domains.getRegistrarLock).
pub fn lock_status<T: Transport>(client: &Client<T>, domain: &str) -> Result<LockStatus, Error> {
    let body = client.call("domains.getRegistrarLock", &[("DomainName", domain)])?;
    let resp: LockResponse = xml::parse(&body)?;
    Ok(resp.result)
}

pub fn render_check(results: &[CheckResult]) {
    println!(
        "{:<40} {:<10} {:<8} PRICE",
        "DOMAIN", "AVAILABLE", "PREMIUM"
    );
    for r in results {
        println!(
            "{:<40} {:<10} {:<8} {}",
            r.domain,
            if r.available { "yes" } else { "no" },
            if r.is_premium { "yes" } else { "no" },
            if r.is_premium {
                r.premium_registration_price.as_str()
            } else {
                "-"
            },
        );
    }
}

pub fn render_lock(status: &LockStatus) {
    println!(
        "{}: registrar lock {}",
        status.domain,
        if status.locked { "on" } else { "off" }
    );
}

pub fn render_table(domains: &[Domain]) {
    println!(
        "{:<40} {:<12} {:<6} {:<6} {:<12} {:<7}",
        "NAME", "EXPIRES", "LOCK", "RENEW", "PRIVACY", "OURDNS"
    );
    for d in domains {
        println!(
            "{:<40} {:<12} {:<6} {:<6} {:<12} {:<7}",
            d.name,
            d.expires,
            if d.is_locked { "yes" } else { "no" },
            if d.auto_renew { "yes" } else { "no" },
            d.privacy,
            if d.is_our_dns { "yes" } else { "no" },
        );
    }
}
