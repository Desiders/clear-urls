//! Query and fragment parameters have deliberately different parse and serialize semantics

use crate::percent::encode_uri_component;
use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug)]
pub(crate) struct QueryParams<'a> {
    pairs: Vec<(Cow<'a, str>, Cow<'a, str>)>,
}

impl<'a> QueryParams<'a> {
    pub(crate) fn parse(query: &'a str) -> Self {
        Self {
            pairs: form_urlencoded::parse(query.as_bytes()).collect(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    pub(crate) fn remove_matching(&mut self, mut matches: impl FnMut(&str) -> bool) {
        self.pairs.retain(|(key, _)| !matches(key));
    }

    /// Values are re-encoded, keys emitted as-is, empty value - bare key.
    pub(crate) fn serialize(&self) -> String {
        let mut out = String::new();
        for (key, value) in &self.pairs {
            if !out.is_empty() {
                out.push('&');
            }
            out.push_str(key);
            if !value.is_empty() {
                out.push('=');
                out.extend(encode_uri_component(value));
            }
        }
        out
    }
}

#[derive(Debug)]
pub(crate) struct FragmentParams<'a> {
    entries: Vec<(&'a str, Vec<Option<&'a str>>)>,
}

impl<'a> FragmentParams<'a> {
    /// A value exists only for exactly one `=` with a non-empty right-hand
    /// side: `a=b=c`, `a=` and `a` all become the bare key `a`. Empty keys
    /// are skipped; nothing is percent-decoded.
    pub(crate) fn parse(fragment: &'a str) -> Self {
        let mut params = Self {
            entries: Vec::new(),
        };
        for token in fragment.split('&') {
            let (key, value) = match token.split_once('=') {
                Some((key, value)) if !value.is_empty() && !value.contains('=') => {
                    (key, Some(value))
                }
                Some((key, _)) => (key, None),
                None => (token, None),
            };
            if !key.is_empty() {
                params.append(key, value);
            }
        }
        params
    }

    fn append(&mut self, key: &'a str, value: Option<&'a str>) {
        if let Some((_, values)) = self
            .entries
            .iter_mut()
            .find(|(existing_key, _)| *existing_key == key)
        {
            if !values.contains(&value) {
                values.push(value);
            }
        } else {
            self.entries.push((key, vec![value]));
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn remove_matching(&mut self, mut matches: impl FnMut(&str) -> bool) {
        self.entries.retain(|(key, _)| !matches(key));
    }

    pub(crate) fn serialize(&self) -> String {
        let mut out = String::new();
        for (key, values) in &self.entries {
            for value in values {
                if !out.is_empty() {
                    out.push('&');
                }
                out.push_str(key);
                if let Some(value) = value {
                    out.push('=');
                    out.push_str(value);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_parse_preserves_order_and_duplicates() {
        let query = QueryParams::parse("a=1&b=2&a=3");
        assert_eq!(query.serialize(), "a=1&b=2&a=3");
    }

    #[test]
    fn query_parse_form_urlencoded_semantics() {
        let query = QueryParams::parse("a=x+y&b=%C3%A9&c&d=");
        assert_eq!(
            query.pairs,
            vec![
                ("a".into(), "x y".into()),
                ("b".into(), "é".into()),
                ("c".into(), "".into()),
                ("d".into(), "".into()),
            ]
        );
        assert!(QueryParams::parse("").is_empty());
        assert!(QueryParams::parse("&&").is_empty());
    }

    #[test]
    fn query_serialize_matches_addon() {
        let query = QueryParams::parse("a=x+y&bare&b=a%2Fb");
        assert_eq!(query.serialize(), "a=x%20y&bare&b=a%2Fb");
    }

    #[test]
    fn query_serialize_does_not_reencode_keys() {
        let query = QueryParams::parse("a%20b=1");
        assert_eq!(query.serialize(), "a b=1");
    }

    #[test]
    fn query_remove_matching_removes_all_duplicates() {
        let mut query = QueryParams::parse("utm_source=x&keep=1&utm_source=y");
        query.remove_matching(|key| key == "utm_source");
        assert_eq!(query.serialize(), "keep=1");
    }

    #[test]
    fn fragment_parse_value_rules() {
        let fragment = FragmentParams::parse("a=1&b&c=&d=x=y");
        assert_eq!(
            fragment.entries,
            vec![
                ("a", vec![Some("1")]),
                ("b", vec![None]),
                ("c", vec![None]),
                ("d", vec![None]),
            ]
        );
    }

    #[test]
    fn fragment_parse_skips_empty_keys_and_keeps_encoding() {
        let fragment = FragmentParams::parse("&=5&a=%20");
        assert_eq!(fragment.entries, vec![("a", vec![Some("%20")])]);
        assert!(FragmentParams::parse("").is_empty());
    }

    #[test]
    fn fragment_multimap_groups_by_key_and_dedupes() {
        let fragment = FragmentParams::parse("a=1&b=2&a=3&a=1");
        assert_eq!(fragment.serialize(), "a=1&a=3&b=2");
    }

    #[test]
    fn fragment_remove_matching() {
        let mut fragment = FragmentParams::parse("utm_source=x&section");
        fragment.remove_matching(|key| key == "utm_source");
        assert_eq!(fragment.serialize(), "section");
    }
}
