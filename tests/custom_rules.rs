//! End-to-end tests with hand-written rules; runs with --no-default-features.

use clear_urls::{Error, Settings, UrlCleaner};
use serde_json::json;

fn cleaner() -> UrlCleaner {
    UrlCleaner::from_rules_json(
        &json!({
            "providers": {
                "shop": {
                    "urlPattern": "^https?://(?:www\\.)?shop\\.example",
                    "rules": ["sid", "track_[a-z]+"],
                    "referralMarketing": ["partner"],
                    "rawRules": ["\\/promo\\/[0-9]+"],
                    "exceptions": ["^https?://(?:www\\.)?shop\\.example/checkout"],
                    "redirections": ["^https?://(?:www\\.)?shop\\.example/out\\?to=([^&]+)"]
                },
                "ads": {
                    "urlPattern": "^https?://ads\\.example",
                    "completeProvider": true
                },
                "global": {
                    "urlPattern": ".*",
                    "rules": ["utm_[a-z]+"]
                }
            }
        })
        .to_string(),
    )
    .unwrap()
}

#[test]
fn rules_and_raw_rules_apply() {
    let result = cleaner().clean("https://shop.example/item/promo/42?sid=abc&track_id=7&color=red");
    assert_eq!(result.url, "https://shop.example/item?color=red");
}

#[test]
fn exceptions_win_over_provider() {
    let url = "https://shop.example/checkout?sid=abc&step=2";
    // The shop provider is skipped, but the global provider still applies.
    assert_eq!(cleaner().clean(url).url, url);
    let result = cleaner().clean("https://shop.example/checkout?utm_source=mail&step=2");
    assert_eq!(result.url, "https://shop.example/checkout?step=2");
}

#[test]
fn redirection_then_global_cleanup() {
    let result = cleaner().clean(
        "https://shop.example/out?to=https%3A%2F%2Fdest.example%2F%3Futm_campaign%3Dx%26id%3D9",
    );
    assert_eq!(result.url, "https://dest.example/?id=9");
    assert!(result.redirected);
}

#[test]
fn blocked_domain_flagged() {
    let result = cleaner().clean("https://ads.example/pixel?x=1");
    assert!(result.blocked);
    assert_eq!(result.url, "https://ads.example/pixel?x=1");
}

#[test]
fn settings_via_with_settings() {
    let cleaner = cleaner().with_settings(Settings {
        domain_blocking: false,
        strip_referral_marketing: true,
        skip_local_hosts: false,
    });
    assert!(!cleaner.clean("https://ads.example/pixel?x=1").blocked);
    assert_eq!(
        cleaner
            .clean("https://shop.example/item?partner=p1&color=red")
            .url,
        "https://shop.example/item?color=red"
    );
    assert_eq!(
        cleaner.clean("http://localhost/?utm_source=x").url,
        "http://localhost/"
    );
}

#[test]
fn from_rules_json_equivalent() {
    let cleaner =
        UrlCleaner::from_rules_json(r#"{"providers": {"g": {"rules": ["utm_[a-z]+"]}}}"#).unwrap();
    assert_eq!(
        cleaner.clean("https://example.com/?utm_source=x&id=1").url,
        "https://example.com/?id=1"
    );
}

#[test]
fn invalid_rules_are_rejected() {
    assert!(matches!(
        UrlCleaner::from_rules_json("not json"),
        Err(Error::InvalidJson)
    ));
    assert!(matches!(
        UrlCleaner::from_rules_json(r#"{"providers": {"bad": {"urlPattern": "["}}}"#),
        Err(Error::InvalidRegex { .. })
    ));
}

#[test]
fn empty_rules_clean_nothing() {
    let cleaner = UrlCleaner::from_rules_json(r#"{"providers": {}}"#).unwrap();
    let url = "https://example.com/?utm_source=x";
    let result = cleaner.clean(url);
    assert_eq!(result.url, url);
    assert!(!result.changed);
}
