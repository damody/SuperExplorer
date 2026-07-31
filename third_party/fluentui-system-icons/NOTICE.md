# Microsoft Fluent UI System Icons attribution

- Upstream: https://github.com/microsoft/fluentui-system-icons
- Pinned source revision: `e80a673366c382be76fa80485cd68669cfa49a1a`
- License: MIT; see `LICENSE` in this directory.
- Variant: 20px regular SVG.

`crates/explorer-ui/src/fluent_assets.rs` embeds the upstream path data for these official assets:

| Explorer name | Upstream asset |
| --- | --- |
| back / forward / up | Arrow Left / Arrow Right / Arrow Up |
| refresh | Arrow Clockwise |
| new | Add Circle |
| cut / copy / paste | Cut / Copy / Clipboard Paste |
| rename / share / delete | Rename / Share / Delete |
| sort / view / more | Arrow Sort / Board / More Horizontal |
| details / search / close | Text Bullet List Square / Search / Dismiss |
| chevron / chevron-down | Chevron Right / Chevron Down |
| minimize / maximize / restore | Subtract / Square / Square Multiple |
| pin | Pin |

Only the SVG wrapper fill is changed from the upstream fixed `#212121` to `currentColor` at the
embedded asset boundary so the same official geometry follows light, dark, and high-contrast themes.
