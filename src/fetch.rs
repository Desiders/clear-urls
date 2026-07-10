//! Rules download with SHA-256 verification

#[cfg(not(any(feature = "rustls", feature = "native-tls")))]
compile_error!(
    "the \"fetch\" feature requires a TLS backend: enable the \"rustls\" or \"native-tls\" feature"
);

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::string::String;
use core::fmt::Write;
use sha2::{Digest, Sha256};
use ureq::tls::{TlsConfig, TlsProvider};

pub const DEFAULT_RULES_URL: &str = "https://rules2.clearurls.xyz/data.minify.json";
pub const DEFAULT_HASH_URL: &str = "https://rules2.clearurls.xyz/rules.minify.hash";

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FetchError {
    #[error("request failed: {0}")]
    Http(#[from] Box<ureq::Error>),
    #[error("empty response from {url}")]
    EmptyResponse { url: String },
    #[error("rules hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
}

/// Download the official `ClearURLs` rules, verified against the published
/// SHA-256 hash. Returns the JSON document for
/// [`crate::UrlCleaner::from_rules_json`], worth caching on disk.
///
/// ```no_run
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let json = clear_urls::fetch_rules()?;
/// let cleaner = clear_urls::UrlCleaner::from_rules_json(&json)?;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// [`FetchError::Http`] on transport failures or non-success status,
/// [`FetchError::EmptyResponse`] on an empty body,
/// [`FetchError::HashMismatch`] when verification fails.
pub fn fetch_rules() -> Result<String, FetchError> {
    fetch_rules_from(DEFAULT_RULES_URL, DEFAULT_HASH_URL)
}

/// Like [`fetch_rules`], but from custom URLs. Both bodies are trimmed and
/// the hash is computed over the trimmed rules text, like the addon does.
///
/// # Errors
///
/// Same as [`fetch_rules`].
pub fn fetch_rules_from(rules_url: &str, hash_url: &str) -> Result<String, FetchError> {
    let expected = get_text(hash_url)?;
    let expected = expected.trim();
    if expected.is_empty() {
        return Err(FetchError::EmptyResponse {
            url: hash_url.to_owned(),
        });
    }

    let body = get_text(rules_url)?;
    let body = body.trim();
    if body.is_empty() {
        return Err(FetchError::EmptyResponse {
            url: rules_url.to_owned(),
        });
    }

    verify_hash(body, expected)?;
    Ok(body.to_owned())
}

fn verify_hash(body: &str, expected: &str) -> Result<(), FetchError> {
    let digest = Sha256::digest(body.as_bytes());
    let mut actual = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(actual, "{byte:02x}").expect("writing to a String cannot fail");
    }
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(FetchError::HashMismatch {
            expected: expected.to_owned(),
            actual,
        })
    }
}

fn get_text(url: &str) -> Result<String, FetchError> {
    let tls_provider = if cfg!(feature = "rustls") {
        TlsProvider::Rustls
    } else {
        TlsProvider::NativeTls
    };
    let agent = ureq::Agent::config_builder()
        .tls_config(TlsConfig::builder().provider(tls_provider).build())
        .build()
        .new_agent();
    let mut response = agent.get(url).call().map_err(Box::new)?;
    Ok(response.body_mut().read_to_string().map_err(Box::new)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn verify_hash_accepts_correct_digest() {
        assert!(verify_hash("abc", ABC_SHA256).is_ok());
        assert!(verify_hash("abc", &ABC_SHA256.to_uppercase()).is_ok());
    }

    #[test]
    fn verify_hash_rejects_wrong_digest() {
        match verify_hash("abcd", ABC_SHA256) {
            Err(FetchError::HashMismatch { expected, actual }) => {
                assert_eq!(expected, ABC_SHA256);
                assert_ne!(actual, ABC_SHA256);
            }
            other => panic!("expected HashMismatch, got {other:?}"),
        }
    }
}
