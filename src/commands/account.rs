use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::xml;
use crate::api::{Client, Error, Transport};

#[derive(Debug, Deserialize)]
pub struct GetBalancesResponse {
    #[serde(rename = "UserGetBalancesResult")]
    result: Balances,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Balances {
    #[serde(rename(deserialize = "@Currency", serialize = "currency"))]
    pub currency: String,
    #[serde(rename(deserialize = "@AvailableBalance", serialize = "available_balance"))]
    pub available_balance: String,
    #[serde(rename(deserialize = "@AccountBalance", serialize = "account_balance"))]
    pub account_balance: String,
    #[serde(rename(deserialize = "@EarnedAmount", serialize = "earned_amount"))]
    pub earned_amount: String,
    #[serde(rename(deserialize = "@WithdrawableAmount", serialize = "withdrawable_amount"))]
    pub withdrawable_amount: String,
    #[serde(rename(
        deserialize = "@FundsRequiredForAutoRenew",
        serialize = "funds_required_for_auto_renew"
    ))]
    pub funds_required_for_auto_renew: String,
}

pub fn balances<T: Transport>(client: &Client<T>) -> Result<Balances, Error> {
    let body = client.call("users.getBalances", &[])?;
    let resp: GetBalancesResponse = xml::parse(&body)?;
    Ok(resp.result)
}

/// Balance amounts are private. The default view carries only the currency,
/// the auto-renew requirement, and whether the available balance covers it —
/// the audit signal — with the amounts themselves behind --full.
pub fn redacted_view(b: &Balances) -> serde_json::Value {
    json!({
        "currency": b.currency,
        "funds_required_for_auto_renew": b.funds_required_for_auto_renew,
        "available_covers_auto_renew": covers_auto_renew(b),
    })
}

/// None when an amount fails to parse (shape is undocumented for all locales).
fn covers_auto_renew(b: &Balances) -> Option<bool> {
    let available: f64 = b.available_balance.parse().ok()?;
    let required: f64 = b.funds_required_for_auto_renew.parse().ok()?;
    Some(available >= required)
}

pub fn render(b: &Balances, full: bool) {
    println!("currency: {}", b.currency);
    println!(
        "funds_required_for_auto_renew: {}",
        b.funds_required_for_auto_renew
    );
    match covers_auto_renew(b) {
        Some(v) => println!(
            "available_covers_auto_renew: {}",
            if v { "yes" } else { "no" }
        ),
        None => println!("available_covers_auto_renew: unknown"),
    }
    if full {
        println!("available_balance: {}", b.available_balance);
        println!("account_balance: {}", b.account_balance);
        println!("earned_amount: {}", b.earned_amount);
        println!("withdrawable_amount: {}", b.withdrawable_amount);
    } else {
        println!("(amounts redacted; pass --full to show them)");
    }
}
