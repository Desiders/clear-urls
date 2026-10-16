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

## Instagram parameter policy

Instagram frequently changes the tracking parameter names on shared links.
The embedded provider removes all four-character names, covering existing
names such as `igsh`, `igsi`, and `stkn` and future names of that length. It
also removes the older six-character `igshid` explicitly. Other name lengths,
including content selectors such as `img_index` and `story_media_id`, are left
to the remaining rules.

This is a length heuristic: every four-character name is removed regardless
of its purpose, and new trackers of other lengths need explicit rules. It uses
the existing `rules` field without changing the schema or cleaning algorithm.
