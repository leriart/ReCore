# Themes

Settings → Appearance shows the window in one of the themes ReCore comes with,
one of the person's own, or a Discord CSS theme read from a `.theme.css` file.
Each theme is a full set of the colours of `Theme` in
`recore_core::theme` (`crates/recore-core/src/theme.rs`), plus whether it's
dark, which sets the scrollbars' shade and the colours worked out from the
others. The core holds the built-in ones as `BUILT_IN` and reads custom
themes and hex colours; `crates/recore-iced/src/themes.rs` makes the chosen
one the kit's `Theme` (`crates/recore-iced-kit/src/theme.rs`), which every
widget paints with; see [the client's theme](client/window.md#theme). The
choice is kept in `layout.json` as `theme`, the theme's id, and applied
before the window's first frame, and again to a window made anew. With none
kept, the window shows ReCore. An id ReCore doesn't know, or a deleted
theme's, shows ReCore too. The window-control command `theme ID` chooses and
keeps one as Appearance does.

## ReCore and ReCore Paper

The fork's own theme is ReCord's terminal palette: signal orange on warm
charcoal, paper-coloured type, square panels. **ReCore Paper** is the same
palette in daylight. `Appearance → ReCore accent` changes the accent both
derive every shade from; it is kept in `layout.json` as `recore_accent`.
The presets match ReCord's: Amber, Phosphor, Ice, Paper (which switches to
ReCore Paper) and Terminal. ReCord's Pixel Operator and Pixel Operator Mono
are bundled and used by both.

Appearance also carries ReCord's own look options for these two themes, kept
in `layout.json` beside `recore_accent`:

- `recore_scanlines`, `recore_grain` and `recore_vignette` (percentages);
- `recore_beam`, `recore_flicker`, `recore_frame` and `recore_avatar_filter`;
- `recore_background`, `recore_background_dim` and `recore_panel_opacity`;
  `recore_background_filter`, `recore_background_brightness` and
  `recore_background_animate`; the background may be a still picture, an
  animated GIF/APNG/WebP, or a video (MP4, WebM, MOV or MKV), which plays
  through the window's picture players and rests on its first frame while
  the window is unfocused;
- `recore_font` (`pixel`, `mono` or `system`) and `recore_motion` (`full`,
  `subtle` or `off`).

**Background dim** is a palette grade, not a black sheet. Each pixel is
weighted towards the theme's own grounds, inks and signals by its distance
from them, following the Gaussian palette shader in Ambxst's `palette.frag`.
The lookup table is built once per frame at 32 levels per channel, so the
per-pixel work is one index and one mix. The slider (0–100%) mixes the mapped
colour back into the picture; at 0 the picture is untouched. The grade applies
to still frames and animated frames alike, and to CSS themes as well as
ReCore. Moving backgrounds rest while the window is unfocused and play at the
renderer's scale, so a full-window frame is not resampled on the processor on
every tick.

The vignette starts at 0, off: it darkens the window's edges, its top and
bottom most of all.

The window boots with ReCord's signal: a stepped mark, a twelve-step loading
bar, a blinking line and the same screen-in. Menus and dialogs use the
theme's stepped 190 ms motion, a changed room flashes its channel and fades
in over 260 ms, unread rail marks pulse, and speaking rings breathe. These are native iced animations. CSS animations on the page or body are
read as a quiet native ground wash; Discord selectors whose targets do not
exist in a native client are still not painted.

## Ambxst

[`themes/Ambxst.theme.css`](../themes/Ambxst.theme.css) is a Discord theme
inspired by [Axenide's Ambxst](https://github.com/Axenide/Ambxst): its exact
default dark adapter (`#1a1111` ground, `#302625` surface, rose `#ffb3ae`)
and a paper-light counterpart, 16 px rounded panels and Roboto Condensed.
It is a complete theme for Vencord, Vesktop, Equicord, BetterDiscord and
Legcord as well as a CSS theme ReCore reads: the same file carries every
Material role from `Colors.qml`, all nineteen `sr*` surface variants from
Ambxst's default `theme.json`, Discord's semantic tokens, ReCore's `--rl-*`
palette and the native region rules. It turns ReCord's CRT layer off and keeps
the window's rounded, shadowed Material surfaces. Its options cover the
Material roles, surface gradients and borders, opacity and transparency, blur
and shadows, gradients, roundness, type sizes, bar, dock, notch, overview,
context-menu, action-grid and notification geometry. Drop it in the themes
folder like any other `.theme.css` file; it has no remote imports. The
package carries it, so a fresh install has it without downloading anything.
[docs/ambxst.md](ambxst.md) is the source-by-source inventory, including the
nine Ambxst presets and the parts of the Quickshell shell a Discord window
cannot paint.

## Bundled themes

Every package carries the themes in `themes/`: ReCore and ReCore Paper,
Ambxst, Catppuccin Mocha, Tokyo Night, Nord Aurora, Gruvbox Ember and
Solarized Paper. ReCore looks for the package's folder beside the binary, in
the app's `share/recore/themes`, and in a macOS bundle's
`Contents/Resources/themes`; the data folder's own `themes` folder comes first,
so a person's copy wins. No theme needs a remote download to show up.

## CSS themes

ReCore reads the themes Vencord, Vesktop, Equicord, BetterDiscord, Legcord
and ReCord itself load. Put a `.theme.css` file in the folder that
**Appearance → Themes folder** opens, or leave it in the client's own themes
folder; ReCore also looks there:

| Client | Folder |
| --- | --- |
| Vencord | `~/.config/Vencord/themes` (or `%APPDATA%\Vencord\themes`) |
| Vesktop | `~/.config/vesktop/themes` |
| Equicord | `~/.config/Equicord/themes` |
| Equibop | `~/.config/Equibop/themes` |
| BetterDiscord | `~/.config/BetterDiscord/themes` |
| Legcord | `~/.config/Legcord/themes` |
| Dorion | `~/.config/Dorion/themes` |
| GoofCord | `~/.config/GoofCord/themes` |
| Replugged | `~/.config/replugged/themes` |
| ReCore | the data folder's `themes` (the button opens it) |

**Reload CSS** reads them again without restarting. It follows theme packs in
subfolders, leaves hidden folders and dependency trees alone, and reads the
same theme once when it is copied into several client folders. While
Settings is open, a theme saved by an editor outside ReCore reloads itself.
The **Theme URL** field downloads a theme into ReCore's folder, and
**Import file** copies one or more local `.theme.css` files there; the tile
turns up ready to choose. **New CSS theme** opens a starter file in
ReCore's own editor, and the pencil on any CSS tile opens that theme's file
whole: its colours, imports, fonts, background pictures, gradients, shapes
and textures are editable as CSS, and **Save CSS** writes it back and shows
it at once. **Duplicate** saves a renamed copy and opens it, **Export...**
writes the whole source to a file, and **Delete CSS theme** removes the file
after a second press. Escape or **Back to themes** leaves the editor without
writing. A theme keeps its choice and its changed options when its `@name`
changes; ReCore follows the file, not the old id.
The editor's **Choose background...** adds `--rl-background-image` with a
dim and panel opacity, and its texture buttons add ReCord's scanlines,
grain or vignette. Each theme appears as a tile with the CSS badge. A
theme carries:

- The window's panel layout: a theme that names `--gap` or `--spacing`, or a
  `gap` on the page's flex/grid container, gets that space between its
  panels; a flex or grid page can also name the channel list's and the
  member list's widths, which last until their edges are dragged.
  `--roundness` or `--radius-lg` rounds
  their corners, and `--app-frame-background` colours the ground between
  them. Ryoku and Midnight are read this way. A `body { background: transparent }`
  means the panels show through, not that the chat has no ground.
- The colours of Discord's tokens: `--background-primary`, `--background-secondary`,
  `--background-floating`, `--channeltextarea-background`, `--text-normal`,
  `--text-muted`, `--header-primary`, `--brand-experiment`, `--status-danger`,
  `--status-positive`, `--text-link`, the border, interactive and input tokens,
  and the newer `--background-base-*`, `--background-surface-*`,
  `--control-*`, `--text-default` names wherever it uses those instead.
  ClearVision/DiscordRecolor's `--main-color`, `--normal-text` and
  `--muted-text` are read as the accent and ink too.
- Its own variable chain: `var()`, fallbacks, `!important`, `@import` between
  local files, `@media`, `.theme-dark` and `.theme-light` blocks,
  `color-mix()` (two or more colours, in sRGB, linear light, Lab, LCh,
  OKLab or OKLCh), `light-dark()`, `hwb()`, `oklab()` and `oklch()`, `lab()`,
  `lch()` and `color(srgb ...)`/`color(display-p3 ...)`. Hex, `rgb()`,
  `rgba()`, `hsl()`, `hsla()`, `contrast-color()` and named colours are read,
  and DiscordRecolor's bare `30,30,30` RGB triples are understood as colours.
  The relative forms (`rgb(from ...)`, `hsl(from ...)`, `oklab(from ...)`,
  `oklch(from ...)`, `lab(from ...)`, `lch(from ...)`, `hwb(from ...)` and
  `color(from ...)`) keep or replace their source channels, including
  `calc(1 * s)` and the other channel keywords inside `calc()`. CSS nesting
  (`&:hover`, `&::after`, a nested `@media` or `@container`) is flattened
  before the cascade, so the nested rules keep their parent's selector.
  The cascade keeps specificity, source order, `!important` and `@layer`
  order across the root, theme-class and media-query blocks, so a later
  `:root` declaration can win over an earlier `.theme-dark` one, as in a
  browser. A `@layer reset, base, theme;` statement fixes the layer order
  before its blocks, and `@import "x.css" layer(theme) screen` puts the
  import in a layer and applies its media condition. `@supports` conditions
  and the media/supports/layer conditions after an `@import` are applied:
  a declaration is supported when the engine reads it (or the reader
  reduces it to sRGB, as with `color-mix()`), so a theme's fallback takes
  the branch a browser would, and a feature the native window cannot paint
  — a `backdrop-filter`, a mask, a container type or a view transition —
  takes its `not (...)` branch instead. `@property` gives a variable its syntax and initial value, which
  fills the variable when the theme does not set it. An at-rule ReCore does
  not read (`@starting-style`, `@page`, a future one) is kept in the file
  and its rules are not applied, as a browser does, and the note says
  which; a block whose closing brace is missing is read to the end of the
  file.
- Media queries are settled at the window's 1280 × 720 reference size:
  `prefers-color-scheme`, `prefers-reduced-motion`, `hover`, `pointer`,
  `orientation`, `width`/`height` with `min-`/`max-` or range syntax,
  `aspect-ratio`, `resolution`, `color-gamut` and the other desktop
  features. A query whose feature the native window cannot know stays
  applicable, rather than silently dropping a theme's rules.
- `@container style(--motion: off)` and its `not style(...)` form read the
  theme's own settled custom properties, so ReCord's motion, icon and
  low-power containers take the branch matching Appearance's values. A
  container can also carry the theme's whole ground: Ryoku's light template
  sets `--light-mode: on` and puts its palette in
  `@container body style(--light-mode: on)`, which ReCore uses to choose the
  light appearance. A container size query uses the same reference window;
  names, `scroll-state()` and unknown conditions stay applicable.
- Remote files: an `http:` or `https:` `@import` is downloaded, with its own
  relative imports; so are a remote font in `@font-face` and a remote
  picture in `--background-image`. Downloads are kept for ten minutes in
  memory and on disk, so a theme works offline after its first load;
  **Reload CSS** forgets them, so a changed base theme is read at once. A
  file over 8 MB, a failed download or an offline machine keeps the rest of
  the theme and says so in Appearance's note. A theme's remote imports are
  fetched when the theme is chosen, so a folder full of remote themes does
  not stall the first frame.
- Local files: an `@import` and a `@font-face`'s file are read only when
  they are regular files of at most 8 MB, the same limit as a download, so a
  theme that names a device, a pipe or a huge file cannot keep the window
  from opening; that import is left out and Appearance's note says why. A
  sheet that imports itself, directly or through other sheets, is read once
  and the loop is named in the note.
- Its fonts: `@font-face` with a `data:` URI, a file beside the theme or a
  remote URL is loaded into the window's font system, and `--font-primary`
  picks the family.
- Its radius (`--radius-md` or ReCord's `--rl-radius`), a background picture,
  CSS gradient or video (`--rl-background-image`, `--background-image`, or a
  `body` / `body::before` background rule) drawn under the panels, its fit
  (`--rl-background-size`: `cover`, `contain` or `100% 100%`), how opaque the
  panels are over it (`--rl-panel-opacity` or `--rl-surface`), per-section
  opacities (`--rl-rail-opacity`, `--rl-sidebar-opacity`,
  `--rl-chat-opacity`, `--rl-members-opacity`,
  `--rl-composer-opacity`, `--rl-raised-opacity`,
  `--rl-tooltip-opacity`), and ReCord's
  CRT options
  (`--rl-motion`, `--rl-icons`, `--rl-scanline-opacity`,
  `--rl-grain-opacity`, `--rl-vignette-opacity`, `--rl-flicker-duration`,
  `--rl-beam-opacity`, `--rl-frame-opacity`, `--rl-avatar-filter`,
  `--rl-avatar-scanline`). The avatar filter and scanline grade the pictures
  the client draws. A `@keyframes` animation on `:root` or `body` whose steps
  change `background`, `background-color`, `color` or `opacity` is sampled by
  the window and painted as a quiet moving ground; finite animations stop,
  infinite ones repeat, and `alternate`/`reverse` are understood. CSS's
  defaults hold: a name with no duration stays still, and one with no count
  runs once. Other animations remain in the file.

ReCord's own `--rl-*` palette is mapped exactly, so dropping
[`themes/ReCord.theme.css`](../themes/ReCord.theme.css) in gives the built-in
ReCore colours and its four Pixel Operator faces.

### Options the theme exposes

Appearance lists the chosen CSS theme's custom properties as controls, in
collapsible groups, with a filter field for large themes (Catppuccin exposes
more than 250, and DiscordRecolor's imported base more than 700) and
**Show every variable** for the ones ReCore can't place:

- a colour becomes a swatch with an HSV picker and a hex field, keeping an
  eight-digit colour's alpha;
- an opacity, dimming or surface becomes a slider;
- `on`/`off`, `true`/`false` and ReCord's flicker/beam durations become a
  switch;
- a font and a text variable become a field;
- a `url(...)` picture becomes a field with **Choose…** and **Clear**,
  including one the theme embeds as a data URI;
- a BetterDiscord `@var slider` becomes a slider with its min/max/step;
- ReCord's `full`/`subtle`/`off`, background fits and choices listed in a
  comment become buttons.

Each option shows its theme default when it names one, a **Reset this
option** button once it changes, and a badge when it exists only in the dark
or light palette. **Only recommended** hides unannotated variables and
internal aliases; **Only changed** shows just what was overridden;
**Export changes** writes those overrides as a small `.theme.css` file that
can be loaded after the theme elsewhere.

The controls are detected from the values and named from what the theme says
about them. ReCore reads the variable name, an inline comment beside the
declaration, and the heading above it (`/* text colors */`, `/* ACCENT
COLORS */`); both Midnight/system24 and ClearVision give much better labels
this way. It also understands BetterDiscord's `@var` header (`checkbox`,
`text`, `color`, `select` and `slider`), matching names such as
`accentcolor_s` to the `--accentcolor` property. A comment that lists choices (`off: default
position, hide: hide buttons, titlebar: move it`) turns the option into
buttons. This is automatic: a theme needs no ReCore-specific metadata.

ReCord's `--rl-*` names get curated labels and groups. Other themes are
grouped as backgrounds, text, channels, accents, status, borders, interface,
base colours, typography, shape, pictures and effects, with a fallback group
for anything ReCore cannot place. Values changed there are kept per theme in
`layout.json` under `css_options` and the theme is re-read with them, so its
palette and effects change live. **Reset these options** forgets them;
**Show every variable** also lists the properties ReCore cannot place.

For predictable labels, theme authors can put a short description after a
declaration (`--main-color: #2780e6; /* main accent colour */`), group a
block under a heading comment, and use BetterDiscord `@var` metadata for
labels and types. ReCore still reads the CSS when none of those are present.

### Native region selectors

Discord themes often style a region directly, not only its variables:

```css
.theme-dark [class*="sidebarList_"] { background: #14161a; }
[class*="chat_"] [class*="message_"]:hover { background: #1b1d21; }
[class*="membersWrap_"] { border-radius: 0; }
```

ReCore gives every part of its window that Discord themes commonly name a
synthetic node with the class fragments they target, then matches the theme's
rules against those nodes. The nodes are the page and its theme roots; the
guild rail; the channel sidebar and its category headings; the chat, message
list, a message row and the composer with its uploads; the member list, a
member and its group headings; the conversation header; cards, dialogs and
popouts; menus; tooltips; threads; forums and their posts; server and channel
settings pages; stage and call panels; the inbox; profile cards; search
results; and polls. A node also carries the `role` attribute Discord puts on
it, so `[role="dialog"]`, `[role="menu"]` and `[role="tooltip"]` reach the
same regions as `[class*="modal_"]`, `[class*="menu_"]` and
`[class*="tooltip_"]`.

ReCore understands class, id, tag and attribute selectors, attribute
fragments such as `[class*="..."]`, descendant, child and sibling
combinators, `:is()`, `:not()`, `:where()`, `:has()` relative selectors,
`:first-child`, `:last-child`, `:only-child`, `:nth-child()` (and their
`-of-type` and `-last-` forms), the `.theme-dark`/`.theme-light` roots and
`:hover`/`:focus` states. A generated Discord class such as `chat_z_gUBg`
or `chat-2ZfjoI` is matched by its stable `chat_` stem, since Discord
changes the hash and separator between builds. Structural selectors are
settled against the synthetic region tree, so they describe its rows and
panels, not Discord's full virtualized DOM. Inheritance, specificity,
`!important` and source order are applied, and `var()` values are resolved
before they reach the window. A bare tag selector or `*` never styles a region on its own: it
needs a class, an id or an attribute, so a theme's `div { ... }` reset cannot
paint the rail.
Fully transparent backgrounds are left to the region's own colour instead of
erasing it, and a radius on a button, a card or a hover state does not redefine
the whole window's shape.

The mapped properties are:

- `background`, `background-color` and `background-image` as the region's
  ground. A `linear-gradient` on the rail, sidebar or chat is painted as a
  gradient; elsewhere, and for radial or conic gradients, its first opaque
  stop stands in.
- `color` as its text: the sidebar's is its secondary ink, the member list's
  and a category's its muted ink, and every other region's the window's text.
- `border-radius` on the page, rail, sidebar, chat or member list as the
  window's shape.
- `font-family` on the page, which picks the window's font, and `font-size` on
  a category heading, a member-list group heading or the conversation's title,
  each of which keeps its own line's size.
- `text-transform` on those three headings: `uppercase`, `lowercase` and
  `capitalize` change only the heading's words, not the messages below them.
- `border`, `border-*-color` and `outline` as the window's line colour.
  `border-width` and `border-style` are read but not painted: a native line is
  one pixel and has no dash or dot.
- `opacity` on a region, which fades that region's ground to the given alpha.
  The window's text keeps its own ink; see the limit below.
- `box-shadow` on a menu, popout or tooltip as the raised panels' shadow, and
  on a card, dialog or profile popout as cards' shadow. Its colour, x and y
  offsets and blur are kept; a spread or `inset` is not.
- `transition` or `animation` on a menu, popout, dialog, card, settings page or
  profile: its duration (up to two seconds) and `steps(n)` timing set how long
  native popups take to open and how many steps they move in. `ease`, `linear`
  and `cubic-bezier()` make the motion continuous instead of ReCord's four
  steps. A rule on any other region is left in the file.

### Pseudo-elements

A native window has no generated box for a pseudo-element, but a few of them
name something ReCore already paints:

- A region's `::before` or `::after` is a decoration on that region. A full
  overlay's background becomes the region's ground only when it covers the
  region (auto/100% width and height and no generated text); a fixed-size
  sun, icon or panel label does not recolour the panel. A box whose `height`
  or `width` is at most 3 px becomes one of the window's lines; its border
  colour and `box-shadow` are kept, and its own `opacity` fades what it
  paints. `body::before` and `body::after` stay the background picture and
  texture paths described above, not a region.
- `::selection`'s `background` is the colour text is selected in, in the
  timeline and every field.
- `::placeholder`'s `color` is a field's hint text.
- `::-webkit-scrollbar-thumb`'s `background` is the scrollbar's thumb, and
  `::-webkit-scrollbar-thumb:hover` its colour under the pointer. Scrollbar
  width, track and corner rules are kept in the file but not painted.

Generated content, icon masks and a pseudo-element's text (`content: "..."`)
are not painted: ReCore colours the box it would draw, not the glyph in it.

### What CSS cannot do here

A Discord CSS theme is still written for Discord's web window. ReCore matches
the selectors that name its regions and can use a page-level flex or grid to
name the panel widths, but it cannot reproduce arbitrary DOM structure or the
flex or grid layout inside a region, and generated class hashes without a
usable substring have no node to reach. A page's display list is drawn whole
by style-engine's tiny-skia sink, so its background images, linear, radial
and conic gradients, shadows, filters, clips, transforms, opacity and blend
modes all reach the window; the native region widgets above it take their
ground, line, ink and linear gradient directly and do not take element-level
filters. `letter-spacing` is read into the
theme's look, but iced's text widget has no tracking setting, so it is not
painted; a root `font-size` is read the same way while every native widget
keeps its own size. `text-transform` outside the three headings, masks and
generated content are kept in the file without being painted. Structural
and sibling selectors reach the synthetic region tree, whose rows are
representative rather than the full virtualized message and member lists.
A root/body animation of the ground is the exception: its colour and opacity
keyframes become a quiet native wash, and a gradient background is painted at
the background size.
ReCord's scanlines, grain, vignette and corner frame are drawn natively over
the window, including the rolling scan beam and flicker, which rest while the
window is not focused. The note under **Appearance → Theme** says how many
files were read and how many could not be.

## Custom themes

New theme opens a palette editor on a copy of the theme shown; START FROM
swaps it for a copy of any other. The editor lists every colour by its name
(`Theme::NAMES`, with `-` for `_`), with a swatch and its hex, which takes
`#rgb`, `#rrggbb` or `#rrggbbaa` and marks anything else. Each change shows at
once, so the whole window is the preview. Save keeps the theme and chooses
it; Cancel, Escape, leaving Appearance or closing settings shows the theme
from before. A saved theme's card has a pencil that opens it again to rename,
change or delete it; deleting the chosen one shows ReCore. A CSS tile's
pencil opens the full source editor above, which carries pictures and
textures no palette-only theme has. Escape leaves either editor before it
closes Settings. Closing the window to the [tray](tray.md) closes Settings,
so it drops the unsaved edit too.

`layout.json` keeps them in `custom_themes`, in the order Appearance shows
them after the built-in ones:

```json
{
  "id": "custom:1",
  "name": "Plum",
  "base": "recore",
  "dark": true,
  "colours": { "rail": "#111215", "chat": "#2a1a3a", "row-hover": "#1a1b20" }
}
```

`base` is the built-in theme the first copy came from (a copy of a custom
theme takes its base). ReCore writes every colour, but a colour that's
missing, such as one added in a later version, or that isn't a colour, takes
the base's value, and a name ReCore doesn't know is ignored, so themes survive
new colours. Ids are `custom:` and a number, and no built-in id has a colon,
so they can't collide; on reading, ReCore gives a fresh one to any custom
theme whose id isn't of that form or repeats another's. The editor's fields
exist only while it's open; otherwise a custom theme costs its parsed colours
and a card.

| Theme | Id | Dark | Accent | From |
| --- | --- | --- | --- | --- |
| ReCore | `recore` | yes | signal orange | ReCord's terminal palette, the default |
| ReCore Paper | `recore-light` | no | signal orange | ReCord's paper palette |
| Yap | `yap` | yes | marigold | Yapfast's own |
| Bonfire | `bonfire` | yes | blurple | Discord's Dark theme |
| Catppuccin Mocha | `catppuccin-mocha` | yes | mauve | [Catppuccin](https://github.com/catppuccin/catppuccin) |
| Dracula | `dracula` | yes | purple | [Dracula](https://draculatheme.com/contribute) |
| Nord | `nord` | yes | frost nord8 | [Nord](https://www.nordtheme.com/docs/colors-and-palettes) |
| Gruvbox Dark | `gruvbox-dark` | yes | orange | [Gruvbox](https://github.com/morhetz/gruvbox) |
| Tokyo Night | `tokyo-night` | yes | blue | [Tokyo Night](https://github.com/tokyo-night/tokyo-night-vscode-theme), Night style |
| Catppuccin Latte | `catppuccin-latte` | no | mauve | [Catppuccin](https://github.com/catppuccin/catppuccin) |
| Rosé Pine Dawn | `rose-pine-dawn` | no | rose | [Rosé Pine](https://rosepinetheme.com/palette/) |
| Sakura | `sakura` | no | blossom pink | Yapfast's own |

The contrast test in `crates/recore-core/src/theme.rs` holds each palette to
what the mapping below promises: 4.5:1 for text, secondary, muted, on-accent
and on-danger, and 2.5:1 for faint. `on_accent` and `on_danger` take the
darkest or lightest ink that reads on the accent or red; a palette's own
secondary or muted ink may stay under 4.5:1 until its ramp is reworked.

The palettes of Catppuccin, Dracula, Nord, Gruvbox, Tokyo Night and Rosé Pine
are published under the MIT licence; thanks to their authors. For Tokyo
Night, the steps between its grounds and text (bg_highlight, fg_gutter, dark3,
dark5) are named as folke's tokyonight.nvim names them. ReCore takes their
colours, not their names for UI parts, so each mapping is ReCore's:

- The ground steps go rail, sidebar, chat from the deepest to the one text
  sits on, with raised surfaces (popovers, reactions, stage tiles) and the
  composer one step off the chat. Where a palette has no shade below its
  background (Nord's nord0, Tokyo Night's bg_dark), the rail, panel and edge
  darken it, and hover and some row grounds are halfway mixes of two palette
  colours.
- Text, secondary, muted and faint take the palette's text ramp; faint keeps
  at least 2.5:1 against the sidebar.
- Content on the accent and on red (on-accent, on-danger) takes the palette's
  darkest or lightest colour, whichever reads, and black or white when its
  ramp stops short; on pastel accents, as in Catppuccin and Tokyo Night,
  that's dark.
- Light themes wash hovers with a dark overlay (black, or the palette's text
  colour) instead of white, sweep skeletons with a white band, and draw the
  tooltip light, since its text is the theme's text colour.

Bonfire maps Discord's Dark theme from the CSS variables of its web client
(`.theme-dark` with `.theme-darker`, which is what its settings call Dark;
`.theme-dark` alone is Ash, `.theme-midnight` Onyx), read from the public
bundle: the rail and sidebar are `--background-base-lowest` (#121214), the chat
`--background-base-lower` (#1a1a1e), the account panel and composer
`--background-base-low` (#202024), raised surfaces and the tooltip
`--background-surface-high` (#242429), text `--text-default`, muted
`--text-muted`, links `--text-link`, the accent
`--control-primary-background-default` (blurple, #5865f2, hover #4452bb, white
on it), red `--control-critical-primary-background-default` (#d22d39), live
`--status-positive`, and hovers, selections, lines and secondary buttons the
`--interactive-background-*`, `--border-subtle` and
`--control-secondary-background-*` washes laid over their grounds. It carries
no Discord name or logo.

A few things that Discord's client draws off its palette, Bonfire draws in
the inks measured from it (`Theme::bonfire` in the core, `bonfire` on the
kit's theme): a mention's text, the channel list's names and glyphs, a
message's time, the header's search box and the Jump to present pill. A
custom theme made from Bonfire keeps them while its link colour is
Bonfire's; every other theme draws those from its own colours.

Discord role colours stay as servers set them, and some read faintly on a
light theme; ReCore doesn't adjust them yet.

## Reading a theme's files

The interpreter lives in `crates/recore-css`. `recore_css::load(path)` reads a
file and returns a `ThemeFile` with the resolved `Palette`, `Look`, fonts,
warnings and the metadata from the theme's `/** @name ... */` header.
`load_with_imports` and `load_str_with_imports` take a fetch callback for a
theme's remote files, and `scan_with_imports` does so while walking folders;
`scan` and `load` are the same without remote access. `theme_folders` names
the common clients' folders. `Options` chooses the dark or light set, forces
an accent, or overrides a custom property.

The reader keeps its own tolerant loader for files, fonts, pictures and the
comments Appearance turns into controls. It flattens nested rules, settles
media queries and `@layer` order while reading; once the root variables are
known it removes the `@container style(...)` rules whose condition does not
match before the semantic pass sees them. Generated Discord class names are
reduced to their stable stems before the engine matches a region. The semantic pass in
`crates/recore-css/src/semantic.rs` then hands the settled rules to
[`style-engine`](https://crates.io/crates/style-engine): its CSS parser,
selector matcher and cascade settle inheritance, custom properties, `:is()`,
`:where()`, `:has()`, `@layer`, `color-mix()` and the rest against the
synthetic regions. The computed values become the window's `Palette`,
`NativeStyle`, selection, placeholder and scrollbar inks and the `::before` /
`::after` decorations. Root variables are seeded from the reader's winning
values because style-engine 1.0.0 does not carry `!important` on custom
properties; colour functions it does not evaluate are reduced to sRGB first.
A page whose `display` is flex or grid runs style-engine's taffy pass once
to name the panel widths; a normal page skips it. The chosen theme's page
paint list is drawn once by the window's own software canvas and shown under
the native panels, so CSS gradients, shadows and `::before` decorations
reach the window without a GPU pass of its own. Fonts and text still come
from iced's cosmic-text; style-engine's parley feature is not on this path.

`scan` reads every local file once, hashes it to skip copies, and parses up
to four at once; the parsed themes are kept by path and content, so the
second read a start makes for the window reuses them instead of parsing the
folder again. The semantic pass reads only the custom properties a rule or a
palette role names, and only creates hover/focus variants for regions a theme
styles under those states. A chosen theme with no changed options and no
remote files is not read again; one naming remote imports, fonts or pictures
is, to fetch them. `NativeStyle` filters the declarations to the properties a
native region actually paints before resolving values or sorting the
cascade, and `Vars::resolve` borrows a value that names no `var()`.

A theme is data, never code. A damaged or hostile file is read as far as it
makes sense and the rest is left out: unterminated blocks, strings and
comments, NUL bytes, invalid numbers, reference loops and a variable that
names itself twice at every level (which would be 2^n substitutions) all
end without a panic, a stack overflow or a run that never returns. A value
that would grow past 1 MiB or read more than 4096 variables is treated as
missing. A number that reaches the window is held to the range it draws in:
a heading's size to 6–200 px, a radius and the space between panels to
0–64 px, letter spacing to −8–32 px, a shadow's offsets and blur to ±512 px.
A size of zero or less is not a length CSS accepts, so the window keeps its
own. `crates/recore-css/tests/robust.rs` cuts every bundled theme at
120 places and damages each with random fragments, and checks the hostile
inputs by name.

```bash
cargo test -p recore-css
```

To see what ReCore read from a theme by hand, and every rule or declaration
it left out with the line it sits on:

```bash
cargo run -p recore-css --example read -- themes/Ambxst.theme.css
```

The same detail is available to code: `Options { diagnostics: true, .. }`
fills `ThemeFile::diagnostics` with a `Diagnostic` per dropped or skipped
declaration, selector or at-rule (with `line` and `column`), plus the
reader's own notes. It is off by default, since scanning a folder pays for a
second parse.
