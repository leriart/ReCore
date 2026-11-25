# Changelog

ReCore's current release is shown in Settings > About. The release workflow
uses the same section for the release page, and publishes this file with the
release.

## 0.5.7 - 2026-11-24

### Themes

- CSS themes read nested rules (`&:hover`, a nested `@media` or `@container`),
  `@layer` order, range media queries, `@supports`, `@container style(...)`
  and `@property` initial values. Selectors match child and sibling
  combinators and the structural pseudo-classes, and Discord's generated
  class names match by their stem; a `::before` or `::after` recolours a
  panel only when it covers it.
- Colours accept `calc()`, `min()`, `max()` and `clamp()` in their channels,
  the relative `rgb(from ...)`, `hsl(from ...)` and `oklch(from ...)` forms,
  `color-mix()` in six colour spaces and `contrast-color()`; ClearVision's
  main, text and muted variables map to ReCore's inks.
- Absolute, viewport and `rem` units and `calc()`, `round()` and `abs()`
  settle before the engine; theme numbers stay inside the ranges the window
  can draw.
- `@import` and `@font-face` files are bounded, regular files only and their
  cycles stop; a font face without a family or source, a quoted animation
  name and a value-less media query are reported, and an unclosed block at
  the file's end no longer loses the rest of a theme.
- Attribute names and tags are lowered as HTML and CSS write them, escaped
  quotes no longer end an attribute early, and `env()` is read only where it
  is a value's function, never inside a string, a comment or a URL.
- Per-section opacity follows `--rl-rail-opacity`, `--rl-sidebar-opacity`,
  `--rl-chat-opacity`, `--rl-members-opacity`, `--rl-composer-opacity`,
  `--rl-raised-opacity` and `--rl-tooltip-opacity`.
- Appearance gains **Import file**, and the CSS editor **Duplicate**,
  **Export...** and **Delete CSS theme** with a second press to delete. A
  theme keeps its choice and changed options when its `@name` changes.
- Every declaration a theme loses is reported with its severity, file, line
  and column. The reader's public API is documented, and a 20-fixture
  conformance suite, a golden snapshot of the bundled themes and a bounded
  robustness suite guard it.

### Discord

- A channel that leaves out **Attach Files** no longer offers a clip or
  takes a dropped or pasted file, and the refusal says why. A channel
  without **Embed Links** still sends a link, just without its card, as
  Discord does.
- **Reconnect** now restarts Discord as well as Matrix: one button serves
  either or both networks, and a Discord account that is not linked is not
  asked to reconnect.
- **Cancel** appears where it can work: a failed send, a queued send or a
  Matrix transaction, never a Discord upload already in flight.

### Interface

- A short DM list no longer goes blank after a long server list was
  scrolled: the sidebar hears the viewport it is about to draw, even when
  its contents fit, and returns to the top.
- Settings choices whose words do not fit wrap onto even lines instead of
  cutting them, at any window width.
- The picture viewer and a full-screen video follow the layout iced draws,
  including a display-scale or monitor change that sends no resize event.
  A full-screen video asks for frames at the fitted size of the new room.
- A search result's right-click menu copies its message link, and with
  Developer Mode its message and author ids, even when the result is not in
  the timeline.
- **Help** opens a card of the keyboard's shortcuts by section; Escape, its
  close button or a click outside closes it, and it scrolls in a short
  window.
- Waiting files say what kind they are beside their names (PDF, ZIP, IMG,
  VID, AUD, TXT, CODE, FILE), with a page icon carrying the same mark. Six
  new 16 px icons join the kit and two 24 px glyphs return to its 2 px
  stroke.

### Fixes

- A notification's toast keeps its six-second timer when the window is
  unfocused and the taskbar flash is on, instead of staying until clicked.
- A theme with a missing import, an oversized file or an unreadable font is
  reported and the rest of the theme still loads, without a panic.

### Accessibility

- A form's error uses `danger_text` rather than the red ground colour, and
  seven built-in palette inks that fell below WCAG are corrected. All
  twelve palettes now read at 4.5:1 for text, secondary, muted, words on the
  accent and on red, and at 2.5:1 for faint, with a test over the colour
  maths.

## 0.5.6 - 2026-11-23

### Themes

- CSS pages are painted with `style-engine`'s complete tiny-skia sink, so
  radial and conic gradients, shadows, borders, clips, transforms, opacity,
  filters and blend modes reach the window instead of being discarded.
- Appearance can create a CSS theme from a complete starter file and edit any
  CSS theme's source. Background pictures and ReCord's scanlines, grain and
  vignette can be added from the editor, and **Save CSS** shows the result at
  once.
- Escape and **Back to themes** leave the theme editor before they close
  Settings, so entering and leaving the menu no longer loses the page under
  the editor.
- Reading a CSS theme's source keeps its comments, imports, fonts and effects;
  a file the reader rejects stays in the editor with its error instead of being
  written.

### Calls

- This account's speaking ring follows the processed microphone's voice rather
  than the transmit gate: it lights when speech is heard, stays through short
  pauses and turns off 220 ms after the voice stops. Push to talk no longer
  keeps it lit during a silent key press, and muting hides it.

### Fixes

- The page paint image now covers every operation the CSS engine produces,
  including groups and clips that used to disappear.
- Theme errors and unreadable files are reported without closing the editor or
  the settings page.

## 0.5.5 - 2026-11-21

### Themes

- CSS themes are interpreted with `style-engine`: selectors, the cascade,
  inheritance, custom properties and state variants now power theme matching.
- A CSS page can lay the window out with Flexbox or Grid. Its rail, sidebar,
  chat and member-list widths, gaps and paint reach the native panels.
- More Discord regions and decorations map to ReCore's own widgets, and the
  rail keeps its own colour beside the sidebar.
- Reading and matching CSS themes and painting their gradients are faster;
  gradients with unpositioned stops stay finite.

### Discord

- Server settings gained Discovery, privacy controls, AutoMod rules,
  integrations, webhooks, soundboard sounds, stage topics and speakers, and
  voice-channel status.
- Joining a server asks its onboarding questions.
- Several accounts can be linked at once, and sign-in with email is supported.
- Group DMs can be renamed and given a new icon.
- Announcements can be crossposted, and super reactions are listed separately.
- Message media, moderation, server folders and the inbox are complete.
- Fenced code is highlighted by its language, and custom emoji can be copied
  as `:name:`.

### Performance and fixes

- Video decoding reuses frames, uses more decode threads and holds its lock
  for less time.
- Cached reactions are read before super reactions.
- Embed icons and thread locks follow Discord again.
- The email login divider spans its row, and the Matrix crosspost command
  keeps its match.
- The preview draws its sample faces and stops failed downloads from retrying.

### Release

- The changelog now appears in Settings > About and on each release page, and
  ships with the Linux, Windows and macOS packages.
