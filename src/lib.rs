//! Remove tracking parameters from URLs
//!
//! ```
//! use clear_urls::UrlCleaner;
//!
//! # #[cfg(not(feature = "embedded-rules"))] fn main() {}
//! # #[cfg(feature = "embedded-rules")]
//! # fn main() -> Result<(), clear_urls::Error> {
//! let cleaner = UrlCleaner::from_embedded_rules()?;
//!
//! let result = cleaner.clean("https://example.com/page?utm_source=newsletter&id=42");
//! assert_eq!(result.url, "https://example.com/page?id=42");
//!
//! let result = cleaner.clean("https://www.google.com/url?q=https%3A%2F%2Fexample.com%2F");
//! assert_eq!(result.url, "https://example.com/");
//! assert!(result.redirected);
//! # Ok(())
//! # }
//! ```
//!
//! Build a [`UrlCleaner`] once and reuse it: construction compiles all rule
//! regexes eagerly (tens of milliseconds), cleaning takes `&self`, and the
//! cleaner is `Send + Sync`. Custom rules can be loaded with
//! [`UrlCleaner::from_rules_json`]; the `fetch` feature adds [`fetch_rules`]
//! for downloading the latest official rules (select its TLS backend with the `rustls` or `native-tls` feature).
//!
//! Without the default `std` feature the crate is `no_std` (it still needs `alloc`)

#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

mod engine;
mod error;
mod params;
mod pattern;
mod percent;
mod rules;

#[cfg(feature = "fetch")]
mod fetch;

use alloc::string::String;
use alloc::vec::Vec;

pub use error::Error;
#[cfg(feature = "fetch")]
pub use fetch::{DEFAULT_HASH_URL, DEFAULT_RULES_URL, FetchError, fetch_rules, fetch_rules_from};

/// `data.min.json` snapshot from <https://gitlab.com/ClearURLs/rules>.
#[cfg(feature = "embedded-rules")]
pub const EMBEDDED_RULES_JSON: &str = include_str!("../data/data.min.json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Sets [`CleanResult::blocked`] for `completeProvider` domains.
    pub domain_blocking: bool,
    /// Off by default so referral-marketing parameters (e.g. Amazon's `tag`) keep affiliate links working.
    pub strip_referral_marketing: bool,
    /// Leaves `localhost` and private-IPv4 URLs untouched.
    pub skip_local_hosts: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            domain_blocking: true,
            strip_referral_marketing: false,
            skip_local_hosts: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanResult {
    pub url: String,
    /// Whether [`url`](Self::url) differs from the input.
    pub changed: bool,
    /// The input was a redirect wrapper; [`url`](Self::url) is the embedded target.
    pub redirected: bool,
    /// The URL belongs to a `completeProvider` domain, meant to be blocked
    /// outright rather than rewritten; the URL itself is left as-is.
    pub blocked: bool,
}

pub struct UrlCleaner {
    providers: Vec<rules::Provider>,
    settings: Settings,
}

impl UrlCleaner {
    /// # Errors
    ///
    /// Only if the bundled [`EMBEDDED_RULES_JSON`] is broken, which would be
    /// a bug in this crate.
    #[cfg(feature = "embedded-rules")]
    pub fn from_embedded_rules() -> Result<Self, Error> {
        Self::from_rules_json(EMBEDDED_RULES_JSON)
    }

    /// Accepts the `ClearURLs` rules schema (`{"providers": {name: {…}}}`),
    /// e.g. a downloaded
    /// [`data.minify.json`](https://rules2.clearurls.xyz/data.minify.json).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidJson`] for a malformed document,
    /// [`Error::InvalidRegex`] for a provider pattern that fails to compile.
    pub fn from_rules_json(json: &str) -> Result<Self, Error> {
        Ok(Self {
            providers: rules::compile_providers(json)?,
            settings: Settings::default(),
        })
    }

    #[must_use]
    pub fn with_settings(mut self, settings: Settings) -> Self {
        self.settings = settings;
        self
    }

    #[must_use]
    pub fn with_domain_blocking(mut self, on: bool) -> Self {
        self.settings.domain_blocking = on;
        self
    }

    #[must_use]
    pub fn with_strip_referral_marketing(mut self, on: bool) -> Self {
        self.settings.strip_referral_marketing = on;
        self
    }

    #[must_use]
    pub fn with_skip_local_hosts(mut self, on: bool) -> Self {
        self.settings.skip_local_hosts = on;
        self
    }

    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    #[must_use]
    pub fn clean(&self, url: &str) -> CleanResult {
        engine::clean(&self.providers, &self.settings, url)
    }
}

impl core::fmt::Debug for UrlCleaner {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("UrlCleaner")
            .field("providers", &self.providers.len())
            .field("settings", &self.settings)
            .finish()
    }
}
