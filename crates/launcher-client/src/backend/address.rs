//! Turns what a player types into the server field into the origin the
//! launcher talks to.

use std::net::IpAddr;
use std::time::Duration;

use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::RData;

use reqwest::Url;

use crate::lang::{Msg, t};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddressError {
    #[error("{}", t(Msg::AddressEmpty))]
    Empty,
    #[error("{}", t(Msg::AddressInvalid))]
    Invalid,
    #[error("{}", t(Msg::AddressInsecure))]
    Insecure,
}

/// Whether plain HTTP is acceptable for this host: only where the traffic
/// never leaves the machine or the local network.
fn is_local(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") || host.to_ascii_lowercase().ends_with(".localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unique_local(),
        Err(_) => false,
    }
}

/// The origin (`scheme://host[:port]`, no trailing slash) for an address.
///
/// A bare host means https, except on the local machine or network where a
/// panel usually runs without a certificate. Plain http to anywhere else is
/// refused: the server hands the launcher code to run.
pub fn origin(input: &str) -> Result<String, AddressError> {
    let input = input.trim().trim_end_matches('/');
    if input.is_empty() {
        return Err(AddressError::Empty);
    }
    let explicit = input.contains("://");
    let url = if explicit {
        Url::parse(input)
    } else {
        Url::parse(&format!("https://{input}"))
    }
    .map_err(|_| AddressError::Invalid)?;

    let host = url.host_str().ok_or(AddressError::Invalid)?.to_owned();
    if host.is_empty() || !url.username().is_empty() || url.password().is_some() {
        return Err(AddressError::Invalid);
    }
    let local = is_local(&host);
    let scheme = match (explicit, url.scheme()) {
        (true, "https") => "https",
        (true, "http") if local => "http",
        (true, "http") => return Err(AddressError::Insecure),
        (true, _) => return Err(AddressError::Invalid),
        (false, _) if local => "http",
        (false, _) => "https",
    };
    Ok(match url.port() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    })
}

/// The domain to ask DNS about: only a bare name, since a port, a scheme
/// or an IP address already says exactly where the panel is.
fn srv_candidate(input: &str) -> Option<String> {
    let input = input.trim().trim_end_matches('/');
    if input.contains("://") || input.contains(['/', ':', '[']) {
        return None;
    }
    let url = Url::parse(&format!("https://{input}")).ok()?;
    let host = url.host_str()?;
    let plain = !is_local(host) && host.parse::<IpAddr>().is_err() && host.contains('.');
    plain.then(|| host.to_owned())
}

/// The origin an SRV record points at. The panel is always reached over
/// https this way.
fn srv_origin(target: &str, port: u16) -> Option<String> {
    let target = target.trim_end_matches('.');
    if target.is_empty() || port == 0 {
        return None;
    }
    origin(&format!("https://{target}:{port}"))
        .ok()
        .map(|found| match found.strip_suffix(":443") {
            Some(bare) => bare.to_owned(),
            None => found,
        })
}

async fn srv_lookup(host: &str) -> Option<String> {
    let resolver = TokioResolver::builder_tokio().ok()?.build().ok()?;
    let lookup = tokio::time::timeout(
        Duration::from_secs(3),
        resolver.srv_lookup(format!("_mcadmin._tcp.{host}.")),
    )
    .await
    .ok()?
    .ok()?;
    lookup
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::SRV(srv) => Some(srv),
            _ => None,
        })
        .min_by_key(|srv| (srv.priority, std::cmp::Reverse(srv.weight)))
        .and_then(|srv| srv_origin(&srv.target.to_utf8(), srv.port))
}

/// Like [`origin`], but lets the domain's `_mcadmin._tcp` SRV record say
/// where the panel is, so players can type the same name they use in the
/// game. Without such a record the address is taken as typed.
pub async fn discover(input: &str) -> Result<String, AddressError> {
    let typed = origin(input)?;
    if let Some(host) = srv_candidate(input)
        && let Some(found) = srv_lookup(&host).await
    {
        return Ok(found);
    }
    Ok(typed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_hosts_get_https() {
        assert_eq!(
            origin("play.example.com").unwrap(),
            "https://play.example.com"
        );
        assert_eq!(
            origin("  play.example.com:8443/ ").unwrap(),
            "https://play.example.com:8443"
        );
    }

    #[test]
    fn local_hosts_default_to_http() {
        assert_eq!(origin("localhost:8097").unwrap(), "http://localhost:8097");
        assert_eq!(origin("127.0.0.1:8097").unwrap(), "http://127.0.0.1:8097");
        assert_eq!(origin("192.168.1.20").unwrap(), "http://192.168.1.20");
        assert_eq!(origin("[::1]:8097").unwrap(), "http://[::1]:8097");
    }

    #[test]
    fn explicit_scheme_is_kept_when_safe() {
        assert_eq!(
            origin("https://127.0.0.1:8443").unwrap(),
            "https://127.0.0.1:8443"
        );
        assert_eq!(
            origin("http://10.0.0.5:8097/").unwrap(),
            "http://10.0.0.5:8097"
        );
        assert_eq!(
            origin("https://mc.example/panel").unwrap(),
            "https://mc.example"
        );
    }

    #[test]
    fn plain_http_to_the_internet_is_refused() {
        assert_eq!(
            origin("http://play.example.com"),
            Err(AddressError::Insecure)
        );
    }

    #[test]
    fn junk_is_rejected() {
        assert_eq!(origin("   "), Err(AddressError::Empty));
        assert_eq!(origin("ftp://example.com"), Err(AddressError::Invalid));
        assert_eq!(origin("user:pw@example.com"), Err(AddressError::Invalid));
        assert_eq!(origin("exa mple"), Err(AddressError::Invalid));
    }
    #[test]
    fn only_bare_domains_are_looked_up() {
        assert_eq!(
            srv_candidate("play.example.com").as_deref(),
            Some("play.example.com")
        );
        assert_eq!(
            srv_candidate(" Play.Example.com/ ").as_deref(),
            Some("play.example.com")
        );
        for input in [
            "play.example.com:8443",
            "https://play.example.com",
            "127.0.0.1",
            "192.168.1.20",
            "localhost",
            "panel.localhost",
            "nas",
            "[::1]",
        ] {
            assert_eq!(srv_candidate(input), None, "{input}");
        }
    }

    #[test]
    fn srv_records_become_https_origins() {
        assert_eq!(
            srv_origin("panel.example.com.", 8443).as_deref(),
            Some("https://panel.example.com:8443")
        );
        assert_eq!(
            srv_origin("panel.example.com.", 443).as_deref(),
            Some("https://panel.example.com")
        );
        assert_eq!(srv_origin(".", 443), None);
        assert_eq!(srv_origin("panel.example.com.", 0), None);
    }
}
