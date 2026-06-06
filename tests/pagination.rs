use std::cell::RefCell;

use ncheap::api::{Client, Transport, TransportFailure};
use ncheap::commands::domains;
use ncheap::config::{Profile, Secret};

struct FakeTransport {
    responses: RefCell<Vec<String>>,
    requests: RefCell<Vec<Vec<(String, String)>>>,
}

impl FakeTransport {
    fn new(responses: Vec<String>) -> Self {
        Self {
            responses: RefCell::new(responses),
            requests: RefCell::new(Vec::new()),
        }
    }
}

impl Transport for FakeTransport {
    fn send(
        &self,
        _endpoint: &str,
        params: &[(String, String)],
    ) -> Result<String, TransportFailure> {
        self.requests.borrow_mut().push(params.to_vec());
        let mut responses = self.responses.borrow_mut();
        if responses.is_empty() {
            return Err(TransportFailure::Other("no more fixture responses".into()));
        }
        Ok(responses.remove(0))
    }
}

fn test_profile() -> Profile {
    Profile {
        name: "test".into(),
        api_user: "testuser".into(),
        api_key: Secret::new("testkey".into()),
        username: "testuser".into(),
        client_ip: "192.0.2.1".into(),
        sandbox: true,
    }
}

fn page_xml(names: &[String], total: usize, page: usize) -> String {
    let domains: String = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            format!(
                r#"<Domain ID="{}" Name="{n}" User="testuser" Created="02/15/2020" Expires="02/15/2027" IsExpired="false" IsLocked="False" AutoRenew="true" WhoisGuard="ENABLED" IsPremium="false" IsOurDNS="false"/>"#,
                1000 + i
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ApiResponse xmlns="http://api.namecheap.com/xml.response" Status="OK">
  <Errors />
  <RequestedCommand>namecheap.domains.getList</RequestedCommand>
  <CommandResponse Type="namecheap.domains.getList">
    <DomainGetListResult>{domains}</DomainGetListResult>
    <Paging>
      <TotalItems>{total}</TotalItems>
      <CurrentPage>{page}</CurrentPage>
      <PageSize>20</PageSize>
    </Paging>
  </CommandResponse>
  <Server>TEST</Server>
  <GMTTimeDifference>+0</GMTTimeDifference>
  <ExecutionTime>0.01</ExecutionTime>
</ApiResponse>"#
    )
}

fn error_xml(number: &str, message: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ApiResponse xmlns="http://api.namecheap.com/xml.response" Status="ERROR">
  <Errors>
    <Error Number="{number}">{message}</Error>
  </Errors>
  <RequestedCommand>namecheap.domains.getList</RequestedCommand>
  <Server>TEST</Server>
</ApiResponse>"#
    )
}

fn param<'a>(request: &'a [(String, String)], key: &str) -> Option<&'a str> {
    request
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// The core fixture: 24 domains across two pages, exercising the >20-item
/// boundary where the API's default PageSize silently drops domains.
#[test]
fn paginates_beyond_default_page_size() {
    let names: Vec<String> = (0..24).map(|i| format!("domain{i:02}.example")).collect();
    let transport = FakeTransport::new(vec![
        page_xml(&names[..20], 24, 1),
        page_xml(&names[20..], 24, 2),
    ]);
    let client = Client::new(transport, test_profile());

    let domains = domains::list(&client).expect("list should succeed");

    assert_eq!(domains.len(), 24, "all 24 domains must survive pagination");
    assert_eq!(domains[0].name, "domain00.example");
    assert_eq!(domains[23].name, "domain23.example");
    assert!(!domains[5].is_locked, "IsLocked=\"False\" must parse");
    assert_eq!(client.calls(), 2);
}

#[test]
fn requests_carry_auth_pagination_and_prefixed_command() {
    let names: Vec<String> = (0..24).map(|i| format!("domain{i:02}.example")).collect();
    let transport = FakeTransport::new(vec![
        page_xml(&names[..20], 24, 1),
        page_xml(&names[20..], 24, 2),
    ]);
    let client = Client::new(transport, test_profile());
    domains::list(&client).expect("list should succeed");

    let requests = client.transport().requests.borrow();
    assert_eq!(requests.len(), 2);
    let first = &requests[0];
    assert_eq!(param(first, "Command"), Some("namecheap.domains.getList"));
    assert_eq!(param(first, "ApiUser"), Some("testuser"));
    assert_eq!(param(first, "ApiKey"), Some("testkey"));
    assert_eq!(param(first, "UserName"), Some("testuser"));
    assert_eq!(param(first, "ClientIp"), Some("192.0.2.1"));
    assert_eq!(param(first, "Page"), Some("1"));
    assert_eq!(param(first, "PageSize"), Some("100"));
    assert_eq!(param(&requests[1], "Page"), Some("2"));
}

#[test]
fn ip_rejection_error_is_explained() {
    let transport =
        FakeTransport::new(vec![error_xml("1011150", "Parameter RequestIP is invalid")]);
    let client = Client::new(transport, test_profile());

    let err = domains::list(&client).expect_err("should fail");
    assert_eq!(err.exit_code(), 1);
    assert_eq!(err.code(), Some("1011150"));
    let msg = err.to_string();
    assert!(
        msg.contains("whitelist"),
        "must explain the IP rejection: {msg}"
    );
}

#[test]
fn stalled_pagination_is_an_error_not_a_hang() {
    let names: Vec<String> = (0..20).map(|i| format!("domain{i:02}.example")).collect();
    let transport = FakeTransport::new(vec![
        page_xml(&names, 24, 1),
        page_xml(&[], 24, 2), // server claims 24 but returns an empty page
    ]);
    let client = Client::new(transport, test_profile());

    let err = domains::list(&client).expect_err("should fail");
    assert_eq!(err.exit_code(), 1);
    assert!(err.to_string().contains("pagination stalled"));
}
