use serde::Serialize;

use crate::api::{Client, Error, Transport};
use crate::commands::{account, domains, privacy};

/// Days since the Unix epoch for a civil date (Howard Hinnant's
/// days_from_civil); enough date math for expiry horizons without a
/// calendar dependency.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parse an ISO-8601 date (as the envelope emits) into days-since-epoch.
fn parse_iso_days(s: &str) -> Option<i64> {
    let mut parts = s.splitn(3, '-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

pub fn today_days() -> i64 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        / 86_400) as i64
}

#[derive(Debug, Serialize)]
pub struct Finding {
    /// critical | warning | info
    pub severity: &'static str,
    pub check: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct AuditSummary {
    pub domains: usize,
    pub critical: usize,
    pub warning: usize,
    pub info: usize,
    pub api_calls: u32,
}

#[derive(Debug, Serialize)]
pub struct AuditReport {
    pub findings: Vec<Finding>,
    pub summary: AuditSummary,
}

const EXPIRY_HORIZON_DAYS: i64 = 90;
const EXPIRY_CRITICAL_DAYS: i64 = 30;

/// The kickoff's founding sentence as a command: every read-only safety
/// check across the account, one envelope. Cost: 3 + 2N API calls for N
/// domains (list, balances, privacy list, then per-domain transfer-lock
/// and contact-consistency probes). Findings are data: exit 0.
pub fn run<T: Transport>(client: &Client<T>, today: i64) -> Result<AuditReport, Error> {
    let mut findings: Vec<Finding> = Vec::new();

    let domain_list = domains::list(client)?;
    let balances = account::balances(client)?;
    let covers = account::covers_auto_renew(&balances);
    let subs = privacy::list(client)?;

    // Account-level: can auto-renew actually be funded?
    let expiring_soon: Vec<&domains::Domain> = domain_list
        .iter()
        .filter(|d| parse_iso_days(&d.expires).is_some_and(|e| e - today <= EXPIRY_HORIZON_DAYS))
        .collect();
    if covers == Some(false) {
        let severity = if expiring_soon.iter().any(|d| d.auto_renew) {
            "critical"
        } else {
            "warning"
        };
        findings.push(Finding {
            severity,
            check: "balance_covers_auto_renew",
            domain: None,
            message: format!(
                "available balance does not cover the {} {} needed for auto-renew",
                balances.funds_required_for_auto_renew, balances.currency
            ),
        });
    }

    for d in &domain_list {
        match parse_iso_days(&d.expires) {
            Some(exp) => {
                let left = exp - today;
                if left <= EXPIRY_HORIZON_DAYS {
                    findings.push(Finding {
                        severity: if left <= EXPIRY_CRITICAL_DAYS {
                            "critical"
                        } else {
                            "warning"
                        },
                        check: "expiry_horizon",
                        domain: Some(d.name.clone()),
                        message: format!(
                            "expires {} ({left} days){}",
                            d.expires,
                            if d.auto_renew {
                                ""
                            } else {
                                " and auto-renew is OFF"
                            }
                        ),
                    });
                }
            }
            None => findings.push(Finding {
                severity: "info",
                check: "expiry_horizon",
                domain: Some(d.name.clone()),
                message: format!("unparseable expiry date {:?}", d.expires),
            }),
        }
        if !d.auto_renew {
            findings.push(Finding {
                severity: "warning",
                check: "auto_renew",
                domain: Some(d.name.clone()),
                message: "auto-renew is OFF; the domain will lapse at expiry".into(),
            });
        }
        if d.is_expired {
            findings.push(Finding {
                severity: "critical",
                check: "expired",
                domain: Some(d.name.clone()),
                message: "domain is EXPIRED".into(),
            });
        }
        if d.registry_hold {
            findings.push(Finding {
                severity: "critical",
                check: "registry_hold",
                domain: Some(d.name.clone()),
                message: "registry/dispute hold: changes are not allowed".into(),
            });
        }
        if !d.privacy.eq_ignore_ascii_case("enabled") {
            findings.push(Finding {
                severity: "warning",
                check: "privacy",
                domain: Some(d.name.clone()),
                message: format!("domain privacy is {}", d.privacy),
            });
        }
        if !d.is_our_dns {
            findings.push(Finding {
                severity: "info",
                check: "dns_posture",
                domain: Some(d.name.clone()),
                message: "uses external nameservers".into(),
            });
        }
    }

    // Privacy subscriptions that exist but are off or unattached.
    for s in &subs {
        if !s.status.eq_ignore_ascii_case("enabled") {
            findings.push(Finding {
                severity: "warning",
                check: "privacy_subscription",
                domain: (!s.domain_name.is_empty()).then(|| s.domain_name.clone()),
                message: format!("privacy subscription {} is {}", s.id, s.status),
            });
        }
    }

    // Per-domain probes: transfer lock and contact consistency.
    for d in &domain_list {
        match domains::lock_status(client, &d.name) {
            Ok(status) if !status.locked => findings.push(Finding {
                severity: "warning",
                check: "transfer_lock",
                domain: Some(d.name.clone()),
                message: "registrar transfer lock is OFF".into(),
            }),
            Ok(_) => {}
            Err(e) => findings.push(Finding {
                severity: "info",
                check: "transfer_lock",
                domain: Some(d.name.clone()),
                message: format!("could not read lock status: {e}"),
            }),
        }
        match domains::contacts(client, &d.name) {
            Ok(c) => {
                let identical =
                    c.registrant == c.tech && c.tech == c.admin && c.admin == c.aux_billing;
                if !identical {
                    findings.push(Finding {
                        severity: "warning",
                        check: "contact_consistency",
                        domain: Some(d.name.clone()),
                        message: "contact sets differ between roles".into(),
                    });
                }
            }
            Err(e) => findings.push(Finding {
                severity: "info",
                check: "contact_consistency",
                domain: Some(d.name.clone()),
                message: format!("could not read contacts: {e}"),
            }),
        }
    }

    let order = |s: &str| match s {
        "critical" => 0,
        "warning" => 1,
        _ => 2,
    };
    findings.sort_by_key(|f| (order(f.severity), f.domain.clone()));
    let count = |s: &str| findings.iter().filter(|f| f.severity == s).count();
    let summary = AuditSummary {
        domains: domain_list.len(),
        critical: count("critical"),
        warning: count("warning"),
        info: count("info"),
        api_calls: client.calls(),
    };
    Ok(AuditReport { findings, summary })
}

pub fn render(report: &AuditReport) {
    let s = &report.summary;
    crate::safe_println!(
        "audit: {} domains — {} critical, {} warning, {} info ({} API calls)",
        s.domains,
        s.critical,
        s.warning,
        s.info,
        s.api_calls
    );
    for f in &report.findings {
        crate::safe_println!(
            "[{}] {} {}: {}",
            f.severity.to_uppercase(),
            f.check,
            f.domain.as_deref().unwrap_or("(account)"),
            f.message
        );
    }
    if report.findings.is_empty() {
        crate::safe_println!("no findings: all checks clean");
    }
}
