mod common;

use common::{FakeTransport, param, test_profile};
use ncheap::api::Client;
use ncheap::commands::{account, domains};

fn envelope(command: &str, inner: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ApiResponse xmlns="http://api.namecheap.com/xml.response" Status="OK">
  <Errors />
  <Warnings />
  <RequestedCommand>namecheap.{command}</RequestedCommand>
  <CommandResponse Type="namecheap.{command}">{inner}</CommandResponse>
  <Server>TEST</Server>
  <GMTTimeDifference>--4:00</GMTTimeDifference>
  <ExecutionTime>0.01</ExecutionTime>
</ApiResponse>"#
    )
}

#[test]
fn check_parses_regular_and_premium_results() {
    let inner = r#"
<DomainCheckResult Domain="taken.example" Available="false" ErrorNo="0" Description="" IsPremiumName="false" PremiumRegistrationPrice="0" PremiumRenewalPrice="0" PremiumRestorePrice="0" PremiumTransferPrice="0" IcannFee="0" EapFee="0"/>
<DomainCheckResult Domain="fancy.example" Available="true" ErrorNo="0" Description="" IsPremiumName="true" PremiumRegistrationPrice="13000.0000" PremiumRenewalPrice="13000.0000" PremiumRestorePrice="65.0000" PremiumTransferPrice="13000.0000" IcannFee="0.0000" EapFee="0.0000"/>"#;
    let transport = FakeTransport::new(vec![envelope("domains.check", inner)]);
    let client = Client::new(transport, test_profile());

    let results = domains::check(
        &client,
        &["taken.example".to_owned(), "fancy.example".to_owned()],
    )
    .expect("check should succeed");

    assert_eq!(results.len(), 2);
    assert!(
        !results[0].available,
        "unavailable domain is data, not error"
    );
    assert!(results[1].available);
    assert!(results[1].is_premium);
    assert_eq!(results[1].premium_registration_price, "13000.0000");

    let requests = client.transport().requests.borrow();
    let first = &requests[0];
    assert_eq!(param(first, "Command"), Some("namecheap.domains.check"));
    assert_eq!(
        param(first, "DomainList"),
        Some("taken.example,fancy.example"),
        "domains must be comma-joined into one call"
    );
}

#[test]
fn lock_status_parses_capitalized_boolean() {
    // Docs say RegistrarLockStatus values are "True"/"False" (capitalized).
    let inner =
        r#"<DomainGetRegistrarLockResult Domain="domain1.example" RegistrarLockStatus="True" />"#;
    let transport = FakeTransport::new(vec![envelope("domains.getRegistrarLock", inner)]);
    let client = Client::new(transport, test_profile());

    let status = domains::lock_status(&client, "domain1.example").expect("lock should succeed");

    assert_eq!(status.domain, "domain1.example");
    assert!(status.locked);
    let requests = client.transport().requests.borrow();
    assert_eq!(
        param(&requests[0], "Command"),
        Some("namecheap.domains.getRegistrarLock")
    );
    assert_eq!(param(&requests[0], "DomainName"), Some("domain1.example"));
}

#[test]
fn balances_redacted_view_hides_amounts() {
    let inner = r#"<UserGetBalancesResult Currency="USD" AvailableBalance="4932.96" AccountBalance="4932.96" EarnedAmount="381.70" WithdrawableAmount="1243.36" FundsRequiredForAutoRenew="120.00" />"#;
    let transport = FakeTransport::new(vec![envelope("users.getBalances", inner)]);
    let client = Client::new(transport, test_profile());

    let balances = account::balances(&client).expect("balances should succeed");
    assert_eq!(balances.available_balance, "4932.96");

    let redacted = account::redacted_view(&balances).to_string();
    assert!(
        !redacted.contains("4932.96") && !redacted.contains("381.70"),
        "redacted view must not carry balance amounts: {redacted}"
    );
    assert!(redacted.contains(r#""available_covers_auto_renew":true"#));
    assert!(
        redacted.contains("120.00"),
        "auto-renew requirement stays visible"
    );
}

#[test]
fn balances_redaction_survives_unparseable_amounts() {
    // The amount format is undocumented across locales; a comma-decimal
    // response must degrade to "unknown", not a wrong answer.
    let inner = r#"<UserGetBalancesResult Currency="EUR" AvailableBalance="4.932,96" AccountBalance="4.932,96" EarnedAmount="0,00" WithdrawableAmount="0,00" FundsRequiredForAutoRenew="120,00" />"#;
    let transport = FakeTransport::new(vec![envelope("users.getBalances", inner)]);
    let client = Client::new(transport, test_profile());

    let balances = account::balances(&client).expect("balances should succeed");
    let redacted = account::redacted_view(&balances);
    assert!(
        redacted["available_covers_auto_renew"].is_null(),
        "unparseable amounts must yield null, got {redacted}"
    );
}
