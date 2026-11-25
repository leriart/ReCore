# recore-css

ReCore's reader for Discord CSS themes. It reads a `.theme.css` file —
Vencord, Vesktop, Equicord, Equibop, BetterDiscord, Legcord, Dorion,
GoofCord, Replugged and any other client that loads a file of custom
properties — and turns it into ReCore's colours, look and options.

A theme is **data, never code**. A damaged or hostile file is read as far as
it makes sense, the rest is left out, and nothing panics or loops forever.
[`docs/themes.md`](../../docs/themes.md) is the source of truth for what is
painted and what is kept in the file; this README is the crate's entry point
and compatibility summary.

## Install

```toml
[dependencies]
recore-css = { path = "crates/recore-css" }
```

The crate depends only on `style-engine` (layout feature). It is a library
with a small command-line tool for checking a theme by hand.

## Use

```rust
use recore_css::{load, Mode, Options};

// Read a file, settling it for the window's 1280×720 reference size.
let theme = load("themes/Ambxst.theme.css").expect("readable");

// Or from a string, forcing the light palette.
let css = ":root { --background-primary: #fff; --brand-experiment: #e94221; }";
let theme = recore_css::load_str(css, None, &Options::with_mode(Mode::Light))
    .expect("readable");

assert_eq!(theme.palette.accent.hex(), "#e94221");
```

`load` and `scan` walk the local themes; `load_with_imports`,
`load_str_with_imports` and `scan_with_imports` take a callback so a theme's
`http:`/`https:` imports, fonts and pictures can be fetched. `theme_folders`
names the folders the common clients keep local themes in. `Options` chooses
the dark or light set, forces an accent, or overrides a custom property. The
result is a `ThemeFile` with the resolved `Palette`, `Look`, fonts, options,
warnings and the `/** @name ... */` header's metadata.

### Command line

The crate builds a `recore-css` binary:

```bash
cargo run -p recore-css --bin recore-css -- --help
cargo run -p recore-css --bin recore-css -- themes/Ambxst.theme.css
cargo run -p recore-css --bin recore-css -- --light themes/Ambxst.theme.css
```

It prints each theme's id, name, dark/light, the chat, sidebar, rail, text and
accent colours, the radius, the font family and the option and font counts.
Warnings go to stderr. The exit code is `0` when every file read, `1` when any
failed, and `2` for a usage error.

## Compatibility

### At-rules

| At-rule | State |
| --- | --- |
| `@import` | Read beside the theme; remote URLs through the fetch callback. `layer(...)`, `supports(...)` and `media` conditions are applied. |
| `@media` | Settled at the 1280×720 reference viewport (`prefers-color-scheme`, `prefers-reduced-motion`, `hover`, `pointer`, `orientation`, `width`/`height`, `aspect-ratio`, `resolution`, `color-gamut`, …). |
| `@supports` | Conditions are evaluated; a feature the native window cannot paint takes its `not (...)` branch. |
| `@font-face` | `data:` URIs, files beside the theme and remote URLs are loaded into the window's fonts. |
| `@keyframes` | A root/body animation of `background`, `color` or `opacity` is painted as a quiet ground wash. |
| `@property` | Syntax and `initial-value` fill a variable the theme does not set. |
| `@container` | `style(...)` and `not style(...)` read the theme's own settled variables; size queries use the reference window. |
| `@layer` | Statements and blocks; layer order is fixed before the cascade. |
| `@charset`, `@namespace`, `@counter-style` | Parsed without error; not applied to a native window. |
| `@scope` | Its rules are read as if unscoped: the native window has no DOM to limit them to, and a scope whose root sits outside a region would otherwise hide a theme's colours. |

### Selectors

Type, class, id and attribute selectors (`[attr]`, `[attr=...]`,
`[attr*="..."]`, `[class*="..."]`); descendant, child `>` and sibling
`+`/`~` combinators; `:is()`, `:not()`, `:where()`, `:has()`; `:first-child`,
`:last-child`, `:only-child`, `:nth-child()`, `:nth-of-type`,
`:nth-last-child`, `:nth-last-of-type`, `:first-of-type`, `:last-of-type`,
`:only-of-type`; the `.theme-dark`/`.theme-light` roots; `:hover` and `:focus`.
A generated Discord class such as `chat_z_gUBg` or `chat-2ZfjoI` matches by
its stable `chat_` stem.

Pseudo-elements the window can paint: a region's `::before`/`::after`
decoration, `::selection`, `::placeholder`, and
`::-webkit-scrollbar-thumb` (and its `:hover`). Generated `content: "..."` is
not painted.

### Values and functions

Lengths: `px`, `em`, `rem`, `%`, the absolute units (`pt`, `pc`, `in`,
`cm`, `mm`, `Q`) and the viewport family (`vw`, `vh`, `vmin`, `vmax`, and
the `sv*`/`lv*`/`dv*` forms), all settled to pixels at the 1280×720 reference
window; `rem` follows the page's `font-size`. `calc()`, `min()`, `max()`,
`clamp()`, `round()` and `abs()` work in a length, a time and an `opacity`,
and `env()` takes its fallback.

Colours: `#rgb`/`#rrggbb`/`#rrggbbaa`, `rgb()`/`rgba()`, `hsl()`/`hsla()`,
`hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()`, `color(srgb ...)` and
`color(display-p3 ...)`, `contrast-color()`, `light-dark()`, named colours,
`transparent`, `currentColor`, and DiscordRecolor's bare `30,30,30` triples.

Relative colours keep or replace channels: `rgb(from ...)`, `hsl(from ...)`,
`hwb(from ...)`, `lab(from ...)`, `lch(from ...)`, `oklab(from ...)`,
`oklch(from ...)`, `color(from ...)`, including `calc(...)` and the channel
keywords inside them. `calc()`, `min()`, `max()`, `clamp()` and `abs()` work
in colour channels; `var()` resolves with fallbacks; `color-mix()` mixes two
or more colours in sRGB, sRGB-linear, HSL, HWB, Lab, LCh, OKLab and OKLCh.
Gradients (`linear`, `radial`, `conic`) and the usual length and angle units
are understood.

### Cascade

Specificity, source order, `!important` and `@layer` order across the root,
theme-class and media blocks; inheritance; custom properties with `var()`
resolution; CSS nesting (`&`, nested `@media`/`@container`) flattened before
the cascade. The cascade is settled by `style-engine` against ReCore's
synthetic region tree (sidebar, chat, member list, composer, settings, menus,
tooltips, threads, forums, calls, the inbox, profiles, search, polls).

## What cannot be painted

ReCore colours the regions its native window has; it cannot reproduce
Discord's virtualized DOM, per-element keyframe animations, icon masks,
`letter-spacing`, `backdrop-filter`, or the flex/grid layout *inside* a
region. Those rules are read without error and kept in the file. See
[`docs/themes.md`](../../docs/themes.md#what-css-cannot-do-here).

## Development

```bash
cargo test -p recore-css                      # unit + robust (cut/damaged) + golden snapshot
UPDATE_GOLDEN=1 cargo test -p recore-css --test golden   # rewrite the snapshot
cargo clippy -p recore-css --all-targets -- -D warnings
cargo fmt --all -- --check
```

`tests/golden.rs` snapshots the interpreter over the bundled themes in
`themes/`; a change to the reader or a bundled theme shows there as a readable
diff before it is committed.
