use serde::{Deserialize, Serialize};

use crate::api::xml::{self, de_bool, de_date};
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
    #[serde(
        rename(deserialize = "@Created", serialize = "created"),
        deserialize_with = "de_date"
    )]
    pub created: String,
    #[serde(
        rename(deserialize = "@Expires", serialize = "expires"),
        deserialize_with = "de_date"
    )]
    pub expires: String,
    #[serde(
        rename(deserialize = "@IsExpired", serialize = "is_expired"),
        deserialize_with = "de_bool"
    )]
    pub is_expired: bool,
    /// getList's IsLocked is a registry/dispute hold ("changes not
    /// allowed"), NOT the registrar transfer lock (see `domains lock`).
    /// Renamed in schema 3 after the old name misled two consumers.
    #[serde(
        rename(deserialize = "@IsLocked", serialize = "registry_hold"),
        deserialize_with = "de_bool"
    )]
    pub registry_hold: bool,
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

#[derive(Debug, Deserialize)]
pub struct SetLockResponse {
    #[serde(rename = "DomainSetRegistrarLockResult")]
    result: SetLockXml,
}

#[derive(Debug, Deserialize)]
struct SetLockXml {
    #[serde(rename = "@Domain", default)]
    domain: String,
    // No default on the outcome field — drift fails as parse (see CreateXml).
    #[serde(rename = "@IsSuccess", deserialize_with = "de_bool")]
    is_success: bool,
}

#[derive(Debug, Serialize)]
pub struct SetLockResult {
    pub domain: String,
    pub locked: bool,
    pub is_success: bool,
    /// Pre-image: the lock state that was replaced.
    pub previously_locked: bool,
}

/// Set the registrar (transfer) lock (mutating). Reads the current state
/// first as the journaled pre-image.
pub fn set_lock<T: Transport>(
    client: &Client<T>,
    domain: &str,
    lock: bool,
) -> Result<SetLockResult, Error> {
    client.require_mutations_permitted()?;
    let domain = crate::domain::normalize(domain)?;
    let before = lock_status(client, &domain)?;
    client.journal_note(
        "domains.lock.set",
        serde_json::json!({
            "domain": domain,
            "previously_locked": before.locked,
            "requested": if lock { "LOCK" } else { "UNLOCK" },
        }),
    );
    let action = if lock { "LOCK" } else { "UNLOCK" };
    let body = client.call_mut(
        "domains.setRegistrarLock",
        &[("DomainName", domain.as_str()), ("LockAction", action)],
    )?;
    let resp: SetLockResponse = xml::parse(&body)?;
    Ok(SetLockResult {
        domain: if resp.result.domain.is_empty() {
            domain
        } else {
            resp.result.domain
        },
        locked: lock,
        is_success: resp.result.is_success,
        previously_locked: before.locked,
    })
}

pub fn render_set_lock(result: &SetLockResult) {
    println!(
        "{}: registrar lock {} ({}; was {})",
        result.domain,
        if result.locked { "on" } else { "off" },
        if result.is_success {
            "success"
        } else {
            "NOT successful"
        },
        if result.previously_locked {
            "on"
        } else {
            "off"
        },
    );
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
    #[serde(rename = "CreatedDate", default, deserialize_with = "de_date")]
    created: String,
    #[serde(rename = "ExpiredDate", default, deserialize_with = "de_date")]
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
    #[serde(rename = "ExpiredDate", default, deserialize_with = "de_date")]
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
        "NAME", "EXPIRES", "HOLD", "RENEW", "PRIVACY", "OURDNS"
    );
    for d in domains {
        println!(
            "{:<40} {:<12} {:<6} {:<6} {:<12} {:<7}",
            d.name,
            d.expires,
            if d.registry_hold { "yes" } else { "no" },
            if d.auto_renew { "yes" } else { "no" },
            d.privacy,
            if d.is_our_dns { "yes" } else { "no" },
        );
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateResponse {
    #[serde(rename = "DomainCreateResult")]
    result: CreateXml,
}

#[derive(Debug, Deserialize)]
struct CreateXml {
    #[serde(rename = "@Domain", default)]
    domain: String,
    // Outcome fields deliberately have NO default: if the API drifts an
    // attribute name, a charge-completed create must fail as a parse error
    // (reconcile-first doctrine), never as a confident "registered: false"
    // — the one answer that invites a double purchase.
    #[serde(rename = "@Registered", deserialize_with = "de_bool")]
    registered: bool,
    #[serde(rename = "@ChargedAmount")]
    charged_amount: String,
    #[serde(rename = "@DomainID")]
    domain_id: String,
    #[serde(rename = "@OrderID")]
    order_id: String,
    #[serde(rename = "@TransactionID")]
    transaction_id: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResult {
    pub domain: String,
    pub registered: bool,
    pub years: u8,
    /// The live listed price the guard approved, pre-purchase.
    pub listed_price: String,
    /// What Namecheap reports it actually charged.
    pub charged_amount: String,
    /// True when the actual charge exceeded --max-price (ICANN fees or a
    /// listed/billed divergence); the purchase already happened — this is
    /// a loud flag, not a refusal.
    pub charged_exceeded_max_price: bool,
    pub domain_id: String,
    pub order_id: String,
    pub transaction_id: String,
}

/// Register a domain (mutating, charges money). Guard order, all live and
/// never cache-fed: availability check (premium refused), live price vs
/// --max-price, contacts copied from an owned domain — only then call_mut.
pub fn register<T: Transport>(
    client: &Client<T>,
    domain: &str,
    years: u8,
    max_price: f64,
    contacts_from: &str,
) -> Result<RegisterResult, Error> {
    client.require_mutations_permitted()?;
    let domain = crate::domain::normalize(domain)?;
    let (_, tld) = crate::domain::split_sld_tld(&domain)?;

    let availability = check(client, std::slice::from_ref(&domain))?;
    let status = availability
        .first()
        .ok_or_else(|| Error::Parse("empty availability response".into()))?;
    if !status.available {
        return Err(Error::Usage(format!(
            "{domain} is not available for registration"
        )));
    }
    if status.is_premium {
        return Err(Error::Usage(format!(
            "{domain} is a premium domain; ncheap does not register premium domains"
        )));
    }
    // Early Access Phase fees can be hundreds of dollars on top of a listed
    // price that passes the guard; fail closed like premium.
    if status
        .eap_fee
        .parse::<f64>()
        .map(|f| f > 0.0)
        .unwrap_or(false)
    {
        return Err(Error::Usage(format!(
            "{domain} carries an Early Access Phase fee ({}); ncheap does not register EAP domains",
            status.eap_fee
        )));
    }

    let listed = crate::commands::account::live_price(client, &tld, "REGISTER", years)?;
    if listed > max_price {
        return Err(Error::Usage(format!(
            "listed price {listed:.2} for {years} year(s) exceeds --max-price {max_price:.2}; not registering"
        )));
    }

    let source = contacts(client, contacts_from)?;
    let mut params: Vec<(String, String)> = vec![
        ("DomainName".into(), domain.clone()),
        ("Years".into(), years.to_string()),
        ("AddFreeWhoisguard".into(), "yes".into()),
        ("WGEnabled".into(), "no".into()),
    ];
    for (role, contact) in [
        ("Registrant", &source.registrant),
        ("Tech", &source.tech),
        ("Admin", &source.admin),
        ("AuxBilling", &source.aux_billing),
    ] {
        contact_params(role, contact, &mut params);
    }
    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let body = client.call_mut("domains.create", &param_refs)?;
    let resp: CreateResponse = xml::parse(&body)?;
    let charged_exceeded_max_price = charge_exceeds(&resp.result.charged_amount, max_price);
    Ok(RegisterResult {
        domain: resp.result.domain,
        registered: resp.result.registered,
        years,
        listed_price: format!("{listed:.2}"),
        charged_amount: resp.result.charged_amount,
        charged_exceeded_max_price,
        domain_id: resp.result.domain_id,
        order_id: resp.result.order_id,
        transaction_id: resp.result.transaction_id,
    })
}

fn contact_params(role: &str, c: &Contact, out: &mut Vec<(String, String)>) {
    let required = [
        ("FirstName", &c.first_name),
        ("LastName", &c.last_name),
        ("Address1", &c.address1),
        ("City", &c.city),
        ("StateProvince", &c.state_province),
        ("PostalCode", &c.postal_code),
        ("Country", &c.country),
        ("Phone", &c.phone),
        ("EmailAddress", &c.email_address),
    ];
    for (key, value) in required {
        out.push((format!("{role}{key}"), value.clone()));
    }
    let optional = [
        ("OrganizationName", &c.organization_name),
        ("JobTitle", &c.job_title),
        ("Address2", &c.address2),
        ("PhoneExt", &c.phone_ext),
        ("Fax", &c.fax),
    ];
    for (key, value) in optional {
        if !value.is_empty() {
            out.push((format!("{role}{key}"), value.clone()));
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RenewResponse {
    #[serde(rename = "DomainRenewResult")]
    result: RenewXml,
}

#[derive(Debug, Deserialize)]
struct RenewXml {
    #[serde(rename = "@DomainName", default)]
    domain_name: String,
    // No defaults on outcome fields — see CreateXml.
    #[serde(rename = "@Renew", deserialize_with = "de_bool")]
    renew: bool,
    #[serde(rename = "@ChargedAmount")]
    charged_amount: String,
    #[serde(rename = "@DomainID")]
    domain_id: String,
    #[serde(rename = "@OrderID")]
    order_id: String,
    #[serde(rename = "@TransactionID")]
    transaction_id: String,
}

#[derive(Debug, Serialize)]
pub struct RenewResult {
    pub domain: String,
    pub renewed: bool,
    pub years: u8,
    pub listed_price: String,
    pub charged_amount: String,
    /// See RegisterResult: loud flag when the charge exceeded --max-price.
    pub charged_exceeded_max_price: bool,
    pub domain_id: String,
    pub order_id: String,
    pub transaction_id: String,
}

/// True when the reported charge parses and exceeds the cap.
fn charge_exceeds(charged: &str, max_price: f64) -> bool {
    charged
        .parse::<f64>()
        .map(|c| c > max_price)
        .unwrap_or(false)
}

/// Renew a domain (mutating, charges money). Live price guard, then call_mut.
pub fn renew<T: Transport>(
    client: &Client<T>,
    domain: &str,
    years: u8,
    max_price: f64,
) -> Result<RenewResult, Error> {
    client.require_mutations_permitted()?;
    let domain = crate::domain::normalize(domain)?;
    let (_, tld) = crate::domain::split_sld_tld(&domain)?;

    let listed = crate::commands::account::live_price(client, &tld, "RENEW", years)?;
    if listed > max_price {
        return Err(Error::Usage(format!(
            "listed price {listed:.2} for {years} year(s) exceeds --max-price {max_price:.2}; not renewing"
        )));
    }

    let years_str = years.to_string();
    let body = client.call_mut(
        "domains.renew",
        &[
            ("DomainName", domain.as_str()),
            ("Years", years_str.as_str()),
        ],
    )?;
    let resp: RenewResponse = xml::parse(&body)?;
    let charged_exceeded_max_price = charge_exceeds(&resp.result.charged_amount, max_price);
    Ok(RenewResult {
        domain: resp.result.domain_name,
        renewed: resp.result.renew,
        years,
        listed_price: format!("{listed:.2}"),
        charged_amount: resp.result.charged_amount,
        charged_exceeded_max_price,
        domain_id: resp.result.domain_id,
        order_id: resp.result.order_id,
        transaction_id: resp.result.transaction_id,
    })
}

pub fn render_register(r: &RegisterResult) {
    println!(
        "{}: {} for {} year(s) — listed {}, charged {} (order {}, transaction {})",
        r.domain,
        if r.registered {
            "registered"
        } else {
            "NOT registered"
        },
        r.years,
        r.listed_price,
        r.charged_amount,
        r.order_id,
        r.transaction_id,
    );
}

pub fn render_renew(r: &RenewResult) {
    println!(
        "{}: {} for {} year(s) — listed {}, charged {} (order {}, transaction {})",
        r.domain,
        if r.renewed { "renewed" } else { "NOT renewed" },
        r.years,
        r.listed_price,
        r.charged_amount,
        r.order_id,
        r.transaction_id,
    );
}
