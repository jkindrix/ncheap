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
        // TotalItems is server-controlled; it must not drive an unbounded
        // loop. 100 pages × PageSize 100 = 10,000 items, far past any
        // realistic account.
        if page >= 100 {
            return Err(Error::Parse(
                "pagination overflow: 100 pages fetched without completing the listing".into(),
            ));
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
/// caps a single call at 50 domains; rejected up front as a usage error
/// rather than surfaced as a server round-trip.
pub fn check<T: Transport>(
    client: &Client<T>,
    domains: &[String],
) -> Result<Vec<CheckResult>, Error> {
    if domains.len() > 50 {
        return Err(Error::Usage(format!(
            "domains check accepts at most 50 domains per call (got {})",
            domains.len()
        )));
    }
    let normalized: Vec<String> = domains
        .iter()
        .map(|d| crate::domain::normalize(d))
        .collect::<Result<_, _>>()?;
    let list = normalized.join(",");
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
    let domain = crate::domain::normalize(domain)?;
    let body = client.call("domains.getRegistrarLock", &[("DomainName", &domain)])?;
    let resp: LockResponse = xml::parse(&body)?;
    Ok(resp.result)
}

#[derive(Debug, Deserialize)]
pub struct GetInfoResponse {
    #[serde(rename = "DomainGetInfoResult")]
    result: InfoXml,
}

#[derive(Debug, Deserialize)]
struct InfoXml {
    #[serde(rename = "@Status")]
    status: String,
    #[serde(rename = "@ID")]
    id: String,
    #[serde(rename = "@DomainName")]
    domain_name: String,
    #[serde(rename = "@OwnerName", default)]
    owner_name: String,
    #[serde(rename = "@IsOwner", deserialize_with = "de_bool", default)]
    is_owner: bool,
    #[serde(rename = "@IsPremium", deserialize_with = "de_bool", default)]
    is_premium: bool,
    #[serde(rename = "DomainDetails", default)]
    details: InfoDetailsXml,
    #[serde(rename = "Whoisguard")]
    whoisguard: Option<WhoisguardXml>,
    #[serde(rename = "DnsDetails")]
    dns_details: Option<DnsDetailsXml>,
    #[serde(rename = "Modificationrights")]
    modification_rights: Option<ModRightsXml>,
}

#[derive(Debug, Default, Deserialize)]
struct InfoDetailsXml {
    #[serde(rename = "CreatedDate", default)]
    created: String,
    #[serde(rename = "ExpiredDate", default)]
    expires: String,
}

#[derive(Debug, Deserialize)]
struct WhoisguardXml {
    /// Kept as a string: documented values are True/False but the privacy
    /// provider migration note suggests other states may appear.
    #[serde(rename = "@Enabled", default)]
    enabled: String,
    #[serde(rename = "ID", default)]
    id: String,
    #[serde(rename = "ExpiredDate", default)]
    expires: String,
}

#[derive(Debug, Deserialize)]
struct DnsDetailsXml {
    #[serde(rename = "@ProviderType", default)]
    provider_type: String,
    #[serde(rename = "Nameserver", default)]
    nameservers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ModRightsXml {
    #[serde(rename = "@All", deserialize_with = "de_bool", default)]
    all: bool,
}

#[derive(Debug, Serialize)]
pub struct DomainInfo {
    pub domain: String,
    pub id: String,
    pub status: String,
    pub owner: String,
    pub is_owner: bool,
    pub is_premium: bool,
    pub created: String,
    pub expires: String,
    pub privacy: Option<PrivacyInfo>,
    pub dns_provider: Option<String>,
    pub nameservers: Vec<String>,
    pub modification_rights_all: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct PrivacyInfo {
    pub enabled: String,
    pub id: String,
    pub expires: String,
}

pub fn info<T: Transport>(client: &Client<T>, domain: &str) -> Result<DomainInfo, Error> {
    let domain = crate::domain::normalize(domain)?;
    let body = client.call("domains.getInfo", &[("DomainName", &domain)])?;
    let resp: GetInfoResponse = xml::parse(&body)?;
    let r = resp.result;
    Ok(DomainInfo {
        domain: r.domain_name,
        id: r.id,
        status: r.status,
        owner: r.owner_name,
        is_owner: r.is_owner,
        is_premium: r.is_premium,
        created: r.details.created,
        expires: r.details.expires,
        privacy: r.whoisguard.map(|w| PrivacyInfo {
            enabled: w.enabled,
            id: w.id,
            expires: w.expires,
        }),
        dns_provider: r.dns_details.as_ref().map(|d| d.provider_type.clone()),
        nameservers: r.dns_details.map(|d| d.nameservers).unwrap_or_default(),
        modification_rights_all: r.modification_rights.map(|m| m.all),
    })
}

#[derive(Debug, Deserialize)]
pub struct GetContactsResponse {
    #[serde(rename = "DomainContactsResult")]
    result: ContactsXml,
}

#[derive(Debug, Deserialize)]
struct ContactsXml {
    #[serde(rename = "@Domain")]
    domain: String,
    #[serde(rename = "Registrant")]
    registrant: Contact,
    #[serde(rename = "Tech")]
    tech: Contact,
    #[serde(rename = "Admin")]
    admin: Contact,
    #[serde(rename = "AuxBilling")]
    aux_billing: Contact,
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Contact {
    #[serde(
        rename(deserialize = "@ReadOnly", serialize = "read_only"),
        deserialize_with = "de_bool",
        default
    )]
    pub read_only: bool,
    #[serde(
        rename(deserialize = "OrganizationName", serialize = "organization_name"),
        default
    )]
    pub organization_name: String,
    #[serde(rename(deserialize = "JobTitle", serialize = "job_title"), default)]
    pub job_title: String,
    #[serde(rename(deserialize = "FirstName", serialize = "first_name"), default)]
    pub first_name: String,
    #[serde(rename(deserialize = "LastName", serialize = "last_name"), default)]
    pub last_name: String,
    #[serde(rename(deserialize = "Address1", serialize = "address1"), default)]
    pub address1: String,
    #[serde(rename(deserialize = "Address2", serialize = "address2"), default)]
    pub address2: String,
    #[serde(rename(deserialize = "City", serialize = "city"), default)]
    pub city: String,
    #[serde(
        rename(deserialize = "StateProvince", serialize = "state_province"),
        default
    )]
    pub state_province: String,
    #[serde(rename(deserialize = "PostalCode", serialize = "postal_code"), default)]
    pub postal_code: String,
    #[serde(rename(deserialize = "Country", serialize = "country"), default)]
    pub country: String,
    #[serde(rename(deserialize = "Phone", serialize = "phone"), default)]
    pub phone: String,
    #[serde(rename(deserialize = "PhoneExt", serialize = "phone_ext"), default)]
    pub phone_ext: String,
    #[serde(rename(deserialize = "Fax", serialize = "fax"), default)]
    pub fax: String,
    #[serde(
        rename(deserialize = "EmailAddress", serialize = "email_address"),
        default
    )]
    pub email_address: String,
}

#[derive(Debug, Serialize)]
pub struct Contacts {
    pub domain: String,
    pub registrant: Contact,
    pub tech: Contact,
    pub admin: Contact,
    pub aux_billing: Contact,
}

pub fn contacts<T: Transport>(client: &Client<T>, domain: &str) -> Result<Contacts, Error> {
    let domain = crate::domain::normalize(domain)?;
    let body = client.call("domains.getContacts", &[("DomainName", &domain)])?;
    let resp: GetContactsResponse = xml::parse(&body)?;
    let r = resp.result;
    Ok(Contacts {
        domain: r.domain,
        registrant: r.registrant,
        tech: r.tech,
        admin: r.admin,
        aux_billing: r.aux_billing,
    })
}

/// Contact details are PII: the default view carries only the audit signals
/// (per-set country/state, read-only flags, and whether all four contact
/// sets match); --full opts into the actual fields.
pub fn contacts_redacted_view(c: &Contacts) -> serde_json::Value {
    let identical = c.registrant == c.tech && c.tech == c.admin && c.admin == c.aux_billing;
    serde_json::json!({
        "domain": c.domain,
        "all_contact_sets_identical": identical,
        "registrant": contact_summary(&c.registrant),
        "tech": contact_summary(&c.tech),
        "admin": contact_summary(&c.admin),
        "aux_billing": contact_summary(&c.aux_billing),
    })
}

fn contact_summary(c: &Contact) -> serde_json::Value {
    serde_json::json!({
        "country": c.country,
        "state_province": c.state_province,
        "read_only": c.read_only,
    })
}

pub fn render_info(info: &DomainInfo) {
    println!("domain: {}", info.domain);
    println!("status: {}", info.status);
    println!("owner: {} (is_owner: {})", info.owner, info.is_owner);
    println!("created: {}", info.created);
    println!("expires: {}", info.expires);
    println!("premium: {}", if info.is_premium { "yes" } else { "no" });
    match &info.privacy {
        Some(p) => println!("privacy: {} (expires {})", p.enabled, p.expires),
        None => println!("privacy: not reported"),
    }
    if let Some(provider) = &info.dns_provider {
        println!("dns_provider: {provider}");
    }
    for ns in &info.nameservers {
        println!("nameserver: {ns}");
    }
    if let Some(all) = info.modification_rights_all {
        println!("modification_rights_all: {all}");
    }
}

pub fn render_contacts(c: &Contacts, full: bool) {
    println!("domain: {}", c.domain);
    let sets: [(&str, &Contact); 4] = [
        ("registrant", &c.registrant),
        ("tech", &c.tech),
        ("admin", &c.admin),
        ("aux_billing", &c.aux_billing),
    ];
    if full {
        for (label, contact) in sets {
            println!("[{label}]");
            println!("  name: {} {}", contact.first_name, contact.last_name);
            if !contact.organization_name.is_empty() {
                println!("  organization: {}", contact.organization_name);
            }
            println!("  address: {} {}", contact.address1, contact.address2);
            println!(
                "  locality: {} {} {} {}",
                contact.city, contact.state_province, contact.postal_code, contact.country
            );
            println!(
                "  phone: {} email: {}",
                contact.phone, contact.email_address
            );
            println!("  read_only: {}", contact.read_only);
        }
    } else {
        let identical = c.registrant == c.tech && c.tech == c.admin && c.admin == c.aux_billing;
        println!("all_contact_sets_identical: {identical}");
        for (label, contact) in sets {
            println!(
                "{label}: country {} read_only {}",
                contact.country, contact.read_only
            );
        }
        println!("(contact details redacted; pass --full to show them)");
    }
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
