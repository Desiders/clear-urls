use crate::error::Error;
use crate::pattern::Pattern;
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::mem;
use miniserde::de::{Map, Visitor};
use miniserde::{Deserialize, make_place};

make_place!(Place);

#[derive(Default)]
struct ProviderData {
    url_pattern: String,
    complete_provider: bool,
    rules: Vec<String>,
    raw_rules: Vec<String>,
    referral_marketing: Vec<String>,
    exceptions: Vec<String>,
    redirections: Vec<String>,
}

/// Manual impl instead of `miniserde::Deserialize` derive: every field is
/// optional in the schema (the minified rules strip empty ones), while the
/// derive treats a missing field as an error.
impl Deserialize for ProviderData {
    fn begin(out: &mut Option<Self>) -> &mut dyn Visitor {
        Place::new(out)
    }
}

impl Visitor for Place<ProviderData> {
    fn map(&mut self) -> miniserde::Result<Box<dyn Map + '_>> {
        Ok(Box::new(ProviderDataBuilder {
            out: &mut self.out,
            url_pattern: None,
            complete_provider: None,
            rules: None,
            raw_rules: None,
            referral_marketing: None,
            exceptions: None,
            redirections: None,
        }))
    }
}

struct ProviderDataBuilder<'a> {
    out: &'a mut Option<ProviderData>,
    url_pattern: Option<String>,
    complete_provider: Option<bool>,
    rules: Option<Vec<String>>,
    raw_rules: Option<Vec<String>>,
    referral_marketing: Option<Vec<String>>,
    exceptions: Option<Vec<String>>,
    redirections: Option<Vec<String>>,
}

impl Map for ProviderDataBuilder<'_> {
    fn key(&mut self, key: &str) -> miniserde::Result<&mut dyn Visitor> {
        Ok(match key {
            "urlPattern" => Deserialize::begin(&mut self.url_pattern),
            "completeProvider" => Deserialize::begin(&mut self.complete_provider),
            "rules" => Deserialize::begin(&mut self.rules),
            "rawRules" => Deserialize::begin(&mut self.raw_rules),
            "referralMarketing" => Deserialize::begin(&mut self.referral_marketing),
            "exceptions" => Deserialize::begin(&mut self.exceptions),
            "redirections" => Deserialize::begin(&mut self.redirections),
            _ => <dyn Visitor>::ignore(),
        })
    }

    fn finish(&mut self) -> miniserde::Result<()> {
        *self.out = Some(ProviderData {
            url_pattern: self.url_pattern.take().unwrap_or_default(),
            complete_provider: self.complete_provider.take().unwrap_or_default(),
            rules: self.rules.take().unwrap_or_default(),
            raw_rules: self.raw_rules.take().unwrap_or_default(),
            referral_marketing: self.referral_marketing.take().unwrap_or_default(),
            exceptions: self.exceptions.take().unwrap_or_default(),
            redirections: self.redirections.take().unwrap_or_default(),
        });
        Ok(())
    }
}

/// The `preserve_order` analog: a `Vec`-backed map,
/// because provider order is load-bearing (providers apply in rules-file order).
struct OrderedProviders(Vec<(String, ProviderData)>);

impl Deserialize for OrderedProviders {
    fn begin(out: &mut Option<Self>) -> &mut dyn Visitor {
        Place::new(out)
    }
}

impl Visitor for Place<OrderedProviders> {
    fn map(&mut self) -> miniserde::Result<Box<dyn Map + '_>> {
        Ok(Box::new(ProvidersBuilder {
            out: &mut self.out,
            providers: Vec::new(),
            key: None,
            value: None,
        }))
    }
}

struct ProvidersBuilder<'a> {
    out: &'a mut Option<OrderedProviders>,
    providers: Vec<(String, ProviderData)>,
    key: Option<String>,
    value: Option<ProviderData>,
}

impl ProvidersBuilder<'_> {
    fn shift(&mut self) {
        if let (Some(key), Some(value)) = (self.key.take(), self.value.take()) {
            self.providers.push((key, value));
        }
    }
}

impl Map for ProvidersBuilder<'_> {
    fn key(&mut self, key: &str) -> miniserde::Result<&mut dyn Visitor> {
        self.shift();
        self.key = Some(key.to_owned());
        Ok(Deserialize::begin(&mut self.value))
    }

    fn finish(&mut self) -> miniserde::Result<()> {
        self.shift();
        *self.out = Some(OrderedProviders(mem::take(&mut self.providers)));
        Ok(())
    }
}

struct RulesFile {
    providers: Vec<(String, ProviderData)>,
}

impl Deserialize for RulesFile {
    fn begin(out: &mut Option<Self>) -> &mut dyn Visitor {
        Place::new(out)
    }
}

impl Visitor for Place<RulesFile> {
    fn map(&mut self) -> miniserde::Result<Box<dyn Map + '_>> {
        Ok(Box::new(RulesFileBuilder {
            out: &mut self.out,
            providers: None,
        }))
    }
}

struct RulesFileBuilder<'a> {
    out: &'a mut Option<RulesFile>,
    providers: Option<OrderedProviders>,
}

impl Map for RulesFileBuilder<'_> {
    fn key(&mut self, key: &str) -> miniserde::Result<&mut dyn Visitor> {
        Ok(match key {
            "providers" => Deserialize::begin(&mut self.providers),
            _ => <dyn Visitor>::ignore(),
        })
    }

    fn finish(&mut self) -> miniserde::Result<()> {
        *self.out = Some(RulesFile {
            providers: self.providers.take().map(|p| p.0).unwrap_or_default(),
        });
        Ok(())
    }
}

/// All patterns are case-insensitive; the parameter-name lists (`rules`,
/// `referral_marketing`) are anchored as `^rule$`, the rest are unanchored.
/// `blocks_domain` is `completeProvider` in the schema.
#[derive(Debug)]
pub(crate) struct Provider {
    pub(crate) url_pattern: Pattern,
    pub(crate) blocks_domain: bool,
    pub(crate) rules: Vec<Pattern>,
    pub(crate) referral_marketing: Vec<Pattern>,
    pub(crate) raw_rules: Vec<Pattern>,
    pub(crate) exceptions: Vec<Pattern>,
    pub(crate) redirections: Vec<Pattern>,
}

pub(crate) fn compile_providers(json: &str) -> Result<Vec<Provider>, Error> {
    let file: RulesFile = miniserde::json::from_str(json).map_err(|_| Error::InvalidJson)?;
    file.providers
        .into_iter()
        .map(|(name, data)| compile_provider(&name, &data))
        .collect()
}

fn compile_provider(name: &str, data: &ProviderData) -> Result<Provider, Error> {
    let compile = |pattern: &str| {
        Pattern::new(pattern).map_err(|build_error| Error::InvalidRegex {
            provider: name.to_owned(),
            pattern: pattern.to_owned(),
            message: build_error.to_string(),
        })
    };
    // Anchored literally without grouping: a top-level alternation `a|b`
    // behaves as `^a|b$`.
    let compile_anchored = |rule: &str| compile(&format!("^{rule}$"));
    let compile_all = |patterns: &[String]| -> Result<Vec<Pattern>, Error> {
        patterns.iter().map(|pattern| compile(pattern)).collect()
    };

    // completeProvider implies an implicit `.*` rule.
    let implicit = data.complete_provider.then_some(".*");
    let rules = implicit
        .into_iter()
        .chain(data.rules.iter().map(String::as_str))
        .map(compile_anchored)
        .collect::<Result<_, _>>()?;

    Ok(Provider {
        url_pattern: compile(&data.url_pattern)?,
        blocks_domain: data.complete_provider,
        rules,
        referral_marketing: data
            .referral_marketing
            .iter()
            .map(|rule| compile_anchored(rule))
            .collect::<Result<_, _>>()?,
        raw_rules: compile_all(&data.raw_rules)?,
        exceptions: compile_all(&data.exceptions)?,
        redirections: compile_all(&data.redirections)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn providers_from(rules: serde_json::Value) -> Result<Vec<Provider>, Error> {
        compile_providers(&rules.to_string())
    }

    #[test]
    fn defaults_for_missing_fields() {
        let providers = providers_from(json!({"providers": {"p": {}}})).unwrap();
        assert_eq!(providers.len(), 1);
        let provider = &providers[0];
        assert!(provider.url_pattern.is_match("https://example.com"));
        assert!(!provider.blocks_domain);
        assert!(provider.rules.is_empty());
        assert!(provider.raw_rules.is_empty());
        assert!(provider.referral_marketing.is_empty());
        assert!(provider.exceptions.is_empty());
        assert!(provider.redirections.is_empty());
    }

    #[test]
    fn unknown_fields_tolerated() {
        let rules = json!({
            "providers": {"p": {"urlPattern": "x", "forceRedirection": true, "methods": ["GET"]}},
            "extraTopLevel": true
        });
        assert!(providers_from(rules).is_ok());
    }

    #[test]
    fn provider_order_preserved() {
        let rules = json!({"providers": {
            "zzz": {"rules": ["a"]},
            "aaa": {"rawRules": ["b"]},
            "mmm": {"redirections": ["(c)"]}
        }});
        let providers = providers_from(rules).unwrap();
        assert_eq!(providers.len(), 3);
        assert_eq!(providers[0].rules.len(), 1);
        assert_eq!(providers[1].raw_rules.len(), 1);
        assert_eq!(providers[2].redirections.len(), 1);
    }

    #[test]
    fn complete_provider_gets_implicit_rule() {
        let rules = json!({"providers": {"p": {"completeProvider": true, "rules": ["foo"]}}});
        let provider = &providers_from(rules).unwrap()[0];
        assert!(provider.blocks_domain);
        assert_eq!(provider.rules.len(), 2);
        assert!(provider.rules[0].is_match("anything_at_all"));
    }

    #[test]
    fn rules_are_anchored_and_case_insensitive() {
        let rules = json!({"providers": {"p": {"rules": ["utm_source"]}}});
        let providers = providers_from(rules).unwrap();
        let rule = &providers[0].rules[0];
        assert!(rule.is_match("utm_source"));
        assert!(rule.is_match("UTM_SOURCE"));
        assert!(!rule.is_match("xutm_source"));
        assert!(!rule.is_match("utm_sourcex"));
    }

    #[test]
    fn invalid_regex_error_carries_context() {
        let rules = json!({"providers": {"bad": {"rules": ["("]}}});
        match providers_from(rules) {
            Err(Error::InvalidRegex {
                provider, pattern, ..
            }) => {
                assert_eq!(provider, "bad");
                assert_eq!(pattern, "^($");
            }
            other => panic!("expected InvalidRegex, got {other:?}"),
        }
    }

    #[test]
    fn invalid_json_error() {
        assert!(matches!(
            compile_providers("not json"),
            Err(Error::InvalidJson)
        ));
        assert!(matches!(
            providers_from(json!({"providers": []})),
            Err(Error::InvalidJson)
        ));
    }

    #[test]
    fn missing_providers_key_is_empty() {
        assert!(providers_from(json!({})).unwrap().is_empty());
    }

    #[test]
    fn duplicate_rule_values_kept() {
        let rules = json!({"providers": {"p": {"rules": ["a", "a"]}}});
        assert_eq!(providers_from(rules).unwrap()[0].rules.len(), 2);
    }
}
