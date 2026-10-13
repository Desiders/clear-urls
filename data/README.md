# Embedded rules

`data.min.json` contains this project's stable, supported default ruleset. It
is derived from <https://gitlab.com/ClearURLs/rules> and maintained here with
project-specific changes.

- Upstream base commit: `11086f40512774dcadef54079f1ba023bfacf940` (2026-03-25)
- Live (minified) version: <https://rules2.clearurls.xyz/data.minify.json>
- License: LGPL-3.0

To update: review and merge changes from upstream while preserving this
project's customizations. Update the base commit above and run the embedded
rules tests with `cargo test --locked --test embedded_rules` before committing.
The output of `fetch_rules()` contains upstream rules and does not include this
project's customizations.
