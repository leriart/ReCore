//! The semantic layer: what a theme's CSS means for ReCore's window.
//!
//! `style-engine` parses the stylesheet, matches its selectors against the
//! synthetic tree in [`crate::native`], and settles inheritance, custom
//! properties and the cascade. This module reads the computed values back as
//! ReCore's own data: the window's [`Palette`], the per-region
//! [`NativeStyle`], the selected-text and placeholder colours, and the
//! decoration a region's `::before` or `::after` draws.
//!
//! The work happens once, when a theme is loaded. The window still paints
//! with iced. The engine's optional taffy/parley pass is for the phase that
//! paints a CSS layout; it is not on the theme-load path. `em`, `rem`,
//! `calc()` and viewport units still settle through the engine's computed
//! lengths, rather than being guessed from the declaration text.

use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap},
};

use style_engine::{
    ComputedStyle, StyleEngine, compute_node,
    css::{
        property::{
            BackgroundImage, BorderStyle, ContentPiece, ContentValue, DeclValue, Display,
            FamilyName, Gradient as EngineGradient, PropertyId, TextTransformKind,
        },
        stylesheet::{MediaEnv, parse_stylesheet},
        value::{ColorValue, LengthPercentage, ResolveCtx},
    },
    tree::{NodeId, NodeState, PseudoWhich, StyleNode as EngineNode, StyleTree},
};

use crate::{
    Gradient, GradientKind, GradientStop, PageLayout, Palette, Rgba,
    css::{self, Stylesheet, Variables},
    native::{self, NODES, NativeStyle, Role, Shadow, TextTransform},
};

/// The viewport a theme is settled at. CSS themes are written for a desktop
/// window; `vw`/`vh`, percentages and flex/grid tracks resolve against this.
pub(crate) const VIEWPORT: (f32, f32) = crate::css::VIEWPORT;

/// A theme's semantic values, ready for the window.
pub(crate) struct Computed {
    pub palette: Palette,
    pub native: NativeStyle,
    /// The panel widths a page-level flex or grid settles at.
    pub page_layout: Option<PageLayout>,
    /// The gap a flex or grid page names, which the window uses between its
    /// panels when no `--gap`-style variable does.
    pub gap: Option<f32>,
}

/// The keys this module gives the style tree. The host key is also how the
/// engine reports a node's computed style after a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Key {
    Html,
    Body,
    Region(Role),
    Hover(Role),
    Focus(Role),
    /// A collapsed region whose Discord wrappers are one native surface
    /// (`.message` and `.messageContent_`).
    Nested(Role),
}

/// Reads a theme's semantic values with `style-engine`.
///
/// `style-engine` settles the theme's parser, selector matching, cascade,
/// inheritance and custom properties against ReCore's synthetic tree. The
/// same tree is also used for the pseudo-element pass, so a theme is parsed
/// once and each region is computed once.
pub(crate) fn compute(sheet: &Stylesheet, vars: &css::Vars, dark: bool) -> Option<Computed> {
    let used = used_vars(sheet, vars, dark);
    let active = active_roles(sheet, dark);
    let states = state_roles(sheet, dark);
    let css = to_css(sheet, vars, dark, &used);
    let engine_sheet = parse_stylesheet(&css);
    let environment = MediaEnv {
        viewport_w: VIEWPORT.0,
        viewport_h: VIEWPORT.1,
        dark,
        ..MediaEnv::default()
    };
    let (mut tree, ids) = build_tree(vars, dark, &used, &active, &states);
    let styles = compute_styles(&tree, &ids, &engine_sheet, &environment, &active, &states);
    let pseudos = pseudo_styles(
        &mut tree,
        &ids,
        &styles,
        &engine_sheet,
        &environment,
        &active,
    );
    let root = styles.get(&Key::Html)?;
    let page = styles.get(&Key::Body);
    let semantic_vars = SemanticVars::new(root, page);
    let palette = crate::theme::palette(&semantic_vars, dark);
    let native = native_from_styles(&styles, &pseudos, sheet, dark);
    let gap = page
        .or(Some(root))
        .and_then(page_gap)
        .filter(|gap| *gap > 0.0);
    let page_layout = page
        .or(Some(root))
        .filter(|style| matches!(style.display(), Display::Flex | Display::Grid))
        .and_then(|_| layout(&css, vars, dark, &active));
    Some(Computed {
        palette,
        native,
        page_layout,
        gap,
    })
}

/// The custom properties the CSS engine settled, with ReCore's forced values
/// taking precedence, and the colours already parsed by name.
struct SemanticVars<'a> {
    style: &'a ComputedStyle,
    page: Option<&'a ComputedStyle>,
    colors: RefCell<HashMap<(String, bool), Option<Rgba>>>,
}

impl<'a> SemanticVars<'a> {
    fn new(style: &'a ComputedStyle, page: Option<&'a ComputedStyle>) -> Self {
        Self {
            style,
            page,
            colors: RefCell::new(HashMap::new()),
        }
    }
}

impl Variables for SemanticVars<'_> {
    fn get(&self, name: &str) -> Option<&str> {
        self.style
            .custom_value(name)
            .or_else(|| self.page.and_then(|page| page.custom_value(name)))
    }

    fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    fn resolve<'a>(&'a self, value: &'a str) -> Option<Cow<'a, str>> {
        Some(Cow::Borrowed(value))
    }

    fn color(&self, name: &str, light: bool) -> Option<Rgba> {
        let key = (name.to_owned(), light);
        if let Some(color) = self.colors.borrow().get(&key) {
            return *color;
        }
        let color = self
            .get(name)
            .and_then(|value| crate::color::parse(value, light));
        self.colors.borrow_mut().insert(key, color);
        color
    }
}

/// Serializes the reader's stylesheet for `style-engine`. The reader has
/// already resolved `@import`, `@media (prefers-color-scheme)`, `@supports`
/// and `@layer` order; this keeps the selectors, declarations and layer
/// membership so the engine can apply the cascade and inheritance.
fn to_css(sheet: &Stylesheet, vars: &css::Vars, dark: bool, used: &BTreeSet<String>) -> String {
    let mut out = String::new();
    let layers: Vec<&str> = sheet
        .layers
        .iter()
        .map(String::as_str)
        .filter(|name| !name.is_empty())
        .collect();
    if !layers.is_empty() {
        out.push_str("@layer ");
        for (index, name) in layers.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(name);
        }
        out.push_str(";\n");
    }
    for rule in &sheet.rules {
        if rule
            .color_scheme
            .is_some_and(|color_scheme| color_scheme != dark)
        {
            continue;
        }
        let layer = rule
            .layer
            .and_then(|rank| sheet.layers.get(rank as usize))
            .map(String::as_str)
            .filter(|name| !name.is_empty());
        if let Some(name) = layer {
            out.push_str("@layer ");
            out.push_str(name);
            out.push_str(" {\n");
        }
        let selectors: Vec<String> = rule
            .selectors
            .iter()
            .filter(|selector| region_selector(selector))
            .map(|selector| normalize_selector(selector).into_owned())
            .collect();
        if selectors.is_empty() {
            if layer.is_some() {
                out.push_str("}\n");
            }
            continue;
        }
        out.push_str(&selectors.join(", "));
        out.push_str(" {\n");
        for declaration in &rule.declarations {
            // A data URI or another huge asset is for the picture loader, not
            // the window's colours; carrying it through every node's
            // inherited custom properties would dominate the load.
            if declaration.name.starts_with("--") && !used.contains(&declaration.name) {
                continue;
            }
            let value = colors(&declaration.value, vars, !dark);
            if value.len() > 4096 || value.contains("data:") {
                continue;
            }
            out.push_str(&declaration.name);
            out.push(':');
            out.push_str(&value);
            if declaration.important {
                out.push_str(" !important");
            }
            out.push_str(";\n");
        }
        out.push_str("}\n");
        if layer.is_some() {
            out.push_str("}\n");
        }
    }
    out
}

/// Whether a selector names a native region. A bare tag or `*` is too broad:
/// `div { ... }` would paint the rail in a browser-free window. The root tags
/// and the window-wide pseudo-elements are the exceptions, as they are in the
/// lightweight matcher.
/// Rewrites a generated class such as `chat_z_gUBg` to the stable prefix
/// `chat_` that ReCore's synthetic nodes carry. Discord hashes the suffix
/// differently per build, so exact generated names otherwise never match.
fn normalize_selector(selector: &str) -> Cow<'_, str> {
    let mut out = String::with_capacity(selector.len());
    let mut changed = false;
    let mut index = 0;
    let mut quote: Option<char> = None;
    let bytes = selector.as_bytes();
    while index < bytes.len() {
        let character = selector[index..].chars().next().unwrap_or_default();
        if let Some(active) = quote {
            if character == active {
                quote = None;
            }
            out.push(character);
            index += character.len_utf8();
            continue;
        }
        match character {
            '"' | '\'' => {
                quote = Some(character);
                out.push(character);
                index += 1;
            }
            '.' => {
                out.push('.');
                let start = index + 1;
                index = start;
                while bytes
                    .get(index)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                {
                    index += 1;
                }
                let class = &selector[start..index];
                let known = NODES
                    .iter()
                    .flat_map(|node| node.classes.iter())
                    .filter(|known| native::class_prefix_matches(known, class))
                    .max_by_key(|known| known.len());
                match known {
                    Some(known) => {
                        out.push_str(known);
                        changed |= !class.eq_ignore_ascii_case(known);
                    }
                    None => out.push_str(class),
                }
            }
            _ => {
                out.push(character);
                index += character.len_utf8();
            }
        }
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(selector)
    }
}

fn region_selector(selector: &str) -> bool {
    let lower = selector.to_ascii_lowercase();
    if lower.contains("::selection")
        || lower.contains("::placeholder")
        || lower.contains("::-webkit-scrollbar")
    {
        return true;
    }
    let base = selector.split("::").next().unwrap_or(selector);
    let compounds = css::selector_compounds(base);
    if compounds.iter().any(|compound| {
        compound.contains('.')
            || compound.contains('#')
            || compound.contains('[')
            || compound.starts_with(":root")
            || compound.starts_with(":where(html")
            || compound.starts_with(":where(:root")
            || compound.starts_with(":is(html")
            || compound.starts_with(":is(:root")
    }) {
        return true;
    }
    let Some(last) = compounds.last() else {
        return false;
    };
    let tag = last
        .trim()
        .trim_start_matches(':')
        .split(|character: char| !character.is_ascii_alphanumeric())
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(tag.as_str(), "html" | "body" | "main" | "aside" | "header")
}

/// The roles a theme's selector text can reach. A region no selector names
/// keeps its palette colour, so it needs no node: this is the largest saving
/// on themes that style only a few of Discord's regions.
fn active_roles(sheet: &Stylesheet, dark: bool) -> BTreeSet<Role> {
    let mut roles = BTreeSet::new();
    roles.insert(Role::Root);
    for rule in &sheet.rules {
        if rule
            .color_scheme
            .is_some_and(|color_scheme| color_scheme != dark)
        {
            continue;
        }
        for selector in &rule.selectors {
            // Structural and sibling selectors depend on the nodes around
            // the one they name. Keep the whole synthetic tree for them,
            // rather than a pruned set where a region's real position and
            // siblings have disappeared.
            if selector.contains(":nth-")
                || selector.contains(":first-")
                || selector.contains(":last-")
                || selector.contains(":only-")
                || selector.contains(" > ")
                || selector.contains(" + ")
                || selector.contains(" ~ ")
            {
                roles.extend(NODES.iter().map(|node| node.role));
                break;
            }
            for node in NODES.iter().filter(|node| node.role != Role::Root) {
                if node.classes.iter().any(|class| selector.contains(class))
                    || node.aria_role.is_some_and(|aria| selector.contains(aria))
                {
                    roles.insert(node.role);
                }
            }
        }
    }
    roles
}

/// The roles a theme styles under a pointer or focus state.
fn state_roles(sheet: &Stylesheet, dark: bool) -> BTreeSet<Role> {
    let mut roles = BTreeSet::new();
    for rule in &sheet.rules {
        if rule
            .color_scheme
            .is_some_and(|color_scheme| color_scheme != dark)
        {
            continue;
        }
        for selector in &rule.selectors {
            if let Some(role) = native::state_role(selector, dark) {
                roles.insert(role);
            }
        }
    }
    roles
}

/// The custom properties the semantic layer reads: the ones a declaration
/// names with `var()` and the names a palette or layout value knows, plus
/// ReCord's `--rl-*` family. Everything else is an Appearance option or an
/// asset, and would only be copied into every node's inherited map.
fn used_vars(sheet: &Stylesheet, vars: &css::Vars, dark: bool) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    for rule in &sheet.rules {
        if rule
            .color_scheme
            .is_some_and(|color_scheme| color_scheme != dark)
        {
            continue;
        }
        for declaration in &rule.declarations {
            collect_vars(&declaration.value, &mut used);
        }
    }
    for (name, _) in vars.iter() {
        if palette_name(name) {
            used.insert(name.to_owned());
        }
    }
    used
}

fn collect_vars(value: &str, used: &mut BTreeSet<String>) {
    let mut rest = value;
    while let Some(start) = css::find_ignore_case(rest, "var(") {
        let after = &rest[start + 4..];
        let Some(end) = css::matching_paren(after) else {
            break;
        };
        let inside = &after[..end];
        let name = inside.split(',').next().unwrap_or("").trim();
        if name.starts_with("--") {
            used.insert(name.to_owned());
        }
        rest = &after[end + 1..];
    }
}

fn palette_name(name: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "--rl-",
        "--background",
        "--chat-",
        "--text",
        "--normal-",
        "--muted-",
        "--main-",
        "--header",
        "--brand",
        "--control",
        "--interactive",
        "--status",
        "--green",
        "--red",
        "--yellow",
        "--white",
        "--primary",
        "--link",
        "--accent",
        "--danger",
        "--success",
        "--warning",
        "--online",
        "--idle",
        "--blurple",
        "--panel",
        "--border",
        "--modal",
        "--card",
        "--input",
        "--channel",
        "--button",
        "--message",
        "--icon",
        "--spine",
        "--avatar",
        "--app-frame",
        "--tooltip",
        "--font",
        "--text-size",
        "--letter-spacing",
        "--gap",
        "--spacing",
        "--radius",
        "--roundness",
    ];
    PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

/// The reader's settled root variables as inline declarations.
fn inline_vars(
    vars: &css::Vars,
    dark: bool,
    used: &BTreeSet<String>,
) -> style_engine::css::decl::DeclarationBlock {
    let mut text = String::new();
    for (name, value) in vars.iter() {
        if !used.contains(name) {
            continue;
        }
        let value = colors(value, vars, !dark);
        if value.len() > 4096 || value.contains("data:") {
            continue;
        }
        text.push_str(name);
        text.push(':');
        text.push_str(&value);
        text.push_str(";\n");
    }
    style_engine::css::decl::parse_inline_declarations(&text).0
}

/// Replaces the colour functions `style-engine` 1.0.0 cannot evaluate
/// (`color-mix()`, `light-dark()` and the Lab family) with the sRGB a native
/// colour needs. A whole colour value becomes one hex; a colour inside a
/// shorthand (`1px solid color-mix(...)`) is replaced in place.
fn colors<'a>(value: &'a str, vars: &'a css::Vars, light: bool) -> Cow<'a, str> {
    let resolved = vars.resolve(value).unwrap_or(Cow::Borrowed(value));
    if let Some(color) = crate::color::parse(resolved.as_ref(), light) {
        return Cow::Owned(color.hex());
    }
    match replace_colors(resolved.as_ref(), light) {
        Some(replaced) => Cow::Owned(replaced),
        None => resolved,
    }
}

fn replace_colors(value: &str, light: bool) -> Option<String> {
    const FUNCTIONS: &[&str] = &[
        "color-mix(",
        "light-dark(",
        "contrast-color(",
        "rgb(from ",
        "rgba(from ",
        "hsl(from ",
        "hsla(from ",
        "hwb(from ",
        "oklab(from ",
        "oklch(from ",
        "lab(from ",
        "lch(from ",
        "color(from ",
        "oklab(",
        "oklch(",
        "lab(",
        "lch(",
        "hwb(",
    ];
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    let mut changed = false;
    while let Some((index, name)) = FUNCTIONS
        .iter()
        .filter_map(|name| css::find_ignore_case(rest, name).map(|index| (index, *name)))
        .min_by_key(|(index, _)| *index)
    {
        out.push_str(&rest[..index]);
        let start = index + name.len();
        let Some(end) = css::matching_paren(&rest[start..]) else {
            out.push_str(&rest[index..]);
            rest = "";
            break;
        };
        let function = &rest[index..start + end + 1];
        if let Some(color) = crate::color::parse(function, light) {
            out.push_str(&color.hex());
            changed = true;
        } else {
            out.push_str(function);
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    changed.then_some(out)
}

/// Builds the page and the native regions, plus the hover and focus variants
/// the window's overlay reads.
fn build_tree(
    vars: &css::Vars,
    dark: bool,
    used: &BTreeSet<String>,
    active: &BTreeSet<Role>,
    states: &BTreeSet<Role>,
) -> (StyleTree, BTreeMap<Key, NodeId>) {
    let mut tree = StyleTree::new();
    let super_root = tree.root();
    let mut ids = BTreeMap::new();
    let root_classes: Vec<&str> = NODES[0]
        .classes
        .iter()
        .copied()
        .filter(|class| !class.starts_with("theme-"))
        .collect();
    let mut html = node("html", &root_classes, NODES[0].id, None, dark);
    // `style-engine` 1.0.0 does not carry `!important` on custom properties.
    // The reader already settled that cascade, so its winning root values are
    // pushed in as inline declarations: the engine still inherits and
    // substitutes them, and a theme's `!important` keeps working.
    html.declarations = inline_vars(vars, dark, used);
    let html_id = tree.insert_child(super_root, html);
    ids.insert(Key::Html, html_id);
    let body = node("body", &root_classes, NODES[0].id, None, dark);
    let body_id = tree.insert_child(html_id, body);
    ids.insert(Key::Body, body_id);
    for node_spec in NODES
        .iter()
        .filter(|node| node.role != Role::Root && active.contains(&node.role))
    {
        let parent = match node_spec.parent {
            Some(index)
                if NODES[index].role != Role::Root && active.contains(&NODES[index].role) =>
            {
                ids[&Key::Region(NODES[index].role)]
            }
            _ => body_id,
        };
        let tag = node_spec.tags.first().copied().unwrap_or("div");
        let normal = node(
            tag,
            node_spec.classes,
            node_spec.id,
            node_spec.aria_role,
            dark,
        );
        let id = tree.insert_child(parent, normal);
        ids.insert(Key::Region(node_spec.role), id);
        if states.contains(&node_spec.role) {
            let mut hover = node(
                tag,
                node_spec.classes,
                node_spec.id,
                node_spec.aria_role,
                dark,
            );
            hover.state = NodeState::HOVER | NodeState::ACTIVE;
            let hover_id = tree.insert_child(parent, hover);
            ids.insert(Key::Hover(node_spec.role), hover_id);
            let mut focus = node(
                tag,
                node_spec.classes,
                node_spec.id,
                node_spec.aria_role,
                dark,
            );
            focus.state = NodeState::FOCUS;
            let focus_id = tree.insert_child(parent, focus);
            ids.insert(Key::Focus(node_spec.role), focus_id);
        }
        if node_spec.role == Role::Message {
            // Discord's `.messageContent_` is inside `.message_`; a native
            // row is one surface, so mirror the same classes one level down
            // and read the inner winning rule.
            let nested = node(
                tag,
                node_spec.classes,
                node_spec.id,
                node_spec.aria_role,
                dark,
            );
            let nested_id = tree.insert_child(id, nested);
            ids.insert(Key::Nested(node_spec.role), nested_id);
        }
    }
    (tree, ids)
}

/// Settles every node's style, parent before child.
fn compute_styles(
    tree: &StyleTree,
    ids: &BTreeMap<Key, NodeId>,
    sheet: &style_engine::css::stylesheet::Stylesheet,
    environment: &MediaEnv,
    active: &BTreeSet<Role>,
    states: &BTreeSet<Role>,
) -> BTreeMap<Key, ComputedStyle> {
    let mut styles = BTreeMap::new();
    let html = compute_node(tree, ids[&Key::Html], sheet, environment, None);
    let body = compute_node(tree, ids[&Key::Body], sheet, environment, Some(&html));
    styles.insert(Key::Html, html);
    styles.insert(Key::Body, body);
    for node in NODES
        .iter()
        .filter(|node| node.role != Role::Root && active.contains(&node.role))
    {
        let parent_key = match node.parent {
            Some(index)
                if NODES[index].role != Role::Root && active.contains(&NODES[index].role) =>
            {
                Key::Region(NODES[index].role)
            }
            _ => Key::Body,
        };
        let parent = styles.get(&parent_key);
        let region = compute_node(
            tree,
            ids[&Key::Region(node.role)],
            sheet,
            environment,
            parent,
        );
        let hover = states.contains(&node.role).then(|| {
            compute_node(
                tree,
                ids[&Key::Hover(node.role)],
                sheet,
                environment,
                parent,
            )
        });
        let focus = states.contains(&node.role).then(|| {
            compute_node(
                tree,
                ids[&Key::Focus(node.role)],
                sheet,
                environment,
                parent,
            )
        });
        if node.role == Role::Message {
            let nested = compute_node(
                tree,
                ids[&Key::Nested(node.role)],
                sheet,
                environment,
                Some(&region),
            );
            styles.insert(Key::Nested(node.role), nested);
        }
        styles.insert(Key::Region(node.role), region);
        if let Some(hover) = hover {
            styles.insert(Key::Hover(node.role), hover);
        }
        if let Some(focus) = focus {
            styles.insert(Key::Focus(node.role), focus);
        }
    }
    styles
}

/// One synthetic node with the attributes Discord themes match.
fn node(
    tag: &str,
    classes: &[&str],
    id: Option<&str>,
    aria_role: Option<&str>,
    dark: bool,
) -> EngineNode {
    let mut list: Vec<String> = classes.iter().map(|class| (*class).to_owned()).collect();
    let theme = if dark { "theme-dark" } else { "theme-light" };
    if !list.iter().any(|class| class == theme) {
        list.push(theme.to_owned());
    }
    if dark {
        for class in ["theme-darker", "theme-midnight", "theme-ash", "theme-onxy"] {
            if !list.iter().any(|known| known == class) {
                list.push(class.to_owned());
            }
        }
    }
    let mut attrs = BTreeMap::new();
    attrs.insert("class".to_owned(), list.join(" "));
    if let Some(id) = id {
        attrs.insert("id".to_owned(), id.to_owned());
    }
    if let Some(role) = aria_role {
        attrs.insert("role".to_owned(), role.to_owned());
    }
    attrs.insert(
        "data-theme".to_owned(),
        if dark { "dark" } else { "light" }.to_owned(),
    );
    EngineNode {
        name: Some(tag.to_owned()),
        id: id.map(str::to_owned),
        classes: list.into_iter().collect(),
        attrs,
        ..Default::default()
    }
}

/// The window values won by the regions' selectors.
fn native_from_styles(
    styles: &BTreeMap<Key, ComputedStyle>,
    pseudos: &[(Role, bool, PseudoStyle)],
    sheet: &Stylesheet,
    dark: bool,
) -> NativeStyle {
    let mut style = NativeStyle::default();
    // A page that never names `color` must keep the palette's ink, not the
    // engine's default black. Many wallpaper and effect themes leave the
    // page's colour to Discord.
    let root_text = sheet.rules.iter().any(|rule| {
        rule.declarations
            .iter()
            .any(|declaration| declaration.name.eq_ignore_ascii_case("color"))
            && rule
                .selectors
                .iter()
                .any(|selector| css::root_selector(selector, dark).is_some())
    });
    // `:root` rules land on `html`; direct page rules land on `body`. The
    // window has one page surface, so read both: html first, body over it.
    let html = styles.get(&Key::Html);
    let page = styles.get(&Key::Body);
    if let Some(root) = html {
        apply(&mut style, Role::Root, false, root, None, dark, root_text);
    }
    if let Some(page) = page {
        apply(&mut style, Role::Root, false, page, html, dark, root_text);
    }
    for node in NODES.iter().filter(|node| node.role != Role::Root) {
        let parent = match node.parent {
            Some(index) if NODES[index].role != Role::Root => {
                styles.get(&Key::Region(NODES[index].role))
            }
            _ => page,
        };
        let region = styles.get(&Key::Region(node.role));
        if let Some(region) = region {
            apply(
                &mut style, node.role, false, region, parent, dark, root_text,
            );
        }
        if node.role == Role::Message
            && let Some(nested) = styles.get(&Key::Nested(node.role))
        {
            apply(
                &mut style,
                node.role,
                false,
                nested,
                region.or(parent),
                dark,
                root_text,
            );
        }
        if let Some(hover) = styles.get(&Key::Hover(node.role)) {
            apply(
                &mut style,
                node.role,
                true,
                hover,
                region.or(parent),
                dark,
                root_text,
            );
        } else if let Some(focus) = styles.get(&Key::Focus(node.role)) {
            apply(
                &mut style,
                node.role,
                true,
                focus,
                region.or(parent),
                dark,
                root_text,
            );
        }
    }
    let (selection, placeholder) = channels(sheet, dark);
    style.selection = selection;
    style.placeholder = placeholder;
    let (scrollbar, scrollbar_hover) = scrollbar(sheet, dark);
    style.scrollbar = scrollbar;
    style.scrollbar_hover = scrollbar_hover;
    apply_pseudos(&mut style, pseudos);
    style
}

/// Maps one computed region onto the window's fields.
fn apply(
    style: &mut NativeStyle,
    role: Role,
    hover: bool,
    computed: &ComputedStyle,
    parent: Option<&ComputedStyle>,
    dark: bool,
    root_text: bool,
) {
    let background = color(computed.background_color(), computed, dark);
    let mut gradient = None;
    for layer in computed.background_layers() {
        if let BackgroundImage::Gradient(found) = layer.image {
            gradient = engine_gradient(&found, computed, dark);
            break;
        }
    }
    if let Some(background) = background.filter(|color| color.a > 0) {
        native::set_background(style, role, hover, background);
    }
    if let Some(gradient) = gradient {
        if !hover && let Some(first) = gradient.stops.iter().find(|stop| stop.color.a > 0) {
            native::set_background(style, role, false, first.color);
        }
        native::set_gradient(style, role, hover, gradient);
    } else if background.is_some_and(|color| color.a > 0) {
        native::clear_gradient(style, role, hover);
    }

    if (role != Role::Root || root_text)
        && let Some(text) = color(computed.color(), computed, dark)
        // The engine starts every page at black. A theme that never names a
        // page colour leaves its regions to Discord's palette, not to that
        // default; only an explicit black survives.
        && (root_text || text != Rgba::rgb(0, 0, 0))
    {
        let inherited = parent.and_then(|parent| color(parent.color(), parent, dark));
        if inherited != Some(text) {
            native::set_text(style, role, text);
        }
    }

    if !hover
        && matches!(
            role,
            Role::Root | Role::Rail | Role::Sidebar | Role::Chat | Role::Members
        )
        && let Some(radius) = length(computed.value(PropertyId::BorderTopLeftRadius), computed)
        && radius > 0.0
        && let Some(radius) = crate::css::bounds::within(radius, crate::css::bounds::RADIUS)
    {
        style.radius = Some(radius);
    }

    if matches!(role, Role::Root)
        && let Some(family) = computed
            .font_family()
            .0
            .iter()
            .find_map(|family| match family {
                FamilyName::Named(name) => Some(name.clone()),
                _ => None,
            })
    {
        style.font_family = Some(family);
    }

    // A zero or negative size is not a length CSS accepts; an absurd one is
    // held to the largest size the window draws a heading at.
    if let Some(size) = length(computed.value(PropertyId::FontSize), computed)
        && size > 0.0
        && let Some(size) = crate::css::bounds::within(size, crate::css::bounds::TEXT)
        && (size - 16.0).abs() > 0.01
    {
        match role {
            Role::Root => style.font_size = Some(size),
            Role::Category => style.category_size = Some(size),
            Role::MembersGroup => style.member_heading_size = Some(size),
            Role::Header => style.header_size = Some(size),
            _ => {}
        }
    }

    if matches!(role, Role::Root)
        && computed.value(PropertyId::LetterSpacing).is_some()
        && let Some(spacing) = length(computed.value(PropertyId::LetterSpacing), computed)
        && let Some(spacing) = crate::css::bounds::within(spacing, crate::css::bounds::TRACKING)
    {
        style.letter_spacing = Some(spacing);
    }

    {
        let transform = match computed.text_transform() {
            TextTransformKind::Uppercase => Some(TextTransform::Uppercase),
            TextTransformKind::Lowercase => Some(TextTransform::Lowercase),
            TextTransformKind::Capitalize => Some(TextTransform::Capitalize),
            _ => None,
        };
        if let Some(transform) = transform {
            match role {
                Role::Category => style.category_transform = Some(transform),
                Role::MembersGroup => style.member_heading_transform = Some(transform),
                Role::Header => style.header_transform = Some(transform),
                _ => {}
            }
        }
    }

    if !hover
        && (computed.opacity() - 1.0).abs() > 0.001
        && let Some(slot) = native::opacity_slot(style, role)
        && let Some(opacity) = crate::css::bounds::opacity(computed.opacity())
    {
        *slot = Some(opacity);
    }

    if let Some(shadow) = shadow(computed, dark) {
        native::set_shadow(style, role, shadow);
    }

    if let Some(line) = line(computed, dark) {
        style.line = Some(line);
    }
    if let Some(accent) = color_prop(computed, PropertyId::AccentColor, dark)
        .or_else(|| color_prop(computed, PropertyId::CaretColor, dark))
    {
        style.accent = Some(accent);
    }

    if matches!(
        role,
        Role::Card | Role::Menu | Role::ServerSettings | Role::Profile
    ) {
        motion(style, computed);
    }
}

/// The colours, shadow and shape a region's `::before` or `::after` draws.
fn apply_pseudos(style: &mut NativeStyle, pseudos: &[(Role, bool, PseudoStyle)]) {
    for (role, _after, pseudo) in pseudos {
        let role = *role;
        let fade = pseudo.opacity;
        if let Some(background) = pseudo.background.filter(|color| color.a > 0) {
            let background = native::fade(background, fade);
            if pseudo.thin {
                style.line = Some(background);
            } else if pseudo.overlay {
                native::set_background(style, role, false, background);
            }
        }
        if pseudo.overlay
            && let Some(gradient) = &pseudo.gradient
        {
            native::set_gradient(style, role, false, gradient.clone());
        }
        if let Some(line) = pseudo.line {
            style.line = Some(native::fade(line, fade));
        }
        if let Some(shadow) = pseudo.shadow {
            native::set_shadow(style, role, shadow);
        }
    }
}

struct PseudoStyle {
    background: Option<Rgba>,
    gradient: Option<Gradient>,
    line: Option<Rgba>,
    shadow: Option<Shadow>,
    opacity: Option<f32>,
    thin: bool,
    /// A box that covers its region. A small sun, icon or badge pseudo is a
    /// decoration, not that region's ground.
    overlay: bool,
}

/// The `::before`/`::after` channels need the originating element, so insert
/// the pseudo nodes under each region and settle them against the region's
/// computed style. The engine materializes its own pseudo nodes for painting,
/// but a native surface reads them here.
fn pseudo_styles(
    tree: &mut StyleTree,
    ids: &BTreeMap<Key, NodeId>,
    styles: &BTreeMap<Key, ComputedStyle>,
    sheet: &style_engine::css::stylesheet::Stylesheet,
    environment: &MediaEnv,
    active: &BTreeSet<Role>,
) -> Vec<(Role, bool, PseudoStyle)> {
    if !sheet.has_pseudo_rules {
        return Vec::new();
    }
    let mut out = Vec::new();
    for node in NODES
        .iter()
        .filter(|node| node.role != Role::Root && active.contains(&node.role))
    {
        let Some(parent) = styles.get(&Key::Region(node.role)) else {
            continue;
        };
        let parent_id = ids[&Key::Region(node.role)];
        for (after, which) in [(false, PseudoWhich::Before), (true, PseudoWhich::After)] {
            let pseudo = tree.insert_child(
                parent_id,
                EngineNode {
                    pseudo: Some(which),
                    ..Default::default()
                },
            );
            let computed = compute_node(tree, pseudo, sheet, environment, Some(parent));
            out.push((node.role, after, pseudo_value(&computed, environment.dark)));
        }
    }
    out
}

fn pseudo_value(computed: &ComputedStyle, dark: bool) -> PseudoStyle {
    let mut gradient = None;
    for layer in computed.background_layers() {
        if let BackgroundImage::Gradient(found) = layer.image {
            gradient = engine_gradient(&found, computed, dark);
            break;
        }
    }
    let thin = [
        PropertyId::Height,
        PropertyId::Width,
        PropertyId::MaxHeight,
        PropertyId::MaxWidth,
    ]
    .into_iter()
    .filter_map(|property| length(computed.value(property), computed))
    .any(|length| length > 0.0 && length <= 3.0);
    PseudoStyle {
        background: color(computed.background_color(), computed, dark),
        gradient,
        line: line(computed, dark),
        shadow: shadow(computed, dark),
        opacity: computed
            .value(PropertyId::Opacity)
            .map(|_| computed.opacity()),
        thin,
        overlay: size_is_full(computed.value(PropertyId::Width))
            && size_is_full(computed.value(PropertyId::Height))
            && !pseudo_text(computed),
    }
}

/// `::selection` and `::placeholder`, which the engine handles per node but
/// a theme usually writes without a subject. Read the rules directly so a
/// window-wide `::selection { background: ... }` reaches every field.
fn channels(sheet: &Stylesheet, dark: bool) -> (Option<Rgba>, Option<Rgba>) {
    let mut selection = None;
    let mut placeholder = None;
    for rule in &sheet.rules {
        if rule
            .color_scheme
            .is_some_and(|color_scheme| color_scheme != dark)
        {
            continue;
        }
        let selection_rule = rule.selectors.iter().any(|selector| {
            let lower = selector.to_ascii_lowercase();
            lower.contains("::selection") || lower.contains(":selection")
        });
        let placeholder_rule = rule.selectors.iter().any(|selector| {
            let lower = selector.to_ascii_lowercase();
            lower.contains("::placeholder") || lower.contains(":placeholder")
        });
        for declaration in &rule.declarations {
            let property = declaration.name.trim().to_ascii_lowercase();
            if selection_rule
                && matches!(
                    property.as_str(),
                    "background" | "background-color" | "background-image"
                )
                && let Some(color) =
                    native::background_color(&declaration.value, !dark).filter(|color| color.a > 0)
            {
                selection = Some(color);
            }
            if placeholder_rule
                && property == "color"
                && let Some(color) =
                    native::color_in(&declaration.value, !dark).filter(|color| color.a > 0)
            {
                placeholder = Some(color);
            }
        }
    }
    (selection, placeholder)
}

/// A `::-webkit-scrollbar-thumb` colour, which the engine's selector grammar
/// does not accept. The reader kept the rule, so read its colours directly.
fn scrollbar(sheet: &Stylesheet, dark: bool) -> (Option<Rgba>, Option<Rgba>) {
    let mut normal = None;
    let mut hover = None;
    for rule in &sheet.rules {
        if rule
            .color_scheme
            .is_some_and(|color_scheme| color_scheme != dark)
        {
            continue;
        }
        let mut thumb = false;
        let mut under_pointer = false;
        for selector in &rule.selectors {
            let lower = selector.to_ascii_lowercase();
            if lower.contains("::-webkit-scrollbar-thumb") {
                thumb = true;
                if lower.contains(":hover") {
                    under_pointer = true;
                }
            }
        }
        if !thumb {
            continue;
        }
        for declaration in &rule.declarations {
            if !matches!(
                declaration.name.to_ascii_lowercase().as_str(),
                "background" | "background-color" | "background-image"
            ) {
                continue;
            }
            if let Some(color) =
                native::background_color(&declaration.value, !dark).filter(|color| color.a > 0)
            {
                if under_pointer {
                    hover = Some(color);
                } else {
                    normal = Some(color);
                }
            }
        }
    }
    (normal, hover)
}

fn color(value: ColorValue, computed: &ComputedStyle, dark: bool) -> Option<Rgba> {
    match value.pick_scheme(dark) {
        ColorValue::Absolute(color) => {
            let [r, g, b, a] = color.components;
            Some(Rgba::rgba(channel(r), channel(g), channel(b), channel(a)))
        }
        ColorValue::CurrentColor => match computed.color().pick_scheme(dark) {
            ColorValue::Absolute(color) => {
                let [r, g, b, a] = color.components;
                Some(Rgba::rgba(channel(r), channel(g), channel(b), channel(a)))
            }
            _ => None,
        },
        _ => None,
    }
}

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn color_prop(computed: &ComputedStyle, property: PropertyId, dark: bool) -> Option<Rgba> {
    match computed.value(property)? {
        DeclValue::Color(found) => color(*found, computed, dark),
        _ => None,
    }
}

fn line(computed: &ComputedStyle, dark: bool) -> Option<Rgba> {
    let mut found = None;
    for (color_property, style_property) in [
        (PropertyId::BorderTopColor, Some(PropertyId::BorderTopStyle)),
        (
            PropertyId::BorderRightColor,
            Some(PropertyId::BorderRightStyle),
        ),
        (
            PropertyId::BorderBottomColor,
            Some(PropertyId::BorderBottomStyle),
        ),
        (
            PropertyId::BorderLeftColor,
            Some(PropertyId::BorderLeftStyle),
        ),
        (PropertyId::OutlineColor, Some(PropertyId::OutlineStyle)),
    ] {
        let Some(DeclValue::Color(value)) = computed.value(color_property) else {
            continue;
        };
        let styled = style_property.is_some_and(|property| match computed.value(property) {
            Some(DeclValue::BorderStyle(style)) => *style != BorderStyle::None,
            Some(DeclValue::OutlineStyle(style)) => {
                !matches!(style, style_engine::css::property::OutlineStyle::None)
            }
            _ => false,
        });
        if matches!(value, ColorValue::CurrentColor) && !styled {
            continue;
        }
        if let Some(color) = color(*value, computed, dark).filter(|color| color.a > 0) {
            found = Some(color);
        }
    }
    found
}

/// Whether a pseudo-element generates text. A panel label's gradient is not
/// the panel's ground; a full-size `content: ""` box still is.
fn pseudo_text(computed: &ComputedStyle) -> bool {
    match computed.value(PropertyId::Content) {
        Some(DeclValue::Content(ContentValue::Str(text))) => !text.is_empty(),
        Some(DeclValue::Content(ContentValue::Seq(parts))) => parts
            .iter()
            .any(|part| !matches!(part, ContentPiece::Str(text) if text.is_empty())),
        _ => false,
    }
}

/// Whether a pseudo-element's width or height covers its parent: absent,
/// `auto`, or a percentage of the whole. A fixed 24 px icon does not.
fn size_is_full(value: Option<&DeclValue>) -> bool {
    match value {
        None | Some(DeclValue::WideKeyword(_)) => true,
        Some(DeclValue::LenAuto(None)) => true,
        Some(DeclValue::LenAuto(Some(LengthPercentage::Percent(percent)))) => *percent >= 0.999,
        Some(DeclValue::Len(LengthPercentage::Percent(percent))) => *percent >= 0.999,
        _ => false,
    }
}

fn length(value: Option<&DeclValue>, computed: &ComputedStyle) -> Option<f32> {
    let length = match value? {
        DeclValue::Len(length) => length,
        DeclValue::LenAuto(Some(length)) => length,
        DeclValue::Radius(horizontal, _) => horizontal,
        _ => return None,
    };
    resolve_length(length, computed, 0.0)
}

fn resolve_length(length: &LengthPercentage, computed: &ComputedStyle, basis: f32) -> Option<f32> {
    let context = ResolveCtx {
        em: computed.font_size_px(),
        rem: 16.0,
        viewport_w: VIEWPORT.0,
        viewport_h: VIEWPORT.1,
        cq_w: VIEWPORT.0,
        cq_h: VIEWPORT.1,
        ch_per_em: 0.5,
        ex_per_em: 0.5,
        ic_per_em: 1.0,
    };
    length.resolve(&context, basis)
}

fn shadow(computed: &ComputedStyle, dark: bool) -> Option<Shadow> {
    let Some(DeclValue::BoxShadows(list)) = computed.value(PropertyId::BoxShadow) else {
        return None;
    };
    list.iter().find(|shadow| !shadow.inset).map(|shadow| {
        let color = color(shadow.color, computed, dark).unwrap_or(Rgba::rgba(0, 0, 0, 0x80));
        let shadow_bound = |value: f32| {
            crate::css::bounds::within(value, crate::css::bounds::SHADOW).unwrap_or(0.0)
        };
        let offset_x = shadow_bound(resolve_length(&shadow.offset_x, computed, 0.0).unwrap_or(0.0));
        let offset_y = shadow_bound(resolve_length(&shadow.offset_y, computed, 0.0).unwrap_or(0.0));
        let blur = shadow_bound(
            resolve_length(&shadow.blur, computed, 0.0)
                .unwrap_or(0.0)
                .max(0.0),
        );
        Shadow {
            color,
            offset_x,
            offset_y,
            blur,
        }
    })
}

fn motion(style: &mut NativeStyle, computed: &ComputedStyle) {
    let transition = match computed.value(PropertyId::TransitionDuration) {
        Some(DeclValue::TransitionTime(list)) => list
            .0
            .first()
            .copied()
            .filter(|seconds| *seconds > 0.0 && *seconds <= 2.0)
            .map(|seconds| {
                let timing = match computed.value(PropertyId::TransitionTimingFunction) {
                    Some(DeclValue::TransitionTiming(list)) => list.0.first().copied(),
                    _ => None,
                };
                (seconds, timing)
            }),
        _ => None,
    };
    let animation = match computed.value(PropertyId::AnimationDuration) {
        Some(DeclValue::AnimationTimeList(list)) => list
            .first()
            .copied()
            .filter(|seconds| *seconds > 0.0 && *seconds <= 2.0)
            .map(|seconds| {
                let timing = match computed.value(PropertyId::AnimationTimingFunction) {
                    Some(DeclValue::AnimationTimingList(list)) => list.first().copied(),
                    _ => None,
                };
                (seconds, timing)
            }),
        _ => None,
    };
    let Some((seconds, timing)) = animation.or(transition) else {
        return;
    };
    style.motion_ms = Some((seconds * 1000.0).round() as u32);
    if let Some(timing) = timing {
        match timing {
            style_engine::css::property::TimingFn::Steps(steps, _) => {
                style.motion_steps = Some(steps.clamp(1, 16));
            }
            _ => style.motion_steps = Some(0),
        }
    }
}

/// Runs style-engine's taffy layout for a page that turns its regions into
/// flex or grid items. Only themes whose page asks for a layout pay for it.
fn layout(css: &str, vars: &css::Vars, dark: bool, active: &BTreeSet<Role>) -> Option<PageLayout> {
    let mut engine = StyleEngine::<Key>::new();
    engine.set_environment(MediaEnv {
        viewport_w: VIEWPORT.0,
        viewport_h: VIEWPORT.1,
        dark,
        ..MediaEnv::default()
    });
    let _ = engine.set_stylesheet(css);
    let mut html = node("html", &root_classes(), NODES[0].id, None, dark);
    html.declarations = inline_vars(vars, dark, &used_vars_from_css(css));
    engine.insert(None, Key::Html, html).ok()?;
    engine
        .insert(
            Some(Key::Html),
            Key::Body,
            node("body", &root_classes(), NODES[0].id, None, dark),
        )
        .ok()?;
    for node_spec in NODES
        .iter()
        .filter(|node| node.role != Role::Root && active.contains(&node.role))
    {
        let parent = match node_spec.parent {
            Some(index)
                if NODES[index].role != Role::Root && active.contains(&NODES[index].role) =>
            {
                Key::Region(NODES[index].role)
            }
            _ => Key::Body,
        };
        let tag = node_spec.tags.first().copied().unwrap_or("div");
        engine
            .insert(
                Some(parent),
                Key::Region(node_spec.role),
                node(
                    tag,
                    node_spec.classes,
                    node_spec.id,
                    node_spec.aria_role,
                    dark,
                ),
            )
            .ok()?;
    }
    let frame = engine.frame(VIEWPORT, 1.0, 0.0);
    let width = |role: Role| {
        frame
            .find(Key::Region(role))
            .map(|entry| entry.width)
            .filter(|width| *width > 0.0)
    };
    Some(PageLayout {
        rail: width(Role::Rail)?,
        sidebar: width(Role::Sidebar).unwrap_or(0.0),
        chat: width(Role::Chat).unwrap_or(0.0),
        members: width(Role::Members).unwrap_or(0.0),
    })
}

/// The chosen page's paint list, settled at [`VIEWPORT`]. The window scales
/// it to its own size; iced paints the operations it can.
pub(crate) fn paint(
    sheet: &Stylesheet,
    vars: &css::Vars,
    dark: bool,
) -> Option<style_engine::DisplayList> {
    let used = used_vars(sheet, vars, dark);
    let css = to_css(sheet, vars, dark, &used);
    let mut engine = StyleEngine::<Key>::new();
    engine.set_environment(MediaEnv {
        viewport_w: VIEWPORT.0,
        viewport_h: VIEWPORT.1,
        dark,
        ..MediaEnv::default()
    });
    let _ = engine.set_stylesheet(&css);
    let (extra, _) =
        style_engine::css::decl::parse_inline_declarations("margin:0;width:100vw;height:100vh;");
    let mut html = node("html", &root_classes(), NODES[0].id, None, dark);
    html.declarations = inline_vars(vars, dark, &used);
    html.declarations.decls.extend(extra.decls.iter().cloned());
    let mut body = node("body", &root_classes(), NODES[0].id, None, dark);
    body.declarations = html.declarations.clone();
    engine.insert(None, Key::Html, html).ok()?;
    engine.insert(Some(Key::Html), Key::Body, body).ok()?;
    let frame = engine.frame(VIEWPORT, 1.0, 0.0);
    (!frame.paint.ops.is_empty()).then_some(frame.paint)
}

fn root_classes() -> Vec<&'static str> {
    NODES[0]
        .classes
        .iter()
        .copied()
        .filter(|class| !class.starts_with("theme-"))
        .collect()
}

/// The custom properties a serialized sheet names. The layout pass has the
/// sheet text, not the reader's `Stylesheet`, and only needs the variables
/// that text actually references plus the palette names.
fn used_vars_from_css(css: &str) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    collect_vars(css, &mut used);
    used
}

fn page_gap(style: &ComputedStyle) -> Option<f32> {
    [PropertyId::Gap, PropertyId::RowGap, PropertyId::ColumnGap]
        .into_iter()
        .filter_map(|property| length(style.value(property), style))
        .find(|gap| *gap > 0.0)
        .and_then(|gap| crate::css::bounds::within(gap, crate::css::bounds::GAP))
}

fn engine_gradient(
    gradient: &EngineGradient,
    computed: &ComputedStyle,
    dark: bool,
) -> Option<Gradient> {
    let kind = match &gradient.kind {
        style_engine::css::property::GradientKind::Linear(angle) => GradientKind::Linear(angle.0),
        style_engine::css::property::GradientKind::Radial(_) => GradientKind::Radial,
        style_engine::css::property::GradientKind::Conic(_) => GradientKind::Conic,
        _ => return None,
    };
    let stops = gradient
        .stops
        .iter()
        .filter_map(|stop| {
            let color = color(stop.color, computed, dark)?;
            let position = stop.position.as_ref().and_then(|position| match position {
                LengthPercentage::Percent(value) => Some(*value),
                _ => None,
            });
            Some(GradientStop { color, position })
        })
        .collect::<Vec<_>>();
    (stops.len() >= 2).then_some(Gradient { kind, stops })
}
