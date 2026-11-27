//! How every HTTP client this CLI builds reaches the network.
//!
//! Like a browser on the same machine, a client trusts the operating system's
//! certificate store as well as the bundled Mozilla roots, and takes its proxy
//! from `HTTPS_PROXY` / `HTTP_PROXY` / `NO_PROXY` or, when those are unset,
//! from the system settings (macOS network settings, the Windows Internet
//! Options). A network that inspects HTTPS with a root of its own, or that only
//! lets traffic out through the system proxy, would otherwise let the browser
//! through and stop the CLI.

use reqwest::Url;
use reqwest::blocking::ClientBuilder;

/// A client builder for requests to `target`, a URL or bare origin.
///
/// Requests to this machine itself go direct: the macOS system settings carry
/// no exceptions a client can read, and a proxy elsewhere cannot reach this
/// machine's loopback.
pub fn client_builder(target: &str) -> ClientBuilder {
    let builder = reqwest::blocking::Client::builder();
    if is_loopback(target) {
        builder.no_proxy()
    } else {
        builder
    }
}

fn is_loopback(target: &str) -> bool {
    let Ok(url) = Url::parse(target.trim()) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost" || host.ends_with(".localhost")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_this_machine_counts_as_loopback() {
        for target in [
            "http://127.0.0.1:8080/openapi/memorylake",
            "http://127.9.9.9",
            "http://localhost:3000",
            "http://api.localhost",
            "http://[::1]:8080",
        ] {
            assert!(is_loopback(target), "{target}");
        }
        for target in [
            "https://app.memorylake.cn/openapi/memorylake",
            "https://localhost.example.com",
            "http://10.0.0.1",
            "not a url",
            "mailto:someone@example.com",
        ] {
            assert!(!is_loopback(target), "{target}");
        }
    }

    #[test]
    fn a_client_builds_for_either_kind_of_target() {
        for target in [
            "http://127.0.0.1:8080",
            "https://app.memorylake.cn/openapi/memorylake",
        ] {
            client_builder(target).build().expect(target);
        }
    }
}
