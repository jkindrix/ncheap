use std::path::{Path, PathBuf};
use std::time::Duration;

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

#[derive(Debug, Deserialize)]
pub struct GetPricingResponse {
    #[serde(rename = "UserGetPricingResult", default)]
    result: PricingResultXml,
}

#[derive(Debug, Default, Deserialize)]
struct PricingResultXml {
    #[serde(rename = "ProductType", default)]
    types: Vec<ProductTypeXml>,
}

#[derive(Debug, Deserialize)]
struct ProductTypeXml {
    #[serde(rename = "@Name")]
    name: String,
    #[serde(rename = "ProductCategory", default)]
    categories: Vec<ProductCategoryXml>,
}

#[derive(Debug, Deserialize)]
struct ProductCategoryXml {
    #[serde(rename = "@Name")]
    name: String,
    #[serde(rename = "Product", default)]
    products: Vec<ProductXml>,
}

#[derive(Debug, Deserialize)]
struct ProductXml {
    #[serde(rename = "@Name")]
    name: String,
    #[serde(rename = "Price", default)]
    prices: Vec<PriceXml>,
}

#[derive(Debug, Deserialize)]
struct PriceXml {
    #[serde(rename = "@Duration", default)]
    duration: String,
    #[serde(rename = "@DurationType", default)]
    duration_type: String,
    #[serde(rename = "@Price", default)]
    price: String,
    #[serde(rename = "@RegularPrice", default)]
    regular_price: String,
    #[serde(rename = "@YourPrice", default)]
    your_price: String,
    #[serde(rename = "@CouponPrice", default)]
    coupon_price: String,
    #[serde(rename = "@Currency", default)]
    currency: String,
}

/// One flattened pricing entry: the nested ProductType > ProductCategory >
/// Product > Price tree as agent-friendly rows.
#[derive(Debug, Deserialize, Serialize)]
pub struct PriceRow {
    pub product_type: String,
    pub category: String,
    pub product: String,
    pub duration: String,
    pub duration_type: String,
    pub price: String,
    pub regular_price: String,
    pub your_price: String,
    pub coupon_price: String,
    pub currency: String,
}

#[derive(Debug)]
pub struct PricingQuery {
    pub product_type: String,
    pub category: Option<String>,
    pub action: Option<String>,
    pub product: Option<String>,
}

impl PricingQuery {
    /// Keyed by profile and sandbox flag as well as the query: YourPrice is
    /// account-specific and sandbox pricing differs from production, so one
    /// profile's prices must never be served to another.
    fn cache_file_name(&self, profile: &crate::config::Profile) -> String {
        let mut key = format!(
            "pricing-{}-{}-{}-{}-{}-{}",
            profile.name,
            profile.sandbox,
            self.product_type,
            self.category.as_deref().unwrap_or(""),
            self.action.as_deref().unwrap_or(""),
            self.product.as_deref().unwrap_or("")
        )
        .to_ascii_lowercase();
        key.retain(|c| c.is_ascii_alphanumeric() || c == '-');
        format!("{key}.json")
    }
}

/// The API docs strongly recommend caching getPricing responses.
const PRICING_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Returns the rows and whether they came from cache (api_calls stays 0 on a
/// hit). Cache failures of any kind fall back to a live fetch; cache writes
/// are best-effort.
pub fn pricing<T: Transport>(
    client: &Client<T>,
    query: &PricingQuery,
    cache_dir: Option<&Path>,
) -> Result<(Vec<PriceRow>, bool), Error> {
    let cache_path: Option<PathBuf> =
        cache_dir.map(|d| d.join(query.cache_file_name(client.profile())));
    if let Some(path) = &cache_path
        && let Some(rows) = read_cache(path)
    {
        return Ok((rows, true));
    }

    let mut params: Vec<(&str, &str)> = vec![("ProductType", query.product_type.as_str())];
    if let Some(v) = &query.category {
        params.push(("ProductCategory", v));
    }
    if let Some(v) = &query.action {
        params.push(("ActionName", v));
    }
    if let Some(v) = &query.product {
        params.push(("ProductName", v));
    }
    let body = client.call("users.getPricing", &params)?;
    let resp: GetPricingResponse = xml::parse(&body)?;

    let mut rows = Vec::new();
    for t in resp.result.types {
        for c in t.categories {
            for p in c.products {
                for price in p.prices {
                    rows.push(PriceRow {
                        product_type: t.name.clone(),
                        category: c.name.clone(),
                        product: p.name.clone(),
                        duration: price.duration,
                        duration_type: price.duration_type,
                        price: price.price,
                        regular_price: price.regular_price,
                        your_price: price.your_price,
                        coupon_price: price.coupon_price,
                        currency: price.currency,
                    });
                }
            }
        }
    }

    if let Some(path) = &cache_path {
        write_cache(path, &rows);
    }
    Ok((rows, false))
}

fn read_cache(path: &Path) -> Option<Vec<PriceRow>> {
    let meta = std::fs::metadata(path).ok()?;
    let age = meta.modified().ok()?.elapsed().ok()?;
    if age > PRICING_CACHE_TTL {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_cache(path: &Path, rows: &[PriceRow]) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let Ok(body) = serde_json::to_string(rows) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // 0600 like the config file: account-specific pricing is private data,
    // and the cache should not be the one world-readable artifact.
    let _ = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut f| f.write_all(body.as_bytes()));
}

pub fn render_pricing(rows: &[PriceRow]) {
    println!(
        "{:<14} {:<12} {:<12} {:<5} {:<10} {:<10} CURRENCY",
        "TYPE", "CATEGORY", "PRODUCT", "DUR", "PRICE", "YOURS"
    );
    for r in rows {
        println!(
            "{:<14} {:<12} {:<12} {:<5} {:<10} {:<10} {}",
            r.product_type, r.category, r.product, r.duration, r.price, r.your_price, r.currency
        );
    }
}

/// Live price for one TLD/action/duration — used by purchase guards, which
/// must never read the cache: a stale or contaminated price cannot be
/// allowed to justify a charge.
pub fn live_price<T: Transport>(
    client: &Client<T>,
    tld: &str,
    action: &str,
    years: u8,
) -> Result<f64, Error> {
    let query = PricingQuery {
        product_type: "DOMAIN".into(),
        category: None,
        action: Some(action.into()),
        product: Some(tld.into()),
    };
    let (rows, _) = pricing(client, &query, None)?;
    // Match category and product too: the server-side ActionName filter is
    // not guaranteed to restrict the response (the docs' own example carries
    // multiple categories), and the first duration-matching row could
    // otherwise be the wrong action's price.
    let row = rows
        .iter()
        .find(|r| {
            r.duration == years.to_string()
                && r.duration_type.eq_ignore_ascii_case("YEAR")
                && r.category.eq_ignore_ascii_case(action)
                && r.product.eq_ignore_ascii_case(tld)
        })
        .ok_or_else(|| {
            Error::Usage(format!(
                "no {action} pricing found for .{tld} at {years} year(s); cannot price-guard the purchase"
            ))
        })?;
    row.your_price.parse::<f64>().map_err(|_| {
        Error::Parse(format!(
            "unparseable price {:?} for .{tld}; cannot price-guard the purchase",
            row.your_price
        ))
    })
}
