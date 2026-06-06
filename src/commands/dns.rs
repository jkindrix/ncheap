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
    /// The docs show `<Host>`; the live API returns `<host>`.
    #[serde(rename = "Host", alias = "host", default)]
    hosts: Vec<HostRecord>,
}

#[derive(Debug, Deserialize, Serialize)]
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

pub fn render(info: &DnsInfo) {
    println!("domain: {}", info.domain);
    println!(
        "dns: {}",
        if info.is_using_our_dns {
            "namecheap"
        } else {
            "external"
        }
    );
    for ns in &info.nameservers {
        println!("nameserver: {ns}");
    }
    match &info.host_records {
        Some(hosts) => {
            println!(
                "{:<30} {:<8} {:<8} {:<6} ADDRESS",
                "NAME", "TYPE", "TTL", "MX"
            );
            for h in hosts {
                println!(
                    "{:<30} {:<8} {:<8} {:<6} {}",
                    h.name, h.record_type, h.ttl, h.mx_pref, h.address
                );
            }
        }
        None => println!("(host records not managed by Namecheap)"),
    }
}
