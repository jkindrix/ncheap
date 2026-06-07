use serde::{Deserialize, Serialize};

use crate::api::xml::{self, de_bool};
use crate::api::{Client, Error, Transport};

#[derive(Debug, Deserialize)]
pub struct CreateResponse {
    #[serde(rename = "DomainTransferCreateResult")]
    result: CreateXml,
}

#[derive(Debug, Deserialize)]
struct CreateXml {
    /// The API really does spell it "Domainname" here.
    #[serde(rename = "@Domainname", alias = "@DomainName", default)]
    domain: String,
    // Outcome fields without defaults: drift fails as parse, never as a
    // confident "transfer: false" that invites a duplicate order.
    #[serde(rename = "@Transfer", deserialize_with = "de_bool")]
    transfer: bool,
    #[serde(rename = "@TransferID")]
    transfer_id: String,
    #[serde(rename = "@OrderID")]
    order_id: String,
    #[serde(rename = "@TransactionID")]
    transaction_id: String,
    #[serde(rename = "@ChargedAmount")]
    charged_amount: String,
    #[serde(rename = "@StatusID", default)]
    status_id: String,
}

#[derive(Debug, Serialize)]
pub struct CreateResult {
    pub domain: String,
    pub accepted: bool,
    pub transfer_id: String,
    pub status_id: String,
    pub listed_price: String,
    pub charged_amount: String,
    pub charged_exceeded_max_price: bool,
    pub order_id: String,
    pub transaction_id: String,
}

/// Start an inbound transfer (mutating, charges money). Same guard stack
/// as register/renew: live TRANSFER price vs --max-price, spend-cap
/// reservation, journal. Years is pinned to 1 (the API mandates it).
pub fn create<T: Transport>(
    client: &Client<T>,
    domain: &str,
    epp_code: &str,
    max_price: f64,
) -> Result<CreateResult, Error> {
    client.require_mutations_permitted()?;
    let domain = crate::domain::normalize(domain)?;
    let (_, tld) = crate::domain::split_sld_tld(&domain)?;
    if epp_code.trim().is_empty() {
        return Err(Error::Usage(
            "an EPP/auth code from the current registrar is required (--epp-code)".into(),
        ));
    }
    let listed = crate::commands::account::live_price(client, &tld, "TRANSFER", 1)?;
    if listed > max_price {
        return Err(Error::Usage(format!(
            "listed transfer price {listed:.2} exceeds --max-price {max_price:.2}; not transferring"
        )));
    }
    client.reserve_spend(listed, "domains.transfer.create", &domain)?;
    let body = client.call_mut(
        "domains.transfer.create",
        &[
            ("DomainName", domain.as_str()),
            ("Years", "1"),
            ("EPPCode", epp_code),
        ],
    )?;
    let resp: CreateResponse = xml::parse(&body)?;
    let charged_exceeded_max_price = resp
        .result
        .charged_amount
        .parse::<f64>()
        .map(|c| c > max_price)
        .unwrap_or(false);
    Ok(CreateResult {
        domain: if resp.result.domain.is_empty() {
            domain
        } else {
            resp.result.domain
        },
        accepted: resp.result.transfer,
        transfer_id: resp.result.transfer_id,
        status_id: resp.result.status_id,
        listed_price: format!("{listed:.2}"),
        charged_amount: resp.result.charged_amount,
        charged_exceeded_max_price,
        order_id: resp.result.order_id,
        transaction_id: resp.result.transaction_id,
    })
}

#[derive(Debug, Deserialize)]
pub struct StatusResponse {
    #[serde(rename = "DomainTransferGetStatusResult")]
    result: StatusXml,
}

#[derive(Debug, Deserialize)]
struct StatusXml {
    #[serde(rename = "@TransferID", default)]
    transfer_id: String,
    #[serde(rename = "@Status", default)]
    status: String,
    #[serde(rename = "@StatusID", default)]
    status_id: String,
}

#[derive(Debug, Serialize)]
pub struct StatusResult {
    pub transfer_id: String,
    pub status: String,
    pub status_id: String,
}

/// Read-only transfer status by TransferID.
pub fn status<T: Transport>(client: &Client<T>, transfer_id: &str) -> Result<StatusResult, Error> {
    let body = client.call("domains.transfer.getStatus", &[("TransferID", transfer_id)])?;
    let resp: StatusResponse = xml::parse(&body)?;
    Ok(StatusResult {
        transfer_id: resp.result.transfer_id,
        status: resp.result.status,
        status_id: resp.result.status_id,
    })
}

pub fn render_create(r: &CreateResult) {
    crate::safe_println!(
        "{}: transfer {} (id {}, status_id {}) — listed {}, charged {} (order {}, transaction {})",
        r.domain,
        if r.accepted {
            "accepted"
        } else {
            "NOT accepted"
        },
        r.transfer_id,
        r.status_id,
        r.listed_price,
        r.charged_amount,
        r.order_id,
        r.transaction_id,
    );
}

pub fn render_status(r: &StatusResult) {
    crate::safe_println!(
        "transfer {}: {} (status_id {})",
        r.transfer_id,
        r.status,
        r.status_id
    );
}
