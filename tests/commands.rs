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
