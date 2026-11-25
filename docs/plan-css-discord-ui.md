# The work: the CSS interpreter, Discord's gaps, the window's looks

A map of what the audit found and what is being done about it, wave by wave.
Each wave ends in a state that builds and passes its tests.

## Wave 1 — the CSS interpreter: correctness and robustness

The interpreter is the heart of the theming. It is also where the audit found
the sharpest edges. Four slaves, each owning one file, so their work never
meets in the same place.

### CSS-1 — `src/css.rs` (the parser)

| Defect | Where | What to do |
| --- | --- | --- |
| Nested rules multiply: 2 selectors per level ⇒ 2^depth rules. A 600-byte file hangs the reader. | `nested_css` (791), `parse_block` (697) | Cap the rules a sheet may hold and the fan-out of one expansion; warn when the cap is hit. |
| `!important` does not invert the layer order. A layered `!important` must beat an unlayered one. | `Vars::insert_weighted` (2650) | Compare on an importance-aware weight. |
| Specificity is a flat 100/10/1 count that mis-reads `:is()`, `:where()`, `:not()` and attribute values. | `specificity` (2975) | Compute a real `(a, b, c)` triple: `:is()`/`:not()` take their most specific argument, `:where()` takes none, an attribute's value is ignored. |
| `@PROPERTY` is matched case-insensitively but its name is read case-sensitively, so the whole rule is dropped. | 624 | Read the name the way every other at-rule is read. |
| `Vars` keeps four parallel maps keyed by the same name; every write costs four operations and four key clones. | 2612 | One map of records. |
| The resolution cache can hold a gigabyte and is cleared on every write. | 2618, 2718 | Bound it by bytes; stop clearing it on every write. |
| `split_last_declaration` re-scans the prefix for every nested rule. | 763 | Track the last `;` in the one pass. |
| `collect_notes` clones the property name for every declaration. | 2023 | Clone only when the name changes. |

### CSS-2 — `src/color.rs` (the colours)

| Defect | Where | What to do |
| --- | --- | --- |
| `color-mix()` averages hue as unit vectors, so unequal weights give the wrong hue. | 1424 | Interpolate along the shorter arc with the weights. |
| An unknown `color()` space is read as sRGB, so `color(rec2020 ...)` is silently wrong. | 972 | Read the spaces CSS names; reject the ones it does not. |
| `color(from ...)` substitutes the source's sRGB channels for the target space's. | 1088 | Substitute the source in the target space. |
| A NaN or infinite channel becomes black. | 515, 859, 1308 | Reject a non-finite value. |
| The colour parser recurses once per unary sign: a megabyte of `+` aborts the process. | 639 | A depth budget and an iterative sign, like the length parser has. |
| The gamma thresholds are never pinned by a test. | 859, 868 | Reference values: `#ff0000` → `oklch(0.6279 0.2577 29.23)`, `#00ff00` → `oklch(0.8664 0.2948 142.5)`, `#ff0000` → `lab(53.2408 80.0925 67.2032)`, `color-mix(in oklab, #000, #fff)` → `rgb(99,99,99)`. |

### CSS-3 — `src/semantic.rs` + `src/native.rs` (the cascade)

| Defect | Where | What to do |
| --- | --- | --- |
| The appearance class is on every node, so `.theme-dark { background }` repaints every region in the window. | `node` (1354) | The class belongs to the page only. |
| One-slot fields — the text ink, the line, the accent, the radius — are won by the last region in the list, not by the cascade. | `apply` (1483) | Keep the best candidate, with its specificity and order. |
| `::selection`, `::placeholder` and the scrollbar thumb skip the cascade and never resolve `var()`. | 1733, 1777 | Run them through the same cascade. |
| `text-transform: none` cannot turn off an earlier `uppercase`. | 1574 | `none` is an opinion, not silence. |
| Every rule is matched against every node. | 1299 | Match only the rules that name an active region. |
| Two pseudo nodes are built for every region, whether or not a theme writes one. | 1663 | Build them only where a rule wants them. |
| `native.rs` keeps a second, hand-written matcher that nothing reaches, and it disagrees with the live one. | 605, 1444 | Delete it. |

### CSS-4 — `src/options.rs` (the theme's own switches)

| Defect | Where | What to do |
| --- | --- | --- |
| `merge` compares every option against every known option: quadratic in the number of properties. | 209 | A set of the names, looked up in one step. |
| `sort` allocates inside the comparator. | 219 | Sort on a key made once per option. |
| `@property`'s `syntax` is a hint, never a check; `inherits` is thrown away. | 340, css.rs 626 | Use the syntax to reject a value the theme got wrong; keep `inherits`. |
| A number only counts as a number with `px`, `rem`, `em`, `ms` or `s`. | 738 | Every unit the reader knows. |

## Wave 2 — the CSS interpreter: what it cannot yet read

The gaps the audit named, in the order they cost a theme the most.

1. `@property` that checks its values, and `inherits`.
2. `calc()`'s missing maths: `mod()`, `rem()`, `sin()`, `cos()`, `tan()`, `asin()`, `acos()`, `atan()`, `atan2()`, `pow()`, `sqrt()`, `hypot()`, `log()`, `exp()`.
3. The colour spaces CSS names and ReCore did not: `xyz`, `xyz-d50`, `xyz-d65`, `rec2020`, `a98-rgb`, `prophoto-rgb`.
4. `color-mix()`'s hue methods: `longer hue`, `increasing hue`, `decreasing hue`.
5. Gradients that say more than a colour and a percentage: lengths, two-position stops, radial and conic geometry, the repeating forms.
6. `attr()`, `color-contrast()`, `image()`, `cross-fade()`.
7. The units CSS names: `cap`, `lh`, `rlh`, `fr`.
8. `@container` with a name, and `scroll-state()`.
9. `@scope` that limits its rules to a region, rather than reading them unscoped.
10. `var()` in the pseudo-elements the reader reads by hand.

## Wave 3 — Discord's gaps

The client's own audit, from the fake's own events down.

1. The gateway events the fake already sends and the client ignores: pins, the automod rules, the integrations, the invites, the webhooks, the scheduled event's users.
2. `CHANNEL_PINS_UPDATE`, and pins that page.
3. Scheduled events: the list, and creating, editing and deleting one.
4. A server's full member list, paged.
5. Forum posts sorted and filtered by their tags.
6. Discord's own account settings: the theme, the language, the notifications, the keybinds.
7. AutoMod beyond keywords: word lists, patterns, link blocking, raid protection.
8. Server templates, and Community Onboarding on the owner's side.
9. A webhook read and edited.
10. Messages sent later.
11. A smaller `READY`, with the guild versions.
12. `GET /gateway`, and an explicit intent mask.
13. The client's own seams: a timeout on every request, bounded queues, the unknown events logged.

## Wave 4 — the window's looks

1. The theme's own font size and letter spacing, which the window drops today.
2. The hardcoded greys that break a light theme: the member heading, the member hover, the server tag chip, the code block's colours, every scrim.
3. A spinner, for every place that says "loading".
4. The header's name and topic, elided rather than cut.
5. The theme's own radius, honoured by the surfaces Discord rounds.
6. The emoji, GIF and sticker grids, on the keyboard.
7. The tabs, on the arrow keys.
8. The tooltips, kept inside the window.
9. The scrims, from the theme.
10. The code block's colours, from the theme, and more languages.
11. One spoiler cover, from the theme.
12. The missing empty and error states.
13. A member list popout.
14. The rows that measure themselves every frame.
15. The picker's width, kept inside the window.
16. The keys that never reach the picker.
17. The search that lands its answers in the wrong list.
18. The toast, the menu and the chip, in the kit.
19. The mention and reply tints, from the theme.
20. The panel's edges, faded with the panel.
