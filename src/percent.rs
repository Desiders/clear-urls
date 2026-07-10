use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::net::Ipv4Addr;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, PercentEncode, utf8_percent_encode};

const ENCODE_URI_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

pub(crate) fn encode_uri_component(text: &str) -> PercentEncode<'_> {
    utf8_percent_encode(text, ENCODE_URI_COMPONENT)
}

/// Strict percent-decoding: `+` is left untouched
pub(crate) fn decode_uri_component(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = hex_val(*bytes.get(index + 1)?)?;
            let lo = hex_val(*bytes.get(index + 2)?)?;
            out.push(hi * 16 + lo);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_val(byte: u8) -> Option<u8> {
    char::from(byte)
        .to_digit(16)
        .and_then(|digit| u8::try_from(digit).ok())
}

/// Percent-decode until stable, stopping at the last well-formed value,
/// then prepend `http://` unless the result already starts with `http`.
pub(crate) fn decode_url(text: &str) -> String {
    let mut current = text.to_owned();
    while let Some(decoded) = decode_uri_component(&current) {
        if decoded == current {
            break;
        }
        current = decoded;
    }
    if !current.starts_with("http") {
        current = format!("http://{current}");
    }
    current
}

pub(crate) fn extract_host(url: &str) -> Option<&str> {
    let (_, after_scheme) = url.split_once("//")?;
    let end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..end];
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host_port)| host_port);
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed
            .split_once(']')
            .map_or(bracketed, |(host, _)| host)
    } else {
        authority
            .split_once(':')
            .map_or(authority, |(host, _)| host)
    };
    (!host.is_empty()).then_some(host)
}

pub(crate) fn is_local_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if !host.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        return false;
    }
    let Ok(ip) = host.parse::<Ipv4Addr>() else {
        return false;
    };
    // 100.64.0.0/10 spelled out: `Ipv4Addr::is_shared` is not stable yet.
    let [first, second, _, _] = ip.octets();
    ip.is_private()
        || ip.is_link_local()
        || (first == 100 && (64..=127).contains(&second))
        || ip == Ipv4Addr::LOCALHOST
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn encode_uri_component_matches_js() {
        assert_eq!(
            encode_uri_component("AZaz09-_.!~*'() &=?#+/%:@,;$").to_string(),
            "AZaz09-_.!~*'()%20%26%3D%3F%23%2B%2F%25%3A%40%2C%3B%24"
        );
        assert_eq!(encode_uri_component("é☃").to_string(), "%C3%A9%E2%98%83");
    }

    #[test]
    fn decode_uri_component_basics() {
        assert_eq!(
            decode_uri_component("https%3A%2F%2Fexample.com").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(decode_uri_component("a+b").as_deref(), Some("a+b"));
        assert_eq!(
            decode_uri_component("no escapes").as_deref(),
            Some("no escapes")
        );
        assert_eq!(decode_uri_component("%C3%A9").as_deref(), Some("é"));
    }

    #[test]
    fn decode_uri_component_malformed() {
        assert_eq!(decode_uri_component("%"), None);
        assert_eq!(decode_uri_component("%2"), None);
        assert_eq!(decode_uri_component("%G1"), None);
        assert_eq!(decode_uri_component("abc%ZZdef"), None);
        assert_eq!(decode_uri_component("%C3%28"), None);
        assert_eq!(decode_uri_component("%FF"), None);
    }

    #[test]
    fn decode_url_decodes_until_stable() {
        assert_eq!(
            decode_url("https%3A%2F%2Fexample.com%2Fpath"),
            "https://example.com/path"
        );
        assert_eq!(
            decode_url("https%253A%252F%252Fexample.com"),
            "https://example.com"
        );
        assert_eq!(decode_url("https://example.com"), "https://example.com");
    }

    #[test]
    fn decode_url_prepends_http() {
        assert_eq!(decode_url("example.com/x"), "http://example.com/x");
        assert_eq!(decode_url("https://example.com"), "https://example.com");
        assert_eq!(
            decode_url("HTTPS://example.com"),
            "http://HTTPS://example.com"
        );
    }

    #[test]
    fn decode_url_stops_on_malformed() {
        assert_eq!(
            decode_url("https://example.com/%2"),
            "https://example.com/%2"
        );
        assert_eq!(decode_url("example.com/%zz"), "http://example.com/%zz");
    }

    #[test]
    fn extract_host_variants() {
        assert_eq!(
            extract_host("https://example.com/a?b#c"),
            Some("example.com")
        );
        assert_eq!(extract_host("https://Example.COM"), Some("Example.COM"));
        assert_eq!(
            extract_host("http://example.com:8080/x"),
            Some("example.com")
        );
        assert_eq!(
            extract_host("http://user:pass@example.com/"),
            Some("example.com")
        );
        assert_eq!(extract_host("http://[::1]:8080/x"), Some("::1"));
        assert_eq!(extract_host("ftp://example.com"), Some("example.com"));
        assert_eq!(extract_host("//example.com/x"), Some("example.com"));
        assert_eq!(extract_host("mailto:foo@example.com"), None);
        assert_eq!(extract_host("not a url"), None);
        assert_eq!(extract_host("https://"), None);
    }

    #[test]
    fn local_hosts() {
        assert!(is_local_host("localhost"));
        assert!(is_local_host("LOCALHOST"));
        assert!(is_local_host("127.0.0.1"));
        assert!(!is_local_host("127.0.0.2"));
        assert!(is_local_host("10.1.2.3"));
        assert!(is_local_host("172.16.0.1"));
        assert!(is_local_host("172.31.255.255"));
        assert!(!is_local_host("172.15.0.1"));
        assert!(!is_local_host("172.32.0.1"));
        assert!(is_local_host("192.168.1.1"));
        assert!(!is_local_host("192.167.1.1"));
        assert!(is_local_host("100.64.0.1"));
        assert!(is_local_host("100.127.255.255"));
        assert!(!is_local_host("100.128.0.1"));
        assert!(is_local_host("169.254.10.10"));
        assert!(!is_local_host("8.8.8.8"));
        assert!(!is_local_host("1e100.net"));
        assert!(!is_local_host("example.com"));
    }
}
