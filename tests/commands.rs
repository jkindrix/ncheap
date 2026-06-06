mod common;

use common::{FakeTransport, param, test_profile};
use ncheap::api::Client;
use ncheap::commands::{account, dns, domains};

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

#[test]
fn dns_get_fetches_hosts_when_namecheap_is_authoritative() {
    let list_inner = r#"
<DomainDNSGetListResult Domain="domain.com" IsUsingOurDNS="true">
  <Nameserver>dns1.registrar-servers.com</Nameserver>
  <Nameserver>dns2.registrar-servers.com</Nameserver>
</DomainDNSGetListResult>"#;
    // One Host uses HostId, the other HostID: the docs disagree on casing.
    let hosts_inner = r#"
<DomainDNSGetHostsResult Domain="domain.com" IsUsingOurDNS="true">
  <Host HostId="12" Name="@" Type="A" Address="192.0.2.4" MXPref="10" TTL="1800" />
  <Host HostID="14" Name="www" Type="CNAME" Address="domain.com." MXPref="10" TTL="1800" />
</DomainDNSGetHostsResult>"#;
    let transport = FakeTransport::new(vec![
        envelope("domains.dns.getList", list_inner),
        envelope("domains.dns.getHosts", hosts_inner),
    ]);
    let client = Client::new(transport, test_profile());

    let info = dns::get(&client, "domain.com").expect("dns get should succeed");

    assert!(info.is_using_our_dns);
    assert_eq!(info.nameservers.len(), 2);
    let hosts = info.host_records.expect("hosts should be fetched");
    assert_eq!(hosts.len(), 2);
    assert_eq!(hosts[0].id, "12");
    assert_eq!(hosts[1].id, "14", "HostID casing variant must parse");
    assert_eq!(hosts[1].record_type, "CNAME");

    let requests = client.transport().requests.borrow();
    assert_eq!(requests.len(), 2);
    assert_eq!(param(&requests[0], "SLD"), Some("domain"));
    assert_eq!(param(&requests[0], "TLD"), Some("com"));
    assert_eq!(
        param(&requests[1], "Command"),
        Some("namecheap.domains.dns.getHosts")
    );
}

#[test]
fn dns_get_skips_hosts_for_external_dns() {
    let list_inner = r#"
<DomainDNSGetListResult Domain="domain.co.uk" IsUsingOurDNS="false">
  <Nameserver>ns1.external.example</Nameserver>
</DomainDNSGetListResult>"#;
    let transport = FakeTransport::new(vec![envelope("domains.dns.getList", list_inner)]);
    let client = Client::new(transport, test_profile());

    let info = dns::get(&client, "domain.co.uk").expect("dns get should succeed");

    assert!(!info.is_using_our_dns);
    assert!(info.host_records.is_none());
    let requests = client.transport().requests.borrow();
    assert_eq!(requests.len(), 1, "no getHosts call for external DNS");
    assert_eq!(param(&requests[0], "SLD"), Some("domain"));
    assert_eq!(
        param(&requests[0], "TLD"),
        Some("co.uk"),
        "PSL split keeps co.uk whole"
    );
}

#[test]
fn info_parses_nested_structure() {
    let inner = r#"
<DomainGetInfoResult Status="Ok" ID="736542" DomainName="domain1.com" OwnerName="apiuser" IsOwner="true" IsPremium="false">
  <DomainDetails>
    <CreatedDate>09/05/2016</CreatedDate>
    <ExpiredDate>09/05/2027</ExpiredDate>
  </DomainDetails>
  <LockDetails />
  <Whoisguard Enabled="True">
    <ID>3655801</ID>
    <ExpiredDate>01/26/2027</ExpiredDate>
  </Whoisguard>
  <DnsDetails ProviderType="CUSTOM">
    <Nameserver>dns1.registrar-servers.com</Nameserver>
  </DnsDetails>
  <Modificationrights All="true" />
</DomainGetInfoResult>"#;
    let transport = FakeTransport::new(vec![envelope("domains.getinfo", inner)]);
    let client = Client::new(transport, test_profile());

    let info = domains::info(&client, "domain1.com").expect("info should succeed");

    assert_eq!(info.domain, "domain1.com");
    assert_eq!(info.status, "Ok");
    assert!(info.is_owner);
    assert_eq!(info.expires, "09/05/2027");
    let privacy = info.privacy.expect("privacy block present");
    assert_eq!(privacy.enabled, "True");
    assert_eq!(privacy.id, "3655801");
    assert_eq!(info.dns_provider.as_deref(), Some("CUSTOM"));
    assert_eq!(info.nameservers, ["dns1.registrar-servers.com"]);
    assert_eq!(info.modification_rights_all, Some(true));
}

fn contact_xml(email: &str) -> String {
    format!(
        r#"<OrganizationName>ExampleCo</OrganizationName>
<JobTitle>Dev</JobTitle>
<FirstName>John</FirstName>
<LastName>Smith</LastName>
<Address1>8939 S. cross Blvd</Address1>
<Address2 />
<City>california</City>
<StateProvince>ca</StateProvince>
<StateProvinceChoice>P</StateProvinceChoice>
<PostalCode>90045</PostalCode>
<Country>US</Country>
<Phone>+1.6613102107</Phone>
<Fax />
<EmailAddress>{email}</EmailAddress>
<PhoneExt />"#
    )
}

#[test]
fn contacts_redacted_view_hides_pii_and_reports_consistency() {
    let same = contact_xml("john@example.net");
    let different = contact_xml("other@example.net");
    let inner = format!(
        r#"
<DomainContactsResult Domain="domain1.com" domainnameid="3152456">
  <Registrant ReadOnly="false">{same}</Registrant>
  <Tech ReadOnly="false">{same}</Tech>
  <Admin ReadOnly="false">{same}</Admin>
  <AuxBilling ReadOnly="false">{different}</AuxBilling>
</DomainContactsResult>"#
    );
    let transport = FakeTransport::new(vec![envelope("domains.getContacts", &inner)]);
    let client = Client::new(transport, test_profile());

    let contacts = domains::contacts(&client, "domain1.com").expect("contacts should succeed");
    assert_eq!(contacts.registrant.first_name, "John");
    assert_eq!(contacts.aux_billing.email_address, "other@example.net");

    let redacted = domains::contacts_redacted_view(&contacts).to_string();
    for pii in [
        "John",
        "Smith",
        "8939",
        "example.net",
        "+1.6613102107",
        "90045",
    ] {
        assert!(
            !redacted.contains(pii),
            "redacted view must not contain {pii:?}: {redacted}"
        );
    }
    assert!(redacted.contains(r#""all_contact_sets_identical":false"#));
    assert!(redacted.contains(r#""country":"US""#));
}

#[test]
fn dns_get_parses_lowercase_host_elements_from_live_api() {
    // The docs show <Host>; the production API actually returns <host>
    // with extra undocumented attributes. Shape captured from a live
    // response 2026-06-06 (addresses synthetic).
    let list_inner = r#"
<DomainDNSGetListResult Domain="domain.com" IsUsingOurDNS="true">
  <Nameserver>dns1.registrar-servers.com</Nameserver>
</DomainDNSGetListResult>"#;
    let hosts_inner = r#"
<DomainDNSGetHostsResult Domain="domain.com" EmailType="FWD" IsUsingOurDNS="true">
  <host HostId="483088975" Name="www" Type="CNAME" Address="parked.example." MXPref="10" TTL="1800" AssociatedAppTitle="" FriendlyName="CNAME Record" IsActive="true" IsDDNSEnabled="false" />
  <host HostId="483088974" Name="@" Type="URL" Address="https://parked.example/" MXPref="10" TTL="1800" AssociatedAppTitle="URL Forwarding" FriendlyName="URL Record" IsActive="true" IsDDNSEnabled="false" />
</DomainDNSGetHostsResult>"#;
    let transport = FakeTransport::new(vec![
        envelope("domains.dns.getList", list_inner),
        envelope("domains.dns.getHosts", hosts_inner),
    ]);
    let client = Client::new(transport, test_profile());

    let info = dns::get(&client, "domain.com").expect("dns get should succeed");
    let hosts = info.host_records.expect("hosts present");
    assert_eq!(hosts.len(), 2, "lowercase <host> elements must parse");
    assert_eq!(hosts[0].record_type, "CNAME");
    assert_eq!(hosts[1].record_type, "URL");
}
