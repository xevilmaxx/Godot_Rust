//! Port of `modules/upnp/upnp.h` — gateway discovery stub.
//! Real SSDP multicast needs OS sockets + LAN; headless CI/agents get the same
//! API reporting "no gateway", and games override via `set_mock_gateway`.

#[derive(Debug, Clone, Default)]
pub struct Upnp {
    mock_gateway: Option<String>,
    pub discover_called: bool,
}

impl Upnp {
    pub fn new() -> Self {
        Self::default()
    }
    /// Test seam: pretend a gateway exists.
    pub fn set_mock_gateway(&mut self, url: Option<String>) {
        self.mock_gateway = url;
    }
    /// SSDP discover (stub: 0 devices unless mocked).
    pub fn discover(&mut self, _timeout_ms: u64) -> usize {
        self.discover_called = true;
        if self.mock_gateway.is_some() {
            1
        } else {
            0
        }
    }
    pub fn gateway(&self) -> Option<&str> {
        self.mock_gateway.as_deref()
    }
    pub fn add_port_mapping(&self, _port: u16, _desc: &str) -> bool {
        self.mock_gateway.is_some()
    }
    pub fn delete_port_mapping(&self, _port: u16) -> bool {
        self.mock_gateway.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stub_reports_no_gateway_until_mocked() {
        let mut u = Upnp::new();
        assert_eq!(u.discover(500), 0);
        assert!(u.discover_called);
        assert!(!u.add_port_mapping(7777, "game"));
        u.set_mock_gateway(Some("http://192.168.1.1/upnp".into()));
        assert_eq!(u.discover(500), 1);
        assert!(u.add_port_mapping(7777, "game"));
        assert_eq!(u.gateway(), Some("http://192.168.1.1/upnp"));
    }
}
