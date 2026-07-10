#![cfg(feature = "fetch")]

use clear_urls::UrlCleaner;

/// Network test; run explicitly with `cargo test --features rustls -- --ignored`
/// (or `--features native-tls`).
#[test]
#[ignore = "requires network access"]
fn fetch_verify_and_compile_official_rules() {
    let json = clear_urls::fetch_rules().unwrap();
    let cleaner = UrlCleaner::from_rules_json(&json).unwrap();
    let result = cleaner.clean("https://example.com/?utm_source=x&id=1");
    assert_eq!(result.url, "https://example.com/?id=1");
}
