//! Integration tests against the real embedded ClearURLs rules.

#![cfg(feature = "embedded-rules")]

use clear_urls::UrlCleaner;

fn cleaner() -> UrlCleaner {
    UrlCleaner::from_embedded_rules().unwrap()
}

#[test]
fn embedded_rules_parse_and_compile() {
    cleaner();
}

#[test]
fn cleaner_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<UrlCleaner>();
}

#[test]
fn shared_across_threads() {
    let cleaner = cleaner();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                let result = cleaner.clean("https://example.com/?utm_source=x&id=1");
                assert_eq!(result.url, "https://example.com/?id=1");
            });
        }
    });
}

#[test]
fn strips_global_tracking_params() {
    let result = cleaner().clean(
        "https://example.com/article?utm_source=nl&utm_medium=mail&fbclid=abc&gclid=123&id=1",
    );
    assert_eq!(result.url, "https://example.com/article?id=1");
    assert!(result.changed);
    assert!(!result.redirected);
    assert!(!result.blocked);
}

#[test]
fn watchdog_vector() {
    let result = cleaner().clean("https://clearurls.roebert.eu?utm_source=addon");
    assert_eq!(result.url, "https://clearurls.roebert.eu");
}

#[test]
fn clean_url_stays_untouched() {
    for url in [
        "https://example.com/",
        "https://example.com/path?id=1&page=2",
        "https://en.wikipedia.org/wiki/Rust_(programming_language)#History",
    ] {
        let result = cleaner().clean(url);
        assert_eq!(result.url, url);
        assert!(!result.changed);
    }
}

#[test]
fn fragment_tracking_removed_anchor_kept() {
    let cleaner = cleaner();
    assert_eq!(
        cleaner
            .clean("https://example.com/page#utm_source=share")
            .url,
        "https://example.com/page"
    );
    assert_eq!(
        cleaner
            .clean("https://example.com/page?utm_source=share#section")
            .url,
        "https://example.com/page#section"
    );
}

#[test]
fn google_search_params_stripped() {
    let result = cleaner().clean(
        "https://www.google.com/search?q=rust+language&ved=2ahUKE&ei=abc&oq=rust&sourceid=chrome",
    );
    assert_eq!(
        result.url,
        "https://www.google.com/search?q=rust%20language"
    );
}

#[test]
fn google_redirect_unwrapped() {
    let result = cleaner()
        .clean("https://www.google.com/url?sa=t&url=https%3A%2F%2Fexample.com%2Fpage&usg=AOvVaw");
    assert_eq!(result.url, "https://example.com/page");
    assert!(result.redirected);
}

#[test]
fn nested_redirect_target_cleaned_by_fixpoint() {
    let result = cleaner().clean(
        "https://www.google.com/url?q=https%3A%2F%2Fexample.com%2Fitem%3Futm_campaign%3Dspring%26id%3D7",
    );
    assert_eq!(result.url, "https://example.com/item?id=7");
    assert!(result.redirected);
}

#[test]
fn google_docs_exception_respected() {
    let url = "https://docs.google.com/document/d/abc/edit?ved=123";
    let result = cleaner().clean(url);
    assert_eq!(result.url, url);
}

#[test]
fn amazon_product_url_cleaned() {
    let result = cleaner().clean(
        "https://www.amazon.com/gp/product/B01M0DUELS/ref=s9_acsd_al_bw_c_x_2_w?pf_rd_r=WBAG&qid=15221&sr=8-1&th=1&psc=1",
    );
    // "/ref=…" is removed by a rawRule; pf_rd_r, qid, sr, th by rules;
    // "psc" is not in the current ruleset and must survive.
    assert_eq!(
        result.url,
        "https://www.amazon.com/gp/product/B01M0DUELS?psc=1"
    );
}

#[test]
fn amazon_referral_tag_kept_by_default() {
    let url = "https://www.amazon.com/dp/B01M0DUELS?tag=affiliate-21";
    assert_eq!(cleaner().clean(url).url, url);
}

#[test]
fn amazon_referral_tag_stripped_on_demand() {
    let cleaner = cleaner().with_strip_referral_marketing(true);
    let result = cleaner.clean("https://www.amazon.com/dp/B01M0DUELS?tag=affiliate-21");
    assert_eq!(result.url, "https://www.amazon.com/dp/B01M0DUELS");
}

#[test]
fn github_urls_excepted_from_global_rules() {
    // globalRules has a github.com exception; the github provider itself
    // only strips email_token/email_source.
    let url = "https://github.com/rust-lang/rust?utm_source=share&tab=readme";
    assert_eq!(cleaner().clean(url).url, url);
    let result = cleaner().clean("https://github.com/notifications?email_token=abc&query=repo");
    assert_eq!(result.url, "https://github.com/notifications?query=repo");
}

#[test]
fn complete_provider_domain_blocked() {
    let url = "https://kevinroebert.gitlab.io/ClearUrls/void/block.svg?type=test";
    let result = cleaner().clean(url);
    assert!(result.blocked);
    assert_eq!(result.url, url);

    let result = cleaner().with_domain_blocking(false).clean(url);
    assert!(!result.blocked);
    // Without blocking, the implicit `.*` rule strips all params instead.
    assert_eq!(
        result.url,
        "https://kevinroebert.gitlab.io/ClearUrls/void/block.svg"
    );
}

#[test]
fn clearurls_test_redirection() {
    let result = cleaner().clean(
        "https://kevinroebert.gitlab.io/ClearUrls/void/index.html?url=https%3A%2F%2Fexample.com%2F",
    );
    assert_eq!(result.url, "https://example.com/");
    assert!(result.redirected);
}

#[test]
fn local_urls_untouched() {
    let cleaner = cleaner();
    for url in [
        "http://localhost:8080/app?utm_source=x",
        "http://127.0.0.1/page?fbclid=1",
        "http://192.168.0.10/?gclid=2",
    ] {
        let result = cleaner.clean(url);
        assert_eq!(result.url, url);
    }
    let result = cleaner
        .with_skip_local_hosts(false)
        .clean("http://localhost:8080/app?utm_source=x");
    assert_eq!(result.url, "http://localhost:8080/app");
}

#[test]
fn cleaning_is_idempotent() {
    let cleaner = cleaner();
    for url in [
        "https://example.com/",
        "https://example.com/?utm_source=a&utm_medium=b&id=1",
        "https://example.com/page#utm_source=share",
        "https://example.com/page?q=a+b%20c",
        "https://www.google.com/url?q=https%3A%2F%2Fexample.com%2F",
        "https://www.google.com/search?q=rust&ved=1&ei=2",
        "https://www.amazon.com/dp/B01M0DUELS/ref=nav?qid=1&th=1",
        "https://github.com/rust-lang/rust?utm_source=share",
        "https://twitter.com/user/status/1?ref_src=twsrc&s=20",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ&feature=share",
        "https://example.com/?fbclid=x&gclid=y&msclkid=z",
        "https://example.com/search?q=%C3%A9t%C3%A9",
        "https://example.com/?a=1&a=2&a=3",
        "https://example.com/?bare&empty=",
        "https://example.com/#/spa/route?x=1",
        "http://localhost/?utm_source=x",
        "javascript:void(0)",
        "not a url at all",
        "",
        "https://kevinroebert.gitlab.io/ClearUrls/void/index.html?url=https%3A%2F%2Fexample.com%2F",
        "https://example.com/?mc_eid=abc&mc_cid=def",
        "https://example.com/path;matrix?utm_term=x",
        "https://example.com/?utm_source=%D1%82%D0%B5%D1%81%D1%82",
        "https://user:pass@example.com/?utm_source=x",
        "https://example.com:8443/?gclid=1&keep=2",
        "ftp://example.com/?utm_source=x",
        "https://example.com/??double=1",
        "https://example.com/?#",
        "https://example.com/#a=1&a=2&b",
        "https://out.example/track?dest=https%253A%252F%252Fexample.com",
    ] {
        let once = cleaner.clean(url);
        let twice = cleaner.clean(&once.url);
        assert_eq!(once.url, twice.url, "not idempotent for {url}");
    }
}
