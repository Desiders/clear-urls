use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use regex_automata::Input;
use regex_automata::meta::{BuildError, Regex};
use regex_automata::util::syntax;

#[derive(Debug)]
pub(crate) struct Pattern {
    regex: Regex,
}

impl Pattern {
    pub(crate) fn new(pattern: &str) -> Result<Self, Box<BuildError>> {
        Ok(Self {
            regex: Regex::builder()
                .syntax(syntax::Config::new().case_insensitive(true))
                .build(pattern)
                .map_err(Box::new)?,
        })
    }

    pub(crate) fn is_match(&self, haystack: &str) -> bool {
        self.regex.is_match(haystack)
    }

    pub(crate) fn capture_group1<'a>(&self, haystack: &'a str) -> Option<&'a str> {
        let mut captures = self.regex.create_captures();
        self.regex.captures(haystack, &mut captures);
        captures.get_group(1).map(|span| &haystack[span.range()])
    }

    pub(crate) fn remove_all<'a>(&self, haystack: &'a str) -> Cow<'a, str> {
        let mut matches = self
            .regex
            .find_iter(Input::new(haystack))
            .map(|found| found.range())
            .peekable();
        if matches.peek().is_none() {
            return Cow::Borrowed(haystack);
        }
        let mut out = String::with_capacity(haystack.len());
        let mut last_end = 0;
        for range in matches {
            out.push_str(&haystack[last_end..range.start]);
            last_end = range.end;
        }
        out.push_str(&haystack[last_end..]);
        Cow::Owned(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive_matching() {
        let pattern = Pattern::new("^utm_source$").unwrap();
        assert!(pattern.is_match("UTM_SOURCE"));
        assert!(!pattern.is_match("xutm_source"));
    }

    #[test]
    fn capture_group1_requires_participation() {
        let pattern = Pattern::new("x\\.com/(?:go|r=(.+))").unwrap();
        assert_eq!(
            pattern.capture_group1("https://x.com/r=target"),
            Some("target")
        );
        assert_eq!(pattern.capture_group1("https://x.com/go"), None);
        assert_eq!(pattern.capture_group1("https://y.com/"), None);
    }

    #[test]
    fn remove_all_matches() {
        let pattern = Pattern::new("/ref=[^/?]*").unwrap();
        assert_eq!(
            pattern.remove_all("https://x.com/a/REF=x/b/ref=y"),
            "https://x.com/a/b"
        );
        assert!(matches!(
            pattern.remove_all("https://x.com/a"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn invalid_pattern_is_rejected() {
        assert!(Pattern::new("(").is_err());
    }
}
