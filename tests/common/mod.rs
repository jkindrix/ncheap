use std::cell::RefCell;

use ncheap::api::{Transport, TransportFailure};
use ncheap::config::{Profile, Secret};

pub struct FakeTransport {
    pub responses: RefCell<Vec<String>>,
    pub requests: RefCell<Vec<Vec<(String, String)>>>,
}

impl FakeTransport {
    pub fn new(responses: Vec<String>) -> Self {
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

pub fn test_profile() -> Profile {
    Profile {
        name: "test".into(),
        api_user: "testuser".into(),
        api_key: Secret::new("testkey".into()),
        username: "testuser".into(),
        client_ip: "192.0.2.1".into(),
        sandbox: true,
    }
}

pub fn param<'a>(request: &'a [(String, String)], key: &str) -> Option<&'a str> {
    request
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}
