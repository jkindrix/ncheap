mod common;

use common::{FakeTransport, param, test_client, test_profile};
use ncheap::api::Client;
use ncheap::commands::{account, dns, domains, privacy, raw};

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
    let client = test_client(transport);

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
    let client = test_client(transport);

    let status = domains::lock_status(&client, "domain1.example").expect("lock should succeed");

    assert_eq!(status.domain, "domain1.example");
    assert!(status.locked);
    let requests = client.transport().requests.borrow();
    assert_eq!(
        param(&requests[0], "Command"),
        Some("namecheap.domains.getregistrarlock")
    );
    assert_eq!(param(&requests[0], "DomainName"), Some("domain1.example"));
}

#[test]
fn balances_redacted_view_hides_amounts() {
    let inner = r#"<UserGetBalancesResult Currency="USD" AvailableBalance="4932.96" AccountBalance="4932.96" EarnedAmount="381.70" WithdrawableAmount="1243.36" FundsRequiredForAutoRenew="120.00" />"#;
    let transport = FakeTransport::new(vec![envelope("users.getBalances", inner)]);
    let client = test_client(transport);

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
    let client = test_client(transport);

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
    let client = test_client(transport);

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
        Some("namecheap.domains.dns.gethosts")
    );
}

#[test]
fn dns_get_skips_hosts_for_external_dns() {
    let list_inner = r#"
<DomainDNSGetListResult Domain="domain.co.uk" IsUsingOurDNS="false">
  <Nameserver>ns1.external.example</Nameserver>
</DomainDNSGetListResult>"#;
    let transport = FakeTransport::new(vec![envelope("domains.dns.getList", list_inner)]);
    let client = test_client(transport);

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
    let client = test_client(transport);

    let info = domains::info(&client, "domain1.com").expect("info should succeed");

    assert_eq!(info.domain, "domain1.com");
    assert_eq!(info.status, "Ok");
    assert!(info.is_owner);
    assert_eq!(info.expires, "2027-09-05", "dates are ISO-8601");
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
    let client = test_client(transport);

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
    let client = test_client(transport);

    let info = dns::get(&client, "domain.com").expect("dns get should succeed");
    let hosts = info.host_records.expect("hosts present");
    assert_eq!(hosts.len(), 2, "lowercase <host> elements must parse");
    assert_eq!(hosts[0].record_type, "CNAME");
    assert_eq!(hosts[1].record_type, "URL");
}

#[test]
fn privacy_list_paginates_and_parses() {
    fn privacy_page(ids: &[u32], total: usize) -> String {
        let subs: String = ids
            .iter()
            .map(|i| {
                format!(
                    r#"<Whoisguard ID="{i}" DomainName="d{i}.example" Created="05/13/2025" Expires="05/13/2027" Status="ENABLED" />"#
                )
            })
            .collect();
        format!(
            r#"<WhoisguardGetListResult>{subs}</WhoisguardGetListResult>
<Paging><TotalItems>{total}</TotalItems><CurrentPage>1</CurrentPage><PageSize>100</PageSize></Paging>"#
        )
    }
    let first: Vec<u32> = (0..100).collect();
    let second: Vec<u32> = (100..124).collect();
    let transport = FakeTransport::new(vec![
        envelope("whoisguard.getList", &privacy_page(&first, 124)),
        envelope("whoisguard.getList", &privacy_page(&second, 124)),
    ]);
    let client = test_client(transport);

    let subs = privacy::list(&client).expect("privacy list should succeed");

    assert_eq!(subs.len(), 124, "all subscriptions must survive pagination");
    assert_eq!(subs[0].status, "ENABLED");
    assert_eq!(subs[123].domain_name, "d123.example");
    let requests = client.transport().requests.borrow();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        param(&requests[0], "Command"),
        Some("namecheap.whoisguard.getlist")
    );
    assert_eq!(param(&requests[1], "Page"), Some("2"));
}

fn pricing_inner() -> &'static str {
    r#"
<UserGetPricingResult>
  <ProductType Name="DOMAIN">
    <ProductCategory Name="REGISTER">
      <Product Name="biz">
        <Price Duration="1" DurationType="YEAR" Price="6.00" RegularPrice="8.55" YourPrice="6.00" CouponPrice="" Currency="USD" />
        <Price Duration="2" DurationType="YEAR" Price="8.87" RegularPrice="8.87" YourPrice="8.87" CouponPrice="" Currency="USD" />
      </Product>
    </ProductCategory>
    <ProductCategory Name="RENEW">
      <Product Name="biz">
        <Price Duration="1" DurationType="YEAR" Price="9.99" RegularPrice="9.99" YourPrice="9.99" CouponPrice="" Currency="USD" />
      </Product>
    </ProductCategory>
  </ProductType>
</UserGetPricingResult>"#
}

#[test]
fn pricing_flattens_nested_tree_and_sends_filters() {
    let transport = FakeTransport::new(vec![envelope("users.getPricing", pricing_inner())]);
    let client = test_client(transport);
    let query = account::PricingQuery {
        product_type: "DOMAIN".into(),
        category: None,
        action: Some("REGISTER".into()),
        product: Some("biz".into()),
    };

    let (rows, cached) = account::pricing(&client, &query, None).expect("pricing should succeed");

    assert!(!cached);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].category, "REGISTER");
    assert_eq!(rows[0].your_price, "6.00");
    assert_eq!(rows[2].category, "RENEW");
    let requests = client.transport().requests.borrow();
    assert_eq!(param(&requests[0], "ProductType"), Some("DOMAIN"));
    assert_eq!(param(&requests[0], "ActionName"), Some("REGISTER"));
    assert_eq!(param(&requests[0], "ProductName"), Some("biz"));
    assert_eq!(param(&requests[0], "ProductCategory"), None);
}

#[test]
fn pricing_second_call_hits_cache_without_api_call() {
    let cache_dir = std::env::temp_dir().join(format!("ncheap-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache_dir);

    let transport = FakeTransport::new(vec![envelope("users.getPricing", pricing_inner())]);
    let client = test_client(transport);
    let query = account::PricingQuery {
        product_type: "DOMAIN".into(),
        category: None,
        action: None,
        product: None,
    };

    let (rows, cached) = account::pricing(&client, &query, Some(&cache_dir)).expect("first call");
    assert!(!cached);
    assert_eq!(client.calls(), 1);
    assert_eq!(rows.len(), 3);

    // Second call: no fixture responses remain, so any API call would error.
    let (rows2, cached2) =
        account::pricing(&client, &query, Some(&cache_dir)).expect("second call");
    assert!(cached2, "second call must come from cache");
    assert_eq!(client.calls(), 1, "no additional API call");
    assert_eq!(rows2.len(), rows.len());

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn raw_calls_allowlisted_command_and_returns_xml() {
    let inner = r#"<Tlds><Tld Name="com" /></Tlds>"#;
    let transport = FakeTransport::new(vec![envelope("domains.gettldlist", inner)]);
    let client = test_client(transport);

    let body = raw::call(&client, "namecheap.Domains.getTldList", &[]).expect("raw should succeed");

    assert!(body.contains("<Tlds>"), "raw XML body is returned verbatim");
    let requests = client.transport().requests.borrow();
    assert_eq!(
        param(&requests[0], "Command"),
        Some("namecheap.domains.gettldlist"),
        "prefix stripped, case-folded, re-prefixed"
    );
}

#[test]
fn raw_forwards_params() {
    let inner = r#"<DomainGetRegistrarLockResult Domain="d.example" RegistrarLockStatus="True" />"#;
    let transport = FakeTransport::new(vec![envelope("domains.getRegistrarLock", inner)]);
    let client = test_client(transport);
    let params = raw::parse_params(&["DomainName=d.example".to_owned()]).expect("parse");

    raw::call(&client, "domains.getRegistrarLock", &params).expect("raw should succeed");

    let requests = client.transport().requests.borrow();
    assert_eq!(param(&requests[0], "DomainName"), Some("d.example"));
}

#[test]
fn raw_rejects_non_allowlisted_command_without_calling() {
    let transport = FakeTransport::new(vec![]);
    let client = test_client(transport);

    let err = raw::call(&client, "domains.dns.setCustom", &[]).expect_err("must be rejected");

    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains("read-only allowlist"));
    assert_eq!(
        client.transport().requests.borrow().len(),
        0,
        "no API call may be made for a rejected command"
    );
}

#[test]
fn raw_rejects_reserved_params() {
    for p in [
        "ApiKey=x",
        "Command=namecheap.domains.dns.setCustom",
        "clientip=1.2.3.4",
    ] {
        let err = raw::parse_params(&[p.to_owned()]).expect_err("must be rejected");
        assert_eq!(err.exit_code(), 2, "{p} must be a usage error");
        assert!(err.to_string().contains("reserved"));
    }
    let err = raw::parse_params(&["NoEqualsSign".to_owned()]).expect_err("must be rejected");
    assert!(err.to_string().contains("KEY=VALUE"));
}

#[test]
fn raw_maps_api_error_envelope_to_error() {
    let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ApiResponse xmlns="http://api.namecheap.com/xml.response" Status="ERROR">
  <Errors><Error Number="2030280">TLD is not supported</Error></Errors>
  <RequestedCommand>namecheap.domains.gettldlist</RequestedCommand>
  <Server>TEST</Server>
</ApiResponse>"#;
    let transport = FakeTransport::new(vec![body.to_owned()]);
    let client = test_client(transport);

    let err = raw::call(&client, "domains.getTldList", &[]).expect_err("must surface API error");

    assert_eq!(err.exit_code(), 1);
    assert_eq!(err.code(), Some("2030280"));
}

#[test]
fn transient_500_is_retried_once_and_counts_one_logical_call() {
    use ncheap::api::TransportFailure;
    let inner = r#"<UserGetBalancesResult Currency="USD" AvailableBalance="1.00" AccountBalance="1.00" EarnedAmount="0.00" WithdrawableAmount="0.00" FundsRequiredForAutoRenew="0.00" />"#;
    let transport = FakeTransport::with_results(vec![
        Err(TransportFailure::Status(500)),
        Ok(envelope("users.getBalances", inner)),
    ]);
    let client = test_client(transport);

    let balances = account::balances(&client).expect("retry should recover");

    assert_eq!(balances.currency, "USD");
    assert_eq!(
        client.transport().requests.borrow().len(),
        2,
        "two transport attempts"
    );
    assert_eq!(client.calls(), 1, "but one logical API call in meta");
}

#[test]
fn persistent_429_maps_to_rate_limited_exit_5() {
    use ncheap::api::TransportFailure;
    let transport = FakeTransport::with_results(vec![
        Err(TransportFailure::Status(429)),
        Err(TransportFailure::Status(429)),
    ]);
    let client = test_client(transport);

    let err = account::balances(&client).expect_err("must fail after one retry");

    assert_eq!(err.exit_code(), 5);
    assert_eq!(err.kind(), "rate_limit");
    assert_eq!(client.transport().requests.borrow().len(), 2);
}

// --- Mutation gate (Client::call / call_mut) ---

fn profile_with(sandbox: bool, allow_mut: bool) -> ncheap::config::Profile {
    let mut p = test_profile();
    p.sandbox = sandbox;
    p.allow_production_mutations = allow_mut;
    p
}

fn gate_client(transport: FakeTransport, sandbox: bool, allow_mut: bool) -> Client<FakeTransport> {
    let mut client = Client::new(transport, profile_with(sandbox, allow_mut));
    client.set_timing(std::time::Duration::ZERO, std::time::Duration::ZERO);
    client
}

#[test]
fn call_refuses_unknown_commands_fail_closed() {
    let client = gate_client(FakeTransport::new(vec![]), true, false);

    let err = client
        .call("domains.dns.setHosts", &[])
        .expect_err("read path must refuse non-read commands");

    assert_eq!(err.exit_code(), 3);
    assert_eq!(err.kind(), "config");
    assert_eq!(client.transport().requests.borrow().len(), 0);
}

#[test]
fn call_mut_is_refused_on_production_without_opt_in() {
    let client = gate_client(FakeTransport::new(vec![]), false, false);

    let err = client
        .call_mut("domains.dns.setHosts", &[])
        .expect_err("production mutation must be gated");

    assert_eq!(err.exit_code(), 3);
    assert!(err.to_string().contains("allow_production_mutations"));
    assert_eq!(client.transport().requests.borrow().len(), 0);
}

#[test]
fn call_mut_dispatches_on_sandbox_and_does_not_retry() {
    use ncheap::api::TransportFailure;
    // A 500 on a mutation must surface immediately, never re-submit.
    let transport = FakeTransport::with_results(vec![Err(TransportFailure::Status(500))]);
    let client = gate_client(transport, true, false);

    let err = client
        .call_mut("domains.dns.setHosts", &[("SLD", "d"), ("TLD", "com")])
        .expect_err("500 surfaces");

    assert_eq!(err.exit_code(), 4);
    assert_eq!(
        client.transport().requests.borrow().len(),
        1,
        "exactly one attempt: mutations never auto-retry"
    );
}

#[test]
fn call_mut_dispatches_on_production_with_explicit_opt_in() {
    let inner = r#"<DomainDNSSetHostsResult Domain="d.com" IsSuccess="true" />"#;
    let transport = FakeTransport::new(vec![envelope("domains.dns.setHosts", inner)]);
    let client = gate_client(transport, false, true);

    let body = client
        .call_mut("domains.dns.setHosts", &[])
        .expect("explicit opt-in permits production mutation");

    assert!(body.contains("IsSuccess"));
    let requests = client.transport().requests.borrow();
    assert_eq!(
        param(&requests[0], "Command"),
        Some("namecheap.domains.dns.sethosts")
    );
}

#[test]
fn pricing_cache_is_keyed_by_profile() {
    let cache_dir = std::env::temp_dir().join(format!("ncheap-profkey-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache_dir);
    let query = account::PricingQuery {
        product_type: "DOMAIN".into(),
        category: None,
        action: None,
        product: None,
    };

    let client = test_client(FakeTransport::new(vec![envelope(
        "users.getPricing",
        pricing_inner(),
    )]));
    let (_, cached) = account::pricing(&client, &query, Some(&cache_dir)).expect("first");
    assert!(!cached);

    // Same query, different profile: must NOT be served profile A's prices.
    let mut other_profile = test_profile();
    other_profile.name = "other".into();
    other_profile.sandbox = false;
    let mut other_client = Client::new(
        FakeTransport::new(vec![envelope("users.getPricing", pricing_inner())]),
        other_profile,
    );
    other_client.set_timing(std::time::Duration::ZERO, std::time::Duration::ZERO);
    let (_, cached) = account::pricing(&other_client, &query, Some(&cache_dir)).expect("second");
    assert!(!cached, "cache must be keyed by profile and sandbox flag");
    assert_eq!(other_client.calls(), 1, "other profile fetched live");

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn dns_set_fetches_pre_image_then_mutates() {
    let list_inner = r#"
<DomainDNSGetListResult Domain="domain.com" IsUsingOurDNS="true">
  <Nameserver>dns1.registrar-servers.com</Nameserver>
  <Nameserver>dns2.registrar-servers.com</Nameserver>
</DomainDNSGetListResult>"#;
    let inner = r#"<DomainDNSSetCustomResult Domain="domain.com" Updated="true" />"#;
    let transport = FakeTransport::new(vec![
        envelope("domains.dns.getList", list_inner),
        envelope("domains.dns.setCustom", inner),
    ]);
    let client = test_client(transport); // sandbox profile: gate permits

    let ns = vec!["ns1.example.net".to_owned(), "ns2.example.net".to_owned()];
    let result = dns::set(&client, "domain.com", &ns).expect("set should succeed");

    assert!(result.updated);
    assert_eq!(result.domain, "domain.com");
    assert_eq!(
        result.previous_nameservers,
        ["dns1.registrar-servers.com", "dns2.registrar-servers.com"],
        "pre-image captured for undo"
    );
    let requests = client.transport().requests.borrow();
    assert_eq!(requests.len(), 2, "pre-image read then mutation");
    assert_eq!(
        param(&requests[1], "Command"),
        Some("namecheap.domains.dns.setcustom")
    );
    assert_eq!(param(&requests[1], "SLD"), Some("domain"));
    assert_eq!(param(&requests[1], "TLD"), Some("com"));
    assert_eq!(
        param(&requests[1], "NameServers"),
        Some("ns1.example.net,ns2.example.net")
    );
}

#[test]
fn dns_set_is_gated_on_production_without_opt_in() {
    let client = gate_client(FakeTransport::new(vec![]), false, false);

    let ns = vec!["ns1.example.net".to_owned(), "ns2.example.net".to_owned()];
    let err = dns::set(&client, "domain.com", &ns).expect_err("must be gated");

    assert_eq!(err.exit_code(), 3);
    assert_eq!(client.transport().requests.borrow().len(), 0);
}

#[test]
fn api_error_with_junk_command_response_surfaces_the_real_error() {
    // Shape captured live from sandbox setCustom 2026-06-06: Status=ERROR
    // still carries a CommandResponse whose attributes are empty strings.
    // The typed parse must not run first and mask the API error.
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<ApiResponse Status="ERROR" xmlns="http://api.namecheap.com/xml.response">
  <Errors>
    <Error Number="3031166">Not allowed host: ns1.example.net.</Error>
  </Errors>
  <Warnings />
  <RequestedCommand>namecheap.domains.dns.setcustom</RequestedCommand>
  <CommandResponse Type="namecheap.domains.dns.setCustom">
    <DomainDNSSetCustomResult Domain="" Updated="" />
  </CommandResponse>
  <Server>TEST</Server>
</ApiResponse>"#;
    let transport = FakeTransport::new(vec![body.to_owned()]);
    let client = test_client(transport);

    let ns = vec!["ns1.example.net".to_owned(), "ns2.example.net".to_owned()];
    let err = dns::set(&client, "domain.com", &ns).expect_err("API error must surface");

    assert_eq!(
        err.code(),
        Some("3031166"),
        "real error, not a parse failure: {err}"
    );
    assert!(err.to_string().contains("Not allowed host"));
}

fn privacy_list_page_with(domain: &str, id: &str) -> String {
    envelope(
        "whoisguard.getList",
        &format!(
            r#"<WhoisguardGetListResult>
<Whoisguard ID="{id}" DomainName="{domain}" Created="05/13/2025" Expires="05/13/2027" Status="ENABLED" />
</WhoisguardGetListResult>
<Paging><TotalItems>1</TotalItems><CurrentPage>1</CurrentPage><PageSize>100</PageSize></Paging>"#
        ),
    )
}

#[test]
fn privacy_enable_resolves_id_and_sends_forward_address() {
    let enable_inner = r#"<WhoisguardEnableResult DomainName="d1.example" IsSuccess="true" />"#;
    let transport = FakeTransport::new(vec![
        privacy_list_page_with("d1.example", "5924316"),
        envelope("whoisguard.enable", enable_inner),
    ]);
    let client = test_client(transport);

    let result = privacy::enable(&client, "d1.example", "ops@example.org").expect("enable");

    assert!(result.is_success);
    assert!(result.enabled);
    assert_eq!(result.privacy_id, "5924316");
    let requests = client.transport().requests.borrow();
    assert_eq!(requests.len(), 2, "one read to resolve, one mutation");
    assert_eq!(
        param(&requests[1], "Command"),
        Some("namecheap.whoisguard.enable")
    );
    assert_eq!(param(&requests[1], "WhoisguardID"), Some("5924316"));
    assert_eq!(
        param(&requests[1], "ForwardedToEmail"),
        Some("ops@example.org")
    );
}

#[test]
fn privacy_disable_resolves_id() {
    let disable_inner = r#"<WhoisguardDisableResult DomainName="d1.example" IsSuccess="true" />"#;
    let transport = FakeTransport::new(vec![
        privacy_list_page_with("d1.example", "5924316"),
        envelope("whoisguard.disable", disable_inner),
    ]);
    let client = test_client(transport);

    let result = privacy::disable(&client, "d1.example").expect("disable");

    assert!(result.is_success);
    assert!(!result.enabled);
    let requests = client.transport().requests.borrow();
    assert_eq!(param(&requests[1], "WhoisguardID"), Some("5924316"));
    assert_eq!(param(&requests[1], "ForwardedToEmail"), None);
}

#[test]
fn privacy_enable_without_subscription_is_a_usage_error() {
    let transport = FakeTransport::new(vec![privacy_list_page_with("other.example", "111")]);
    let client = test_client(transport);

    let err = privacy::enable(&client, "d1.example", "ops@example.org").expect_err("no sub");

    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains("d1.example"));
    assert_eq!(
        client.transport().requests.borrow().len(),
        1,
        "only the resolution read, no mutation attempt"
    );
}

#[test]
fn privacy_enable_is_gated_on_production_without_opt_in() {
    let transport = FakeTransport::new(vec![privacy_list_page_with("d1.example", "5924316")]);
    let client = gate_client(transport, false, false);

    let err = privacy::enable(&client, "d1.example", "ops@example.org").expect_err("gated");

    assert_eq!(err.exit_code(), 3);
    assert_eq!(
        client.transport().requests.borrow().len(),
        0,
        "gate fires before ANY traffic: no rate budget spent, no intent leaked"
    );
}

// --- register / renew price guards ---

fn check_available_inner(domain: &str, premium: bool) -> String {
    format!(
        r#"<DomainCheckResult Domain="{domain}" Available="true" ErrorNo="0" Description="" IsPremiumName="{premium}" PremiumRegistrationPrice="0" PremiumRenewalPrice="0" PremiumRestorePrice="0" PremiumTransferPrice="0" IcannFee="0" EapFee="0"/>"#
    )
}

fn pricing_com_inner(action: &str, your_price: &str) -> String {
    format!(
        r#"<UserGetPricingResult>
  <ProductType Name="domains">
    <ProductCategory Name="{action}">
      <Product Name="com">
        <Price Duration="1" DurationType="YEAR" Price="{your_price}" RegularPrice="{your_price}" YourPrice="{your_price}" CouponPrice="" Currency="USD" />
      </Product>
    </ProductCategory>
  </ProductType>
</UserGetPricingResult>"#
    )
}

fn contacts_inner(domain: &str) -> String {
    let c = contact_xml("owner@example.org");
    format!(
        r#"<DomainContactsResult Domain="{domain}" domainnameid="1">
  <Registrant ReadOnly="false">{c}</Registrant>
  <Tech ReadOnly="false">{c}</Tech>
  <Admin ReadOnly="false">{c}</Admin>
  <AuxBilling ReadOnly="false">{c}</AuxBilling>
</DomainContactsResult>"#
    )
}

#[test]
fn register_happy_path_guards_then_mutates() {
    let create_inner = r#"<DomainCreateResult Domain="newdomain.com" Registered="true" ChargedAmount="14.1800" DomainID="42" OrderID="7" TransactionID="9" WhoisguardEnable="false" NonRealTimeDomain="false"/>"#;
    let transport = FakeTransport::new(vec![
        envelope(
            "domains.check",
            &check_available_inner("newdomain.com", false),
        ),
        envelope("users.getPricing", &pricing_com_inner("register", "11.28")),
        envelope("domains.getContacts", &contacts_inner("owned.com")),
        envelope("domains.create", create_inner),
    ]);
    let client = test_client(transport);

    let result =
        domains::register(&client, "newdomain.com", 1, 15.0, "owned.com").expect("register");

    assert!(result.registered);
    assert_eq!(result.listed_price, "11.28");
    assert_eq!(result.charged_amount, "14.1800");
    let requests = client.transport().requests.borrow();
    assert_eq!(requests.len(), 4, "check, pricing, contacts, create");
    let create = &requests[3];
    assert_eq!(param(create, "Command"), Some("namecheap.domains.create"));
    assert_eq!(param(create, "DomainName"), Some("newdomain.com"));
    assert_eq!(param(create, "Years"), Some("1"));
    assert_eq!(param(create, "RegistrantFirstName"), Some("John"));
    assert_eq!(
        param(create, "AuxBillingEmailAddress"),
        Some("owner@example.org")
    );
}

#[test]
fn register_refuses_when_price_exceeds_cap_before_contacts_or_mutation() {
    let transport = FakeTransport::new(vec![
        envelope(
            "domains.check",
            &check_available_inner("newdomain.com", false),
        ),
        envelope("users.getPricing", &pricing_com_inner("register", "11.28")),
    ]);
    let client = test_client(transport);

    let err =
        domains::register(&client, "newdomain.com", 1, 10.0, "owned.com").expect_err("price guard");

    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains("exceeds --max-price"));
    assert_eq!(
        client.transport().requests.borrow().len(),
        2,
        "stops at the guard: no contacts read, no mutation"
    );
}

#[test]
fn register_refuses_unavailable_and_premium_domains() {
    let taken = r#"<DomainCheckResult Domain="taken.com" Available="false" ErrorNo="0" Description="" IsPremiumName="false"/>"#;
    let transport = FakeTransport::new(vec![envelope("domains.check", taken)]);
    let client = test_client(transport);
    let err = domains::register(&client, "taken.com", 1, 15.0, "owned.com").expect_err("taken");
    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains("not available"));

    let transport = FakeTransport::new(vec![envelope(
        "domains.check",
        &check_available_inner("premium.com", true),
    )]);
    let client = test_client(transport);
    let err = domains::register(&client, "premium.com", 1, 15.0, "owned.com").expect_err("premium");
    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains("premium"));
}

#[test]
fn renew_guards_price_then_mutates() {
    let renew_inner = r#"<DomainRenewResult DomainName="owned.com" DomainID="42" Renew="true" OrderID="7" TransactionID="9" ChargedAmount="14.9800"><DomainDetails><ExpiredDate>4/30/2028</ExpiredDate><NumYears>0</NumYears></DomainDetails></DomainRenewResult>"#;
    let transport = FakeTransport::new(vec![
        envelope("users.getPricing", &pricing_com_inner("renew", "14.98")),
        envelope("domains.renew", renew_inner),
    ]);
    let client = test_client(transport);

    let result = domains::renew(&client, "owned.com", 1, 20.0).expect("renew");

    assert!(result.renewed);
    assert_eq!(result.charged_amount, "14.9800");
    let requests = client.transport().requests.borrow();
    assert_eq!(
        param(&requests[1], "Command"),
        Some("namecheap.domains.renew")
    );

    // and the refusal side
    let transport = FakeTransport::new(vec![envelope(
        "users.getPricing",
        &pricing_com_inner("renew", "14.98"),
    )]);
    let client = test_client(transport);
    let err = domains::renew(&client, "owned.com", 1, 10.0).expect_err("guard");
    assert_eq!(err.exit_code(), 2);
    assert_eq!(client.transport().requests.borrow().len(), 1, "no mutation");
}

#[test]
fn register_is_gated_on_production_without_opt_in() {
    let transport = FakeTransport::new(vec![
        envelope(
            "domains.check",
            &check_available_inner("newdomain.com", false),
        ),
        envelope("users.getPricing", &pricing_com_inner("register", "11.28")),
        envelope("domains.getContacts", &contacts_inner("owned.com")),
    ]);
    let client = gate_client(transport, false, false);

    let err = domains::register(&client, "newdomain.com", 1, 15.0, "owned.com").expect_err("gated");

    assert_eq!(err.exit_code(), 3);
    assert_eq!(
        client.transport().requests.borrow().len(),
        0,
        "gate fires before ANY traffic: no rate budget spent, no intent leaked"
    );
}

#[test]
fn deeply_nested_xml_fails_or_parses_without_crashing() {
    // Bounded by ureq's 10MB body cap in production; this pins that a deep
    // document within that budget errors cleanly instead of blowing the
    // stack during the IgnoredAny envelope pass.
    let depth = 50_000;
    let mut inner = String::with_capacity(depth * 8);
    for _ in 0..depth {
        inner.push_str("<a>");
    }
    for _ in 0..depth {
        inner.push_str("</a>");
    }
    let transport = FakeTransport::new(vec![envelope("domains.getList", &inner)]);
    let client = test_client(transport);

    // Any Result is acceptable; the assertion is "no crash".
    let _ = domains::list(&client);
}

// --- punch-list regression tests (F1, F3, F4, F18) ---

#[test]
fn drifted_create_response_fails_as_parse_not_false_negative() {
    // A charge may have completed; a missing/renamed outcome attribute must
    // surface as a parse error (reconcile-first), never "registered: false".
    let drifted = r#"<DomainCreateResult Domain="newdomain.com" registered="true" ChargedAmount="14.18" DomainID="1" OrderID="2" TransactionID="3"/>"#;
    let transport = FakeTransport::new(vec![
        envelope(
            "domains.check",
            &check_available_inner("newdomain.com", false),
        ),
        envelope("users.getPricing", &pricing_com_inner("register", "11.28")),
        envelope("domains.getContacts", &contacts_inner("owned.com")),
        envelope("domains.create", drifted),
    ]);
    let client = test_client(transport);

    let err = domains::register(&client, "newdomain.com", 1, 15.0, "owned.com")
        .expect_err("drift must fail");
    assert_eq!(err.kind(), "parse", "got: {err}");
}

#[test]
fn price_guard_matches_action_category_not_first_duration_row() {
    // Fixture carries REGISTER (11.28) before RENEW (14.98); a renew guard
    // capped at 12 must see the RENEW price and refuse — matching the first
    // duration row would wrongly approve.
    let transport = FakeTransport::new(vec![envelope(
        "users.getPricing",
        pricing_inner(), // register 6.00/8.87 rows first, renew 9.99 after
    )]);
    let client = test_client(transport);

    let err = domains::renew(&client, "owned.biz", 1, 8.0).expect_err("renew price is 9.99");
    assert!(
        err.to_string().contains("9.99"),
        "guard must price against the RENEW row, not the first duration match: {err}"
    );
}

#[test]
fn register_refuses_eap_domains() {
    let eap = r#"<DomainCheckResult Domain="eap.com" Available="true" ErrorNo="0" Description="" IsPremiumName="false" EapFee="220.0000"/>"#;
    let transport = FakeTransport::new(vec![envelope("domains.check", eap)]);
    let client = test_client(transport);

    let err = domains::register(&client, "eap.com", 1, 15.0, "owned.com").expect_err("EAP");
    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains("Early Access"));
    assert_eq!(
        client.transport().requests.borrow().len(),
        1,
        "stops at check"
    );
}

#[test]
fn charge_exceeding_cap_sets_loud_flag() {
    let create_inner = r#"<DomainCreateResult Domain="newdomain.com" Registered="true" ChargedAmount="15.43" DomainID="42" OrderID="7" TransactionID="9"/>"#;
    let transport = FakeTransport::new(vec![
        envelope(
            "domains.check",
            &check_available_inner("newdomain.com", false),
        ),
        envelope("users.getPricing", &pricing_com_inner("register", "11.28")),
        envelope("domains.getContacts", &contacts_inner("owned.com")),
        envelope("domains.create", create_inner),
    ]);
    let client = test_client(transport);

    let result =
        domains::register(&client, "newdomain.com", 1, 15.0, "owned.com").expect("registers");
    assert!(result.registered);
    assert!(
        result.charged_exceeded_max_price,
        "15.43 charged > 15.00 cap must be flagged"
    );
}

#[test]
fn short_test_keys_are_not_mangled_by_redaction() {
    use ncheap::api::TransportFailure;
    let transport = FakeTransport::with_results(vec![
        Err(TransportFailure::Other("failed to lookup address".into())),
        Err(TransportFailure::Other("failed to lookup address".into())),
    ]);
    // test_profile key is "testkey" (7 chars, < 8): must not be substring-replaced
    let client = test_client(transport);

    let err = account::balances(&client).expect_err("transport error");
    assert!(
        err.to_string().contains("failed to lookup address"),
        "short key must not mangle the message: {err}"
    );
}

// --- mutation journal (F2) and domains lock set ---

fn journal_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ncheap-journal-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn mutations_write_intent_and_outcome_records() {
    let dir = journal_dir("happy");
    let inner = r#"<WhoisguardEnableResult DomainName="d1.example" IsSuccess="true" />"#;
    let transport = FakeTransport::new(vec![
        privacy_list_page_with("d1.example", "5924316"),
        envelope("whoisguard.enable", inner),
    ]);
    let mut client = Client::new(transport, test_profile());
    client.set_timing(std::time::Duration::ZERO, std::time::Duration::ZERO);
    client.set_journal_dir(Some(dir.clone()));

    privacy::enable(&client, "d1.example", "ops@example.org").expect("enable");

    let log = std::fs::read_to_string(dir.join("mutations.jsonl")).expect("journal exists");
    let records: Vec<serde_json::Value> = log
        .lines()
        .map(|l| serde_json::from_str(l).expect("valid JSONL"))
        .collect();
    assert_eq!(records.len(), 2, "intent + outcome: {log}");
    assert_eq!(records[0]["kind"], "intent");
    assert_eq!(records[0]["command"], "whoisguard.enable");
    assert_eq!(records[0]["params"]["WhoisguardID"], "5924316");
    assert!(
        !log.contains("testkey"),
        "journal must never contain the key"
    );
    assert_eq!(records[1]["kind"], "outcome");
    assert_eq!(records[1]["ok"], true);
    assert_eq!(records[0]["seq"], records[1]["seq"], "linkable pair");

    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(dir.join("mutations.jsonl"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "journal is 0600");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unwritable_journal_refuses_the_mutation_fail_closed() {
    // Point the journal dir at a regular FILE so create_dir_all fails.
    let bogus = std::env::temp_dir().join(format!("ncheap-journal-file-{}", std::process::id()));
    std::fs::write(&bogus, b"not a dir").unwrap();
    let transport = FakeTransport::new(vec![privacy_list_page_with("d1.example", "5924316")]);
    let mut client = Client::new(transport, test_profile());
    client.set_timing(std::time::Duration::ZERO, std::time::Duration::ZERO);
    client.set_journal_dir(Some(bogus.clone()));

    let err = privacy::enable(&client, "d1.example", "ops@example.org").expect_err("refused");

    assert_eq!(err.exit_code(), 3);
    assert!(err.to_string().contains("journal"));
    assert_eq!(
        client.transport().requests.borrow().len(),
        1,
        "the resolution read happened; the mutation itself was never sent"
    );
    let _ = std::fs::remove_file(&bogus);
}

#[test]
fn reads_are_not_journaled() {
    let dir = journal_dir("reads");
    let transport = FakeTransport::new(vec![privacy_list_page_with("d1.example", "1")]);
    let mut client = Client::new(transport, test_profile());
    client.set_timing(std::time::Duration::ZERO, std::time::Duration::ZERO);
    client.set_journal_dir(Some(dir.clone()));

    privacy::list(&client).expect("list");

    assert!(
        !dir.join("mutations.jsonl").exists(),
        "read-only commands leave no journal"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lock_set_reads_pre_image_then_mutates() {
    let status_inner =
        r#"<DomainGetRegistrarLockResult Domain="d.example" RegistrarLockStatus="True" />"#;
    let set_inner = r#"<DomainSetRegistrarLockResult Domain="d.example" IsSuccess="true" />"#;
    let transport = FakeTransport::new(vec![
        envelope("domains.getRegistrarLock", status_inner),
        envelope("domains.setRegistrarLock", set_inner),
    ]);
    let client = test_client(transport);

    let result = domains::set_lock(&client, "d.example", false).expect("unlock");

    assert!(result.is_success);
    assert!(!result.locked);
    assert!(result.previously_locked, "pre-image captured");
    let requests = client.transport().requests.borrow();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        param(&requests[1], "Command"),
        Some("namecheap.domains.setregistrarlock")
    );
    assert_eq!(param(&requests[1], "LockAction"), Some("UNLOCK"));
}

#[test]
fn lock_set_is_gated_on_production_without_opt_in() {
    let client = gate_client(FakeTransport::new(vec![]), false, false);
    let err = domains::set_lock(&client, "d.example", true).expect_err("gated");
    assert_eq!(err.exit_code(), 3);
    assert_eq!(client.transport().requests.borrow().len(), 0);
}

// --- daily spend cap ---

fn spend_client(
    transport: FakeTransport,
    sandbox: bool,
    cap: Option<f64>,
    dir: &std::path::Path,
) -> Client<FakeTransport> {
    let mut profile = test_profile();
    profile.sandbox = sandbox;
    profile.allow_production_mutations = !sandbox; // armed when production
    profile.max_daily_spend = cap;
    let mut client = Client::new(transport, profile);
    client.set_timing(std::time::Duration::ZERO, std::time::Duration::ZERO);
    client.set_journal_dir(Some(dir.to_path_buf()));
    client
}

fn renew_fixtures() -> Vec<String> {
    vec![
        envelope("users.getPricing", &pricing_com_inner("renew", "14.98")),
        envelope(
            "domains.renew",
            r#"<DomainRenewResult DomainName="owned.com" DomainID="42" Renew="true" OrderID="7" TransactionID="9" ChargedAmount="14.98"/>"#,
        ),
    ]
}

#[test]
fn production_purchase_without_cap_is_refused() {
    let dir = journal_dir("nocap");
    let transport = FakeTransport::new(vec![envelope(
        "users.getPricing",
        &pricing_com_inner("renew", "14.98"),
    )]);
    let client = spend_client(transport, false, None, &dir);

    let err = domains::renew(&client, "owned.com", 1, 20.0).expect_err("no cap, no purchase");

    assert_eq!(err.exit_code(), 3);
    assert!(err.to_string().contains("max_daily_spend"));
    assert_eq!(
        client.transport().requests.borrow().len(),
        1,
        "price read happened; the renew itself was never sent"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn spend_cap_accumulates_and_refuses_when_exceeded() {
    let dir = journal_dir("cap");
    // First renew: 14.98 against a 20.00 cap — proceeds.
    let client = spend_client(
        FakeTransport::new(renew_fixtures()),
        false,
        Some(20.0),
        &dir,
    );
    domains::renew(&client, "owned.com", 1, 20.0).expect("first renew under cap");
    let ledger = std::fs::read_to_string(dir.join("spend.jsonl")).expect("ledger");
    assert_eq!(ledger.lines().count(), 1);

    // Second renew: 14.98 + 14.98 > 20.00 — refused before the mutation.
    let client = spend_client(
        FakeTransport::new(vec![envelope(
            "users.getPricing",
            &pricing_com_inner("renew", "14.98"),
        )]),
        false,
        Some(20.0),
        &dir,
    );
    let err = domains::renew(&client, "owned.com", 1, 20.0).expect_err("over cap");
    assert_eq!(err.exit_code(), 3);
    assert!(err.to_string().contains("daily spend cap"), "{err}");
    assert_eq!(client.transport().requests.borrow().len(), 1);

    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(dir.join("spend.jsonl"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "ledger is 0600");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn spend_records_older_than_24h_and_other_profiles_do_not_count() {
    let dir = journal_dir("window");
    std::fs::create_dir_all(&dir).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let stale = now - 25 * 60 * 60;
    std::fs::write(
        dir.join("spend.jsonl"),
        format!(
            "{}\n{}\n",
            serde_json::json!({"ts": stale, "profile": "test", "sandbox": false, "command": "domains.renew", "domain": "old.com", "amount": 100.0}),
            serde_json::json!({"ts": now, "profile": "other", "sandbox": false, "command": "domains.renew", "domain": "other.com", "amount": 100.0}),
        ),
    )
    .unwrap();

    let client = spend_client(
        FakeTransport::new(renew_fixtures()),
        false,
        Some(20.0),
        &dir,
    );
    domains::renew(&client, "owned.com", 1, 20.0)
        .expect("stale and foreign records must not consume the budget");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_spend_ledger_refuses_purchases() {
    let dir = journal_dir("corrupt");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("spend.jsonl"), "not json\n").unwrap();

    let client = spend_client(
        FakeTransport::new(vec![envelope(
            "users.getPricing",
            &pricing_com_inner("renew", "14.98"),
        )]),
        false,
        Some(20.0),
        &dir,
    );
    let err = domains::renew(&client, "owned.com", 1, 20.0).expect_err("corrupt ledger");
    assert_eq!(err.exit_code(), 3);
    assert!(err.to_string().contains("ledger"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn http_405_maps_to_rate_limit_without_retry() {
    // Captured live 2026-06-07: the sandbox rate limiter answers HTTP 405
    // (HTML body), not 429 and not in-band error 500000.
    use ncheap::api::TransportFailure;
    let transport = FakeTransport::with_results(vec![Err(TransportFailure::Status(405))]);
    let client = test_client(transport);

    let err = account::balances(&client).expect_err("rate limited");

    assert_eq!(err.exit_code(), 5);
    assert_eq!(err.kind(), "rate_limit");
    assert_eq!(
        client.transport().requests.borrow().len(),
        1,
        "a throttle response must not be retried into the throttle"
    );
}

// --- backlog round: resolve_id early exit, cross-process throttle ---

#[test]
fn privacy_resolution_stops_at_first_matching_page() {
    fn page(ids: std::ops::Range<u32>, total: usize) -> String {
        let subs: String = ids
            .map(|i| format!(r#"<Whoisguard ID="{i}" DomainName="d{i}.example" Created="05/13/2025" Expires="05/13/2027" Status="ENABLED" />"#))
            .collect();
        envelope(
            "whoisguard.getList",
            &format!(
                r#"<WhoisguardGetListResult>{subs}</WhoisguardGetListResult>
<Paging><TotalItems>{total}</TotalItems><CurrentPage>1</CurrentPage><PageSize>100</PageSize></Paging>"#
            ),
        )
    }
    // 250 subscriptions; the target is on page 1. Only ONE call may happen.
    let enable_inner = r#"<WhoisguardEnableResult DomainName="d7.example" IsSuccess="true" />"#;
    let transport = FakeTransport::new(vec![
        page(0..100, 250),
        envelope("whoisguard.enable", enable_inner),
    ]);
    let client = test_client(transport);

    let result = privacy::enable(&client, "d7.example", "ops@example.org").expect("enable");

    assert_eq!(result.privacy_id, "7");
    assert_eq!(
        client.transport().requests.borrow().len(),
        2,
        "one resolution page + the mutation; not the whole 250-sub listing"
    );
}

#[test]
fn cross_process_throttle_spaces_two_clients_sharing_state() {
    let dir = journal_dir("xproc");
    let mk = |fixtures: Vec<String>| {
        let mut c = Client::new(FakeTransport::new(fixtures), test_profile());
        c.set_timing(
            std::time::Duration::from_millis(250),
            std::time::Duration::ZERO,
        );
        c.set_journal_dir(Some(dir.clone()));
        c
    };
    let balances_inner = r#"<UserGetBalancesResult Currency="USD" AvailableBalance="1.00" AccountBalance="1.00" EarnedAmount="0.00" WithdrawableAmount="0.00" FundsRequiredForAutoRenew="0.00" />"#;

    let a = mk(vec![envelope("users.getBalances", balances_inner)]);
    account::balances(&a).expect("a");

    // A separate Client (≈ separate process): its first call must still be
    // spaced against A's via the shared throttle file.
    let b = mk(vec![envelope("users.getBalances", balances_inner)]);
    let start = std::time::Instant::now();
    account::balances(&b).expect("b");
    assert!(
        start.elapsed() >= std::time::Duration::from_millis(150),
        "second client must wait out the shared spacing, waited {:?}",
        start.elapsed()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
