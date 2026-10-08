//! Server network policy for provider endpoints.
//!
//! Custom endpoint configuration (including endpoints coming from generated
//! app input) must not turn the platform into unrestricted internal-network
//! access: only https URLs are accepted in production, and plain http only for
//! loopback development targets. Private/linked-local literal addresses are
//! refused; hostnames resolve at request time and must be governed by the
//! deployment egress policy as documented in docs/deployment.md.

use crate::types::ModelError;

/// Host part of an absolute URL.
pub fn host_of(base_url: &str) -> Option<&str> {
    let (_, rest) = base_url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    Some(host.split(':').next().unwrap_or(""))
}

pub fn is_loopback(base_url: &str) -> bool {
    matches!(host_of(base_url), Some("localhost" | "127.0.0.1" | "::1"))
}

/// HTTP client for a provider endpoint. Loopback endpoints skip the ambient
/// proxy configuration so local development and contract tests connect
/// directly; remote endpoints honor deployment proxy settings.
pub fn build_client(base_url: &str) -> reqwest::Client {
    let mut builder = reqwest::Client::builder();
    if is_loopback(base_url) {
        builder = builder.no_proxy();
    }
    builder.build().unwrap_or_default()
}

pub fn validate_endpoint(base_url: &str) -> Result<(), ModelError> {
    let (scheme, rest) = base_url
        .split_once("://")
        .ok_or_else(|| ModelError::Config("endpoint must be an absolute URL".into()))?;
    let host = host_of(base_url).unwrap_or("");
    if host.is_empty() {
        return Err(ModelError::Config("endpoint must have a host".into()));
    }
    let is_loopback = host == "localhost" || host == "127.0.0.1" || host == "::1";
    let _ = rest;
    match scheme {
        "https" => {}
        "http" if is_loopback => {}
        _ => {
            return Err(ModelError::Config(
                "endpoint must be https (http allowed for loopback only)".into(),
            ))
        }
    }
    // Refuse literal private and link-local addresses.
    if !is_loopback {
        if let Ok(addr) = host.parse::<std::net::IpAddr>() {
            let blocked = match addr {
                std::net::IpAddr::V4(v4) => {
                    v4.is_private() || v4.is_link_local() || v4.is_loopback() || v4.is_broadcast()
                }
                std::net::IpAddr::V6(v6) => {
                    v6.is_loopback() || (v6.segments()[0] & 0xffc0) == 0xfe80
                }
            };
            if blocked {
                return Err(ModelError::Config(
                    "endpoint resolves to a private or link-local address".into(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_allowed() {
        assert!(validate_endpoint("https://api.example.com/v1").is_ok());
    }

    #[test]
    fn loopback_http_allowed_for_development() {
        assert!(validate_endpoint("http://127.0.0.1:8080/v1").is_ok());
        assert!(validate_endpoint("http://localhost:9000").is_ok());
    }

    #[test]
    fn plain_http_rejected_elsewhere() {
        assert!(validate_endpoint("http://api.example.com").is_err());
    }

    #[test]
    fn private_literal_addresses_rejected() {
        assert!(validate_endpoint("https://10.0.0.5").is_err());
        assert!(validate_endpoint("https://192.168.1.10").is_err());
        assert!(validate_endpoint("https://169.254.169.254").is_err());
    }

    #[test]
    fn loopback_detection() {
        assert!(is_loopback("http://127.0.0.1:8080/v1"));
        assert!(is_loopback("http://localhost:9000"));
        assert!(!is_loopback("https://api.example.com"));
    }

    #[test]
    fn malformed_rejected() {
        assert!(validate_endpoint("not a url").is_err());
        assert!(validate_endpoint("https://").is_err());
    }
}
