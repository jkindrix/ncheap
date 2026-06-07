use serde::{Deserialize, Serialize};

use crate::api::xml::{self, de_bool};
use crate::api::{Client, Error, Transport};
use crate::domain::split_sld_tld;

#[derive(Debug, Deserialize)]
pub struct GetListResponse {
    #[serde(rename = "DomainDNSGetListResult")]
    result: DnsListXml,
}

#[derive(Debug, Deserialize)]
struct DnsListXml {
    #[serde(rename = "@Domain")]
    domain: String,
    #[serde(rename = "@IsUsingOurDNS", deserialize_with = "de_bool")]
    is_using_our_dns: bool,
    #[serde(rename = "Nameserver", default)]
    nameservers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetHostsResponse {
    #[serde(rename = "DomainDNSGetHostsResult")]
    result: HostsXml,
}

#[derive(Debug, Deserialize)]
struct HostsXml {
    /// setHosts requires EmailType on resend; dropping it can reset the
    /// domain's mail configuration. Captured here for the rewrite.
    #[serde(rename = "@EmailType", default)]
    email_type: String,
    /// The docs show `<Host>`; the live API returns `<host>`.
    #[serde(rename = "Host", alias = "host", default)]
    hosts: Vec<HostRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HostRecord {
    /// Docs disagree on the attribute casing (HostId vs HostID).
    #[serde(
        rename(deserialize = "@HostId", serialize = "id"),
        alias = "@HostID",
        default
    )]
    pub id: String,
    #[serde(rename(deserialize = "@Name", serialize = "name"))]
    pub name: String,
    #[serde(rename(deserialize = "@Type", serialize = "type"))]
    pub record_type: String,
    #[serde(rename(deserialize = "@Address", serialize = "address"))]
    pub address: String,
    #[serde(rename(deserialize = "@MXPref", serialize = "mx_pref"), default)]
    pub mx_pref: String,
    #[serde(rename(deserialize = "@TTL", serialize = "ttl"), default)]
    pub ttl: String,
}

#[derive(Debug, Serialize)]
pub struct DnsInfo {
    pub domain: String,
    pub is_using_our_dns: bool,
    pub nameservers: Vec<String>,
    /// None when the domain uses external DNS: Namecheap only serves host
    /// records for domains on its own nameservers (getHosts errors 2030288).
    pub host_records: Option<Vec<HostRecord>>,
}

/// Nameserver mode plus host records where Namecheap is authoritative.
/// One API call for external-DNS domains, two when records are fetchable.
pub fn get<T: Transport>(client: &Client<T>, domain: &str) -> Result<DnsInfo, Error> {
    let (sld, tld) = split_sld_tld(domain)?;
    let params = [("SLD", sld.as_str()), ("TLD", tld.as_str())];
    let body = client.call("domains.dns.getList", &params)?;
    let list: GetListResponse = xml::parse(&body)?;
    let host_records = if list.result.is_using_our_dns {
        let body = client.call("domains.dns.getHosts", &params)?;
        let hosts: GetHostsResponse = xml::parse(&body)?;
        Some(hosts.result.hosts)
    } else {
        None
    };
    Ok(DnsInfo {
        domain: list.result.domain,
        is_using_our_dns: list.result.is_using_our_dns,
        nameservers: list.result.nameservers,
        host_records,
    })
}

#[derive(Debug, Deserialize)]
pub struct SetCustomResponse {
    #[serde(rename = "DomainDNSSetCustomResult")]
    result: SetCustomXml,
}

#[derive(Debug, Deserialize)]
struct SetCustomXml {
    #[serde(rename = "@Domain")]
    domain: String,
    #[serde(rename = "@Updated", deserialize_with = "de_bool")]
    updated: bool,
}

#[derive(Debug, Serialize)]
pub struct SetResult {
    pub domain: String,
    pub updated: bool,
    pub nameservers: Vec<String>,
    /// The nameservers that were replaced (pre-image): the manual-undo
    /// input, also journaled before the mutation.
    pub previous_nameservers: Vec<String>,
}

/// Point a domain at custom nameservers (namecheap.domains.dns.setCustom).
/// Mutating: goes through call_mut — production-gated, never auto-retried.
pub fn set<T: Transport>(
    client: &Client<T>,
    domain: &str,
    nameservers: &[String],
) -> Result<SetResult, Error> {
    client.require_mutations_permitted()?;
    let (sld, tld) = split_sld_tld(domain)?;
    // Pre-image: setCustom is full-replace with no upstream undo, so the
    // outgoing nameservers are fetched and journaled before the mutation.
    let pre_params = [("SLD", sld.as_str()), ("TLD", tld.as_str())];
    let pre_body = client.call("domains.dns.getList", &pre_params)?;
    let pre: GetListResponse = xml::parse(&pre_body)?;
    let previous_nameservers = pre.result.nameservers;
    client.journal_note(
        "dns.set",
        serde_json::json!({
            "domain": domain,
            "previous_nameservers": previous_nameservers,
            "is_using_our_dns": pre.result.is_using_our_dns,
        }),
    );
    let list = nameservers.join(",");
    let body = client.call_mut(
        "domains.dns.setCustom",
        &[
            ("SLD", sld.as_str()),
            ("TLD", tld.as_str()),
            ("NameServers", list.as_str()),
        ],
    )?;
    let resp: SetCustomResponse = xml::parse(&body)?;
    Ok(SetResult {
        domain: resp.result.domain,
        updated: resp.result.updated,
        nameservers: nameservers.to_vec(),
        previous_nameservers,
    })
}

pub fn render_set(result: &SetResult) {
    crate::safe_println!(
        "{}: nameservers {} ({})",
        result.domain,
        if result.updated {
            "updated"
        } else {
            "NOT updated"
        },
        result.nameservers.join(", "),
    );
}

pub fn render(info: &DnsInfo) {
    crate::safe_println!("domain: {}", info.domain);
    crate::safe_println!(
        "dns: {}",
        if info.is_using_our_dns {
            "namecheap"
        } else {
            "external"
        }
    );
    for ns in &info.nameservers {
        crate::safe_println!("nameserver: {ns}");
    }
    match &info.host_records {
        Some(hosts) => {
            crate::safe_println!(
                "{:<30} {:<8} {:<8} {:<6} ADDRESS",
                "NAME",
                "TYPE",
                "TTL",
                "MX"
            );
            for h in hosts {
                crate::safe_println!(
                    "{:<30} {:<8} {:<8} {:<6} {}",
                    h.name,
                    h.record_type,
                    h.ttl,
                    h.mx_pref,
                    h.address
                );
            }
        }
        None => crate::safe_println!("(host records not managed by Namecheap)"),
    }
}

/// Record types setHosts accepts (mirror doc, capture 2025-08-14).
const RECORD_TYPES: &[&str] = &[
    "A", "AAAA", "ALIAS", "CAA", "CNAME", "MX", "MXE", "NS", "TXT", "URL", "URL301", "FRAME",
];

#[derive(Debug, Serialize)]
pub struct EditResult {
    pub domain: String,
    pub is_success: bool,
    pub action: String,
    pub records_before: usize,
    pub records_after: usize,
}

#[derive(Debug, Deserialize)]
pub struct SetHostsResponse {
    #[serde(rename = "DomainDNSSetHostsResult")]
    result: SetHostsXml,
}

#[derive(Debug, Deserialize)]
struct SetHostsXml {
    #[serde(rename = "@Domain", default)]
    domain: String,
    // No default — drift fails as parse, like every mutation outcome.
    #[serde(rename = "@IsSuccess", deserialize_with = "de_bool")]
    is_success: bool,
}

fn fetch_zone<T: Transport>(
    client: &Client<T>,
    sld: &str,
    tld: &str,
) -> Result<(Vec<HostRecord>, String), Error> {
    let body = client.call("domains.dns.getHosts", &[("SLD", sld), ("TLD", tld)])?;
    let resp: GetHostsResponse = xml::parse(&body)?;
    Ok((resp.result.hosts, resp.result.email_type))
}

/// setHosts is FULL REPLACE with no upstream undo or compare-and-swap:
/// the journal note carries the complete pre-image, and concurrent edits
/// are last-writer-wins (documented).
#[allow(clippy::too_many_arguments)]
fn write_zone<T: Transport>(
    client: &Client<T>,
    domain: &str,
    sld: &str,
    tld: &str,
    records: &[HostRecord],
    email_type: &str,
    action: &str,
    pre: &[HostRecord],
) -> Result<bool, Error> {
    client.journal_note(
        "dns.records",
        serde_json::json!({
            "domain": domain,
            "action": action,
            "email_type": email_type,
            "previous_records": pre
                .iter()
                .map(|h| serde_json::json!({
                    "name": h.name, "type": h.record_type, "address": h.address,
                    "ttl": h.ttl, "mx_pref": h.mx_pref,
                }))
                .collect::<Vec<_>>(),
        }),
    );
    let mut params: Vec<(String, String)> = vec![
        ("SLD".into(), sld.to_owned()),
        ("TLD".into(), tld.to_owned()),
    ];
    if !email_type.is_empty() {
        params.push(("EmailType".into(), email_type.to_owned()));
    }
    for (i, r) in records.iter().enumerate() {
        let n = i + 1;
        params.push((format!("HostName{n}"), r.name.clone()));
        params.push((format!("RecordType{n}"), r.record_type.clone()));
        params.push((format!("Address{n}"), r.address.clone()));
        if !r.ttl.is_empty() {
            params.push((format!("TTL{n}"), r.ttl.clone()));
        }
        if r.record_type.eq_ignore_ascii_case("MX") && !r.mx_pref.is_empty() {
            params.push((format!("MXPref{n}"), r.mx_pref.clone()));
        }
    }
    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let body = client.call_mut("domains.dns.setHosts", &param_refs)?;
    let resp: SetHostsResponse = xml::parse(&body)?;
    let _ = resp.result.domain;
    Ok(resp.result.is_success)
}

fn validate_type(record_type: &str) -> Result<String, Error> {
    let upper = record_type.to_ascii_uppercase();
    if !RECORD_TYPES.contains(&upper.as_str()) {
        return Err(Error::Usage(format!(
            "unknown record type {record_type:?}; supported: {}",
            RECORD_TYPES.join(", ")
        )));
    }
    Ok(upper)
}

/// Add one host record (mutating; full-zone rewrite under the hood).
#[allow(clippy::too_many_arguments)]
pub fn add_record<T: Transport>(
    client: &Client<T>,
    domain: &str,
    name: &str,
    record_type: &str,
    address: &str,
    ttl: Option<u32>,
    mx_pref: Option<u32>,
) -> Result<EditResult, Error> {
    client.require_mutations_permitted()?;
    let record_type = validate_type(record_type)?;
    if record_type == "MX" && mx_pref.is_none() {
        return Err(Error::Usage(
            "MX records require --mx-pref (the API mandates MXPref for MX)".into(),
        ));
    }
    let (sld, tld) = split_sld_tld(domain)?;
    let (mut records, email_type) = fetch_zone(client, &sld, &tld)?;
    let before = records.len();
    if records.iter().any(|r| {
        r.name.eq_ignore_ascii_case(name)
            && r.record_type.eq_ignore_ascii_case(&record_type)
            && r.address == address
    }) {
        return Err(Error::Usage(format!(
            "an identical {record_type} record for {name:?} already exists"
        )));
    }
    let pre = records.clone();
    records.push(HostRecord {
        id: String::new(),
        name: name.to_owned(),
        record_type: record_type.clone(),
        address: address.to_owned(),
        mx_pref: mx_pref.map(|p| p.to_string()).unwrap_or_default(),
        ttl: ttl.map(|t| t.to_string()).unwrap_or_default(),
    });
    let action = format!("add {record_type} {name} -> {address}");
    let is_success = write_zone(
        client,
        domain,
        &sld,
        &tld,
        &records,
        &email_type,
        &action,
        &pre,
    )?;
    Ok(EditResult {
        domain: domain.to_owned(),
        is_success,
        action,
        records_before: before,
        records_after: records.len(),
    })
}

/// Remove matching host records (mutating; full-zone rewrite).
pub fn remove_record<T: Transport>(
    client: &Client<T>,
    domain: &str,
    name: &str,
    record_type: &str,
    address: Option<&str>,
) -> Result<EditResult, Error> {
    client.require_mutations_permitted()?;
    let record_type = validate_type(record_type)?;
    let (sld, tld) = split_sld_tld(domain)?;
    let (records, email_type) = fetch_zone(client, &sld, &tld)?;
    let before = records.len();
    let (matched, remaining): (Vec<HostRecord>, Vec<HostRecord>) =
        records.into_iter().partition(|r| {
            r.name.eq_ignore_ascii_case(name)
                && r.record_type.eq_ignore_ascii_case(&record_type)
                && address.is_none_or(|a| r.address == a)
        });
    if matched.is_empty() {
        return Err(Error::Usage(format!(
            "no {record_type} record for {name:?}{} found",
            address
                .map(|a| format!(" with address {a:?}"))
                .unwrap_or_default()
        )));
    }
    if remaining.is_empty() {
        return Err(Error::Usage(
            "removal would leave the zone empty; refusing (clear the zone via the dashboard if intended)"
                .into(),
        ));
    }
    let pre: Vec<HostRecord> = matched.iter().chain(remaining.iter()).cloned().collect();
    let action = format!(
        "remove {} {record_type} record(s) for {name}",
        matched.len()
    );
    let is_success = write_zone(
        client,
        domain,
        &sld,
        &tld,
        &remaining,
        &email_type,
        &action,
        &pre,
    )?;
    Ok(EditResult {
        domain: domain.to_owned(),
        is_success,
        action,
        records_before: before,
        records_after: remaining.len(),
    })
}

pub fn render_edit(result: &EditResult) {
    crate::safe_println!(
        "{}: {} ({}; records {} -> {})",
        result.domain,
        result.action,
        if result.is_success {
            "success"
        } else {
            "NOT successful"
        },
        result.records_before,
        result.records_after,
    );
}

#[derive(Debug, Deserialize)]
pub struct SetDefaultResponse {
    #[serde(rename = "DomainDNSSetDefaultResult")]
    result: SetDefaultXml,
}

#[derive(Debug, Deserialize)]
struct SetDefaultXml {
    #[serde(rename = "@Domain", default)]
    domain: String,
    #[serde(rename = "@Updated", deserialize_with = "de_bool")]
    updated: bool,
}

/// Revert a domain to Namecheap default DNS (mutating) — the inverse of
/// `dns set`. Pre-image (the custom nameservers being left) journaled.
pub fn set_default<T: Transport>(client: &Client<T>, domain: &str) -> Result<SetResult, Error> {
    client.require_mutations_permitted()?;
    let (sld, tld) = split_sld_tld(domain)?;
    let pre_params = [("SLD", sld.as_str()), ("TLD", tld.as_str())];
    let pre_body = client.call("domains.dns.getList", &pre_params)?;
    let pre: GetListResponse = xml::parse(&pre_body)?;
    let previous_nameservers = pre.result.nameservers;
    client.journal_note(
        "dns.set_default",
        serde_json::json!({
            "domain": domain,
            "previous_nameservers": previous_nameservers,
            "is_using_our_dns": pre.result.is_using_our_dns,
        }),
    );
    let body = client.call_mut("domains.dns.setDefault", &pre_params)?;
    let resp: SetDefaultResponse = xml::parse(&body)?;
    Ok(SetResult {
        domain: resp.result.domain,
        updated: resp.result.updated,
        nameservers: vec![],
        previous_nameservers,
    })
}
