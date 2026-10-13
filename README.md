# clear-urls

Remove tracking parameters from URLs. A Rust library port of the
[ClearURLs](https://github.com/ClearURLs/Addon) browser addon's cleaning
algorithm, with a maintained ruleset derived from
[ClearURLs rules](https://gitlab.com/ClearURLs/rules) (206 providers).

```rust
use clear_urls::UrlCleaner;

let cleaner = UrlCleaner::from_embedded_rules()?;

// Strip tracking parameters:
let result = cleaner.clean("https://example.com/page?utm_source=newsletter&id=42");
assert_eq!(result.url, "https://example.com/page?id=42");

// Unwrap redirect wrappers (and clean the target too):
let result = cleaner.clean("https://www.google.com/url?q=https%3A%2F%2Fexample.com%2F");
assert_eq!(result.url, "https://example.com/");
assert!(result.redirected);

// Detect domains ClearURLs blocks outright:
let result = cleaner.clean("https://kevinroebert.gitlab.io/ClearUrls/void/block.svg");
assert!(result.blocked);
```

Build a `UrlCleaner` once and reuse it: construction compiles all ~1100 rule
regexes eagerly, cleaning takes `&self`, and the cleaner is `Send + Sync`.
`clean` is infallible — input no rule applies to (including strings that
aren't URLs) comes back unchanged.

## Settings

The addon's user settings are exposed as builder toggles (defaults match the
addon):

```rust
let cleaner = UrlCleaner::from_embedded_rules()?
    .with_strip_referral_marketing(true) // also strip e.g. Amazon's `tag` (default: keep)
    .with_domain_blocking(false)         // don't flag blocked domains (default: flag)
    .with_skip_local_hosts(false);       // also clean localhost/private IPs (default: skip)
```

## `no_std`

Without the default `std` feature the crate is `no_std` (it still requires
`alloc`): regex matching runs on `regex-automata`'s meta engine — the same
engine inside the `regex` crate, so match behavior is identical. The `std`
feature pulls no extra dependencies, and `fetch` works independently of it —
it only needs a platform with std for the HTTP client.

```toml
[dependencies]
clear-urls = { version = "0.1", default-features = false, features = ["embedded-rules"] }
```

## Rules

- **Embedded** (default feature `embedded-rules`): the stable, supported ruleset
  maintained by this project. It is based on upstream ClearURLs rules and
  includes project-specific changes; see `data/README.md` for provenance.
- **Your own**: `UrlCleaner::from_rules_json(&str)` accepts any document in
  the ClearURLs rules schema.
- **Fresh from upstream** (feature `fetch`): `fetch_rules()` downloads
  `https://rules2.clearurls.xyz/data.minify.json` and verifies it against the
  published SHA-256 hash, like the addon does. Pick the TLS backend with the
  `rustls` or `native-tls` feature:

```toml
[dependencies]
clear-urls = { version = "0.1", features = ["fetch", "rustls"] }
# or: features = ["fetch", "native-tls"]
```

Fetching upstream rules returns the upstream ruleset, independently of this
project's maintained embedded rules.

## Compatibility

The cleaning engine follows the ClearURLs rules schema, with adjustments for
Rust and this library's API. The engine and embedded rules are maintained by
this project, so cleaning results may differ from the browser addon.

## License

LGPL-3.0-or-later, matching the ClearURLs addon and rules this is derived
from.
