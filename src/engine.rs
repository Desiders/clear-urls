use crate::params::{FragmentParams, QueryParams};
use crate::percent::{decode_url, extract_host, is_local_host};
use crate::rules::Provider;
use crate::{CleanResult, Settings};
use alloc::borrow::{Cow, ToOwned};
use alloc::string::String;

/// Guards pathological custom rules; real rules converge in 2-3 passes.
const MAX_ITERATIONS: usize = 10;

/// This exact URL is excepted from every provider.
const SITE_BLOCKED_ALERT: &str = "javascript:void(0)";

pub(crate) fn clean(providers: &[Provider], settings: &Settings, url: &str) -> CleanResult {
    let mut redirected = false;
    let mut blocked = false;
    let mut current = Cow::Borrowed(url);
    for _ in 0..MAX_ITERATIONS {
        let Some(next) = clean_pass(providers, settings, &current, &mut redirected, &mut blocked)
        else {
            break;
        };
        let stable = next == *current;
        current = Cow::Owned(next);
        if stable {
            break;
        }
    }
    CleanResult {
        changed: current != url,
        url: current.into_owned(),
        redirected,
        blocked,
    }
}

fn clean_pass(
    providers: &[Provider],
    settings: &Settings,
    url: &str,
    redirected: &mut bool,
    blocked: &mut bool,
) -> Option<String> {
    if url == SITE_BLOCKED_ALERT {
        return None;
    }
    let mut current = Cow::Borrowed(url);
    for provider in providers {
        if !provider.url_pattern.is_match(&current)
            || provider
                .exceptions
                .iter()
                .any(|exception| exception.is_match(&current))
        {
            continue;
        }
        match remove_fields(provider, settings, &current) {
            Outcome::Redirect(target) => {
                *redirected = true;
                return Some(target);
            }
            Outcome::Blocked => *blocked = true,
            Outcome::Changed(url) => current = Cow::Owned(url),
            Outcome::Unchanged => {}
        }
    }
    match current {
        Cow::Borrowed(_) => None,
        Cow::Owned(url) => Some(url),
    }
}

enum Outcome {
    Redirect(String),
    Blocked,
    Changed(String),
    Unchanged,
}

fn remove_fields(provider: &Provider, settings: &Settings, url: &str) -> Outcome {
    if settings.skip_local_hosts && extract_host(url).is_some_and(is_local_host) {
        return Outcome::Unchanged;
    }

    for redirection in &provider.redirections {
        let target = redirection
            .capture_group1(url)
            .filter(|target| !target.is_empty());
        if let Some(target) = target {
            return Outcome::Redirect(decode_url(target));
        }
    }

    if provider.blocks_domain && settings.domain_blocking {
        return Outcome::Blocked;
    }

    let mut current = Cow::Borrowed(url);
    for raw_rule in &provider.raw_rules {
        if let Cow::Owned(replaced) = raw_rule.remove_all(&current) {
            current = Cow::Owned(replaced);
        }
    }

    let (base, query, fragment) = split_url(&current);
    let mut query = QueryParams::parse(query);
    let mut fragments = FragmentParams::parse(fragment);

    if query.is_empty() && fragments.is_empty() {
        return match current {
            Cow::Borrowed(_) => Outcome::Unchanged,
            Cow::Owned(url) => Outcome::Changed(url),
        };
    }

    let referral_marketing = settings
        .strip_referral_marketing
        .then_some(&provider.referral_marketing);
    for rule in provider
        .rules
        .iter()
        .chain(referral_marketing.into_iter().flatten())
    {
        query.remove_matching(|key| rule.is_match(key));
        fragments.remove_matching(|key| rule.is_match(key));
    }

    let mut rebuilt = base.to_owned();
    let query = query.serialize();
    if !query.is_empty() {
        rebuilt.push('?');
        rebuilt.push_str(&query);
    }
    let fragments = fragments.serialize();
    if !fragments.is_empty() {
        rebuilt.push('#');
        rebuilt.push_str(&fragments);
    }
    let rebuilt = rebuilt.replacen("?&", "?", 1).replacen("#&", "#", 1);

    if rebuilt == url {
        Outcome::Unchanged
    } else {
        Outcome::Changed(rebuilt)
    }
}

/// The fragment starts at the first `#`; the query is between the first `?`
/// before that and the `#`. Separators are not included in the parts.
fn split_url(url: &str) -> (&str, &str, &str) {
    let (before_hash, fragment) = url.split_once('#').unwrap_or((url, ""));
    let (base, query) = before_hash.split_once('?').unwrap_or((before_hash, ""));
    (base, query, fragment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::compile_providers;
    use alloc::format;
    use alloc::string::ToString;
    use alloc::vec::Vec;
    use serde_json::{Value, json};

    fn providers(rules: Value) -> Vec<Provider> {
        compile_providers(&rules.to_string()).unwrap()
    }

    fn run(providers: &[Provider], url: &str) -> CleanResult {
        clean(providers, &Settings::default(), url)
    }

    #[test]
    fn split_url_variants() {
        assert_eq!(
            split_url("https://x.com/p?a=1#b=2"),
            ("https://x.com/p", "a=1", "b=2")
        );
        assert_eq!(split_url("https://x.com/p"), ("https://x.com/p", "", ""));
        assert_eq!(split_url("https://x.com/?a"), ("https://x.com/", "a", ""));
        assert_eq!(
            split_url("https://x.com/#/route?a=1"),
            ("https://x.com/", "", "/route?a=1")
        );
    }

    #[test]
    fn removes_matching_params_only() {
        let rule_set = providers(json!({"providers": {"p": {"rules": ["utm_[a-z]+"]}}}));
        let result = run(&rule_set, "https://x.com/?utm_source=a&id=1&utm_medium=b");
        assert_eq!(result.url, "https://x.com/?id=1");
        assert!(result.changed);
        assert!(!result.redirected);
        assert!(!result.blocked);
    }

    #[test]
    fn rules_match_names_not_values() {
        let rule_set = providers(json!({"providers": {"p": {"rules": ["tracker"]}}}));
        let result = run(&rule_set, "https://x.com/?id=tracker");
        assert_eq!(result.url, "https://x.com/?id=tracker");
        assert!(!result.changed);
    }

    #[test]
    fn rules_are_case_insensitive_on_names() {
        let rule_set = providers(json!({"providers": {"p": {"rules": ["fbclid"]}}}));
        assert_eq!(
            run(&rule_set, "https://x.com/?FBCLID=abc&ok=1").url,
            "https://x.com/?ok=1"
        );
    }

    #[test]
    fn all_duplicate_params_removed() {
        let rule_set = providers(json!({"providers": {"p": {"rules": ["dup"]}}}));
        assert_eq!(
            run(&rule_set, "https://x.com/?dup=1&keep=2&dup=3").url,
            "https://x.com/?keep=2"
        );
    }

    #[test]
    fn fragment_params_cleaned_too() {
        let rule_set = providers(json!({"providers": {"p": {"rules": ["utm_source"]}}}));
        assert_eq!(
            run(&rule_set, "https://x.com/page#utm_source=x&anchor").url,
            "https://x.com/page#anchor"
        );
        assert_eq!(
            run(&rule_set, "https://x.com/page#utm_source=x").url,
            "https://x.com/page"
        );
    }

    #[test]
    fn empty_query_dropped_entirely() {
        let rule_set = providers(json!({"providers": {"p": {"rules": ["a"]}}}));
        assert_eq!(run(&rule_set, "https://x.com/?a=1").url, "https://x.com/");
    }

    #[test]
    fn url_without_params_returned_byte_identical() {
        let rule_set = providers(json!({"providers": {"p": {"rules": ["a"]}}}));
        let url = "HTTPS://X.com/Path%2F";
        let result = run(&rule_set, url);
        assert_eq!(result.url, url);
        assert!(!result.changed);
    }

    #[test]
    fn garbage_in_garbage_out() {
        let rule_set = providers(json!({"providers": {"p": {"rules": ["a"]}}}));
        for garbage in ["", "not a url", "::::", "mailto:x@y.z"] {
            let result = run(&rule_set, garbage);
            assert_eq!(result.url, garbage);
            assert!(!result.changed);
        }
    }

    #[test]
    fn url_pattern_limits_provider() {
        let rule_set = providers(
            json!({"providers": {"p": {"urlPattern": "^https?://x\\.com", "rules": ["t"]}}}),
        );
        assert_eq!(run(&rule_set, "https://x.com/?t=1").url, "https://x.com/");
        assert_eq!(
            run(&rule_set, "https://y.com/?t=1").url,
            "https://y.com/?t=1"
        );
    }

    #[test]
    fn exceptions_skip_provider() {
        let rule_set = providers(
            json!({"providers": {"p": {"rules": ["t"], "exceptions": ["x\\.com/keep"]}}}),
        );
        assert_eq!(
            run(&rule_set, "https://x.com/keep?t=1").url,
            "https://x.com/keep?t=1"
        );
        assert_eq!(
            run(&rule_set, "https://x.com/other?t=1").url,
            "https://x.com/other"
        );
    }

    #[test]
    fn raw_rules_rewrite_whole_url() {
        let rule_set = providers(json!({"providers": {"p": {"rawRules": ["\\/ref=[^/?]*"]}}}));
        let result = run(&rule_set, "https://x.com/item/ref=sr_1_2?id=1");
        assert_eq!(result.url, "https://x.com/item?id=1");
        assert!(result.changed);
        assert_eq!(
            run(&rule_set, "https://x.com/a/REF=x/b/ref=y").url,
            "https://x.com/a/b"
        );
    }

    #[test]
    fn redirection_unwraps_and_decodes() {
        let rule_set = providers(
            json!({"providers": {"p": {"urlPattern": "^https?://out\\.x\\.com",
                "redirections": ["out\\.x\\.com/link\\?to=([^&]+)"]}}}),
        );
        let result = run(
            &rule_set,
            "https://out.x.com/link?to=https%3A%2F%2Fexample.com%2Fpage",
        );
        assert_eq!(result.url, "https://example.com/page");
        assert!(result.redirected);
        assert!(result.changed);
    }

    #[test]
    fn redirection_decodes_multiply_encoded_targets() {
        let rule_set =
            providers(json!({"providers": {"p": {"redirections": ["x\\.com/r\\?u=(.+)"]}}}));
        assert_eq!(
            run(
                &rule_set,
                "https://x.com/r?u=https%253A%252F%252Fexample.com"
            )
            .url,
            "https://example.com"
        );
    }

    #[test]
    fn redirection_prepends_http_scheme() {
        let rule_set =
            providers(json!({"providers": {"p": {"redirections": ["x\\.com/r\\?u=([^&]+)"]}}}));
        assert_eq!(
            run(&rule_set, "https://x.com/r?u=example.com%2Fpage").url,
            "http://example.com/page"
        );
    }

    #[test]
    fn redirection_target_gets_cleaned_by_fixpoint() {
        let rule_set = providers(json!({"providers": {
            "redir": {"urlPattern": "x\\.com", "redirections": ["x\\.com/r\\?u=([^&]+)"]},
            "global": {"urlPattern": ".*", "rules": ["utm_[a-z]+"]}
        }}));
        let result = run(
            &rule_set,
            "https://x.com/r?u=https%3A%2F%2Fexample.com%2F%3Futm_source%3Dx%26id%3D1",
        );
        assert_eq!(result.url, "https://example.com/?id=1");
        assert!(result.redirected);
    }

    #[test]
    fn first_matching_provider_redirection_wins() {
        let rule_set = providers(json!({"providers": {
            "first": {"redirections": ["x\\.com/r\\?u=([^&]+)"]},
            "second": {"redirections": ["x\\.com/(r)\\?"]}
        }}));
        assert_eq!(
            run(&rule_set, "https://x.com/r?u=https%3A%2F%2Fexample.com").url,
            "https://example.com"
        );
    }

    #[test]
    fn redirection_without_capture_participation_is_skipped() {
        let rule_set = providers(
            json!({"providers": {"p": {"redirections": ["x\\.com/(?:go|r=(.+))"], "rules": ["t"]}}}),
        );
        let result = run(&rule_set, "https://x.com/go?t=1");
        assert!(!result.redirected);
        assert_eq!(result.url, "https://x.com/go");
    }

    #[test]
    fn complete_provider_sets_blocked() {
        let rule_set = providers(
            json!({"providers": {"p": {"urlPattern": "tracker\\.com", "completeProvider": true}}}),
        );
        let result = run(&rule_set, "https://tracker.com/pixel?x=1");
        assert!(result.blocked);
        assert_eq!(result.url, "https://tracker.com/pixel?x=1");
        assert!(!result.changed);

        let settings = Settings {
            domain_blocking: false,
            ..Settings::default()
        };
        let result = clean(&rule_set, &settings, "https://tracker.com/pixel?x=1");
        assert!(!result.blocked);
        assert_eq!(result.url, "https://tracker.com/pixel");
    }

    #[test]
    fn referral_marketing_kept_by_default_stripped_on_demand() {
        let rule_set =
            providers(json!({"providers": {"p": {"rules": ["t"], "referralMarketing": ["tag"]}}}));
        let url = "https://x.com/?tag=aff-21&t=1&id=2";
        assert_eq!(run(&rule_set, url).url, "https://x.com/?tag=aff-21&id=2");

        let settings = Settings {
            strip_referral_marketing: true,
            ..Settings::default()
        };
        assert_eq!(clean(&rule_set, &settings, url).url, "https://x.com/?id=2");
    }

    #[test]
    fn local_hosts_skipped_by_default() {
        let rule_set = providers(json!({"providers": {"p": {"rules": ["t"]}}}));
        for url in [
            "http://localhost:3000/?t=1",
            "http://127.0.0.1/?t=1",
            "https://192.168.1.10/admin?t=1",
            "http://10.0.0.5/?t=1",
        ] {
            let result = run(&rule_set, url);
            assert_eq!(result.url, url, "{url} should be skipped");
        }

        let settings = Settings {
            skip_local_hosts: false,
            ..Settings::default()
        };
        assert_eq!(
            clean(&rule_set, &settings, "http://localhost:3000/?t=1").url,
            "http://localhost:3000/"
        );
    }

    #[test]
    fn providers_compose_within_one_pass() {
        let rule_set = providers(json!({"providers": {
            "one": {"rules": ["a"]},
            "two": {"rules": ["b"]}
        }}));
        assert_eq!(
            run(&rule_set, "https://x.com/?a=1&b=2&c=3").url,
            "https://x.com/?c=3"
        );
    }

    #[test]
    fn site_blocked_alert_never_cleaned() {
        let rule_set = providers(json!({"providers": {"p": {"rules": [".*"]}}}));
        let result = run(&rule_set, "javascript:void(0)");
        assert_eq!(result.url, "javascript:void(0)");
        assert!(!result.changed);
    }

    #[test]
    fn fixpoint_iteration_is_capped() {
        let rule_set = providers(
            json!({"providers": {"p": {"urlPattern": "x\\.com", "rawRules": ["/(aa)$"]}}}),
        );
        let url = format!("https://x.com{}", "/aa".repeat(20));
        let result = run(&rule_set, &url);
        assert_eq!(result.url.matches("/aa").count(), 20 - MAX_ITERATIONS);
    }

    #[test]
    fn changed_reflects_serialization_normalization() {
        let rule_set = providers(json!({"providers": {"p": {"rules": ["zzz"]}}}));
        let result = run(&rule_set, "https://x.com/?q=a+b");
        assert_eq!(result.url, "https://x.com/?q=a%20b");
        assert!(result.changed);
    }
}
