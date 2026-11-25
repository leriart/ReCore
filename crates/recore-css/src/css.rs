//! A small CSS reader: comments and strings, rules and their declarations,
//! `@import`, `@font-face`, and the at-rules that wrap variable declarations
//! (`@media`, `@supports`, `@layer`, `@container`). It reads what themes use,
//! not every corner of the language, and keeps what it doesn't know as text.

use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    fmt, fs,
    path::{Path, PathBuf},
};

use crate::color::{Rgba, parse as parse_color};

/// The window a theme's media and container size queries are settled at, in
/// CSS pixels. It is the same reference the layout pass uses.
pub(crate) const VIEWPORT: (f32, f32) = (1280.0, 720.0);

/// The ranges a theme's numbers are held to before the window draws them.
///
/// A theme is data and may say anything; a negative font size or a radius of
/// a billion pixels would reach iced's layout as a crash or an empty window.
/// CSS asks a browser to clamp some of these at computed-value time and to
/// drop invalid lengths; the interpreter does both, at different points, so
/// a hostile or damaged file can only produce a usable window.
pub(crate) mod bounds {
    /// A text size, in CSS pixels: the smallest one still legible and the
    /// largest the window ever draws one of its own headings.
    pub(crate) const TEXT: (f32, f32) = (6.0, 200.0);
    /// A line's or a panel's radius.
    pub(crate) const RADIUS: (f32, f32) = (0.0, 64.0);
    /// The space between the window's panels.
    pub(crate) const GAP: (f32, f32) = (0.0, 64.0);
    /// How far apart a heading's letters sit.
    pub(crate) const TRACKING: (f32, f32) = (-8.0, 32.0);
    /// A shadow's offsets and blur.
    pub(crate) const SHADOW: (f32, f32) = (-512.0, 512.0);

    /// `value` held to `range`, or `None` when it is not a finite number.
    pub(crate) fn within(value: f32, range: (f32, f32)) -> Option<f32> {
        value.is_finite().then(|| value.clamp(range.0, range.1))
    }

    /// `value` as CSS reads an `opacity`: every number is valid, and the
    /// computed value is held to 0..1.
    pub(crate) fn opacity(value: f32) -> Option<f32> {
        within(value, (0.0, 1.0))
    }
}

/// One declaration, and whether it carried `!important`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    /// The property's name as written, custom properties included.
    pub name: String,
    /// The value as written, still holding any `var()` calls.
    pub value: String,
    /// Whether the declaration ended in `!important`.
    pub important: bool,
}

/// One style rule: its selectors and declarations, under the at-rules around
/// it (whether they ask for a dark or a light colour scheme).
#[derive(Clone, Debug, Default)]
pub struct Rule {
    /// The rule's selectors, each trimmed.
    pub selectors: Vec<String>,
    /// The declarations in source order.
    pub declarations: Vec<Declaration>,
    /// `Some(true)` under `prefers-color-scheme: dark`, `Some(false)` under
    /// its light query, `None` under anything else.
    pub color_scheme: Option<bool>,
    /// The `@container` conditions around the rule, outermost first. A
    /// style query is settled against the theme's custom properties after
    /// the root cascade is known; a size query is settled against the
    /// reference window.
    pub containers: Vec<String>,
    /// The folder or web address of the stylesheet the rule came from, for
    /// the files it names relatively.
    pub origin: Option<String>,
    /// The `@layer` it sits in, by the order the layers were first named.
    pub layer: Option<u32>,
}

/// One step of a `@keyframes` animation.
#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    /// Where the step sits in the animation, 0..1.
    pub offset: f32,
    /// The properties this step sets.
    pub declarations: Vec<Declaration>,
}

/// A named `@keyframes` animation. Themes use these for a moving gradient,
/// a pulse or a glow; ReCore reads the ones that change the window's own
/// ground, and leaves DOM-only animations in the file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keyframes {
    /// The animation's name, as `animation-name` writes it.
    pub name: String,
    /// Sorted by offset, with `from` and `to` as 0 and 1.
    pub frames: Vec<Keyframe>,
}

/// A font a theme carries or points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFace {
    /// The family name the theme asks for, unquoted.
    pub family: String,
    /// The font's weight, 100 to 900, defaulting to 400.
    pub weight: u16,
    /// Whether the face is the family's italic cut.
    pub italic: bool,
    /// Pulled out of a `data:` URI, or read from the path beside the theme.
    pub data: Option<Vec<u8>>,
    /// What the `src` asked for, for the diagnostics.
    pub source: String,
    /// The folder or web address of the stylesheet that named it.
    pub origin: Option<String>,
}

/// What a comment beside a custom property says about it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Note {
    /// The description written beside or above the declaration.
    pub description: Option<String>,
    /// The section the declaration sits under (`/* text colors */`).
    pub group: Option<String>,
    /// The value a `[default: ...]` or `- default: ...` comment names.
    pub default_value: Option<String>,
}

/// What a `@property` rule says about a custom property.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PropertySpec {
    /// The CSS syntax, such as `<color>` or `<length>`.
    pub syntax: Option<String>,
    /// The value used when the theme does not set it.
    pub initial: Option<String>,
}

/// What a stylesheet holds.
#[derive(Clone, Debug, Default)]
pub struct Stylesheet {
    /// Every rule in source order, at-rules unwrapped.
    pub rules: Vec<Rule>,
    /// The `@font-face` families the sheet names.
    pub fonts: Vec<FontFace>,
    /// Notes about rules the reader could not use, one sentence each.
    pub warnings: Vec<String>,
    /// What themes' comments say about their custom properties, by name.
    pub notes: BTreeMap<String, Note>,
    /// The `@property` rules, by custom property.
    pub properties: BTreeMap<String, PropertySpec>,
    /// The `@keyframes` animations, by name.
    pub keyframes: BTreeMap<String, Keyframes>,
    /// The cascade layers, in the order they were first named.
    pub layers: Vec<String>,
}

impl Stylesheet {
    /// The order of `name` among the cascade layers, adding it when new.
    fn layer_rank(&mut self, name: &str) -> u32 {
        if let Some(index) = self.layers.iter().position(|known| known == name) {
            return index as u32;
        }
        self.layers.push(name.to_owned());
        (self.layers.len() - 1) as u32
    }

    /// Registers a `@layer a, b;` statement. A layer named before any rule
    /// is declared sits below the ones later named; that order is part of
    /// the cascade even when the statement itself styles nothing.
    fn declare_layers(&mut self, names: &str) {
        for name in crate::color::split_top(names, ',') {
            let name = name.trim();
            if !name.is_empty() {
                self.layer_rank(name);
            }
        }
    }
}

/// A parse failure; the loader keeps what came before it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// The sentence shown to the person, naming what could not be read.
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}

/// Reads a stylesheet, pulling its `@import`s in through `resolve`.
///
/// `resolve` gets the folder of the stylesheet that holds the import and the
/// path as written, and returns the imported file's text and path, so the
/// caller decides which files and which URLs it will read.
pub fn parse(
    text: &str,
    mut resolve: impl FnMut(Option<&Path>, &str) -> Option<(String, PathBuf)>,
) -> Stylesheet {
    parse_in(text, None, &mut resolve)
}

/// [`parse`] with the folder of `text` for its first imports.
pub fn parse_in(
    text: &str,
    base: Option<&Path>,
    resolve: &mut impl FnMut(Option<&Path>, &str) -> Option<(String, PathBuf)>,
) -> Stylesheet {
    let mut stylesheet = Stylesheet::default();
    let mut chain = Vec::new();
    expand(
        text,
        base,
        None,
        &mut stylesheet,
        resolve,
        &mut chain,
        0,
        None,
        None,
    );
    stylesheet
}

/// Reads a stylesheet, inlining each top-level `@import` where it appears so
/// the rules after it override the ones it brought in, as CSS orders them.
#[allow(
    clippy::too_many_arguments,
    reason = "the parse state is threaded through by value so each import keeps its own"
)]
fn expand(
    text: &str,
    base: Option<&Path>,
    file: Option<&Path>,
    stylesheet: &mut Stylesheet,
    resolve: &mut impl FnMut(Option<&Path>, &str) -> Option<(String, PathBuf)>,
    chain: &mut Vec<PathBuf>,
    depth: usize,
    layer: Option<u32>,
    scheme: Option<bool>,
) {
    if depth > 16 {
        stylesheet
            .warnings
            .push("more than 16 nested imports; stopped".into());
        return;
    }
    // A sheet that imports itself, directly or through others, is read once:
    // the file the reader resolved is the one a cycle repeats, whatever the
    // import beside it wrote. It joins the chain for as long as its own
    // imports are read.
    let mut here = None;
    if let Some(file) = file {
        let now = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
        if chain.contains(&now) {
            stylesheet
                .warnings
                .push(format!("{} imports itself; read once", file.display()));
            return;
        }
        chain.push(now.clone());
        here = Some(now);
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    collect_notes(text, &mut stylesheet.notes);
    let (source, comment_open) = strip_comments(text);
    if comment_open {
        stylesheet
            .warnings
            .push("a comment is never closed; the file ends where it starts".into());
    }
    let origin = base.map(|path| path.to_string_lossy().into_owned());
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut chunk_start = 0;
    // `@import` is only valid before the first rule; once a rule has been read,
    // a later import is ignored, as a browser does.
    let mut seen_rule = false;
    while index < bytes.len() {
        let start = index;
        let mut depth_in_rule = 0u32;
        let mut quote: Option<u8> = None;
        let mut escaped = false;
        let mut statement_end = None;
        let mut block = false;
        while index < bytes.len() {
            let byte = bytes[index];
            if escaped {
                escaped = false;
                index += 1;
                continue;
            }
            match byte {
                b'\\' => escaped = true,
                b'"' | b'\'' if quote == Some(byte) => quote = None,
                b'"' | b'\'' if quote.is_none() => quote = Some(byte),
                b'(' if quote.is_none() => depth_in_rule += 1,
                b')' if quote.is_none() => depth_in_rule = depth_in_rule.saturating_sub(1),
                b';' if quote.is_none() && depth_in_rule == 0 => {
                    statement_end = Some(index);
                    break;
                }
                b'{' if quote.is_none() && depth_in_rule == 0 => {
                    block = true;
                    break;
                }
                _ => {}
            }
            index += 1;
        }
        if block {
            // Skip the whole block, keeping its text in the chunk.
            let Some(close) = matching_brace(&source, index) else {
                break;
            };
            index = close + 1;
            continue;
        }
        let Some(end) = statement_end else {
            break;
        };
        let statement = source[start..end].trim();
        let at_rule = statement
            .strip_prefix('@')
            .map(|rest| rest.trim_start())
            .map(|rest| {
                rest.split(|character: char| character.is_whitespace() || character == '(')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase()
            });
        let is_import = at_rule.as_deref() == Some("import");
        if is_import || matches!(at_rule.as_deref(), Some("layer" | "charset" | "namespace")) {
            // Parse what came before, then the imported sheet or the layer
            // declaration, then go on. A statement outside a rule belongs to
            // the stylesheet's preamble, not to the next rule's body.
            let before = &source[chunk_start..start];
            if !before.trim().is_empty() {
                parse_block(
                    before,
                    stylesheet,
                    scheme,
                    &mut Vec::new(),
                    origin.as_deref(),
                    layer,
                    0,
                );
                seen_rule = true;
            }
            if is_import {
                let rest = statement
                    .strip_prefix('@')
                    .unwrap_or(statement)
                    .trim_start();
                let rest = if rest.is_char_boundary(6) && rest[..6].eq_ignore_ascii_case("import") {
                    rest[6..].trim()
                } else {
                    rest
                };
                match import_path(rest) {
                    Some((path, condition)) => {
                        let condition = ImportCondition::read(&condition);
                        if seen_rule {
                            stylesheet
                                .warnings
                                .push(format!("an @import after a rule is ignored: {path}"));
                        } else if !condition.supports || !media_applies(&condition.media) {
                            stylesheet.warnings.push(format!(
                                "@import {path} does not apply here ({}); skipped",
                                condition.media
                            ));
                        } else if path.starts_with("data:") {
                            stylesheet.warnings.push(format!(
                                "@import {path} is a data: URI, not a stylesheet, so it is skipped"
                            ));
                        } else {
                            let rank = condition
                                .layer
                                .as_deref()
                                .map(|name| stylesheet.layer_rank(name))
                                .or(layer);
                            match resolve(base, &path) {
                                Some((imported_text, full)) => {
                                    let canonical =
                                        fs::canonicalize(&full).unwrap_or_else(|_| full.clone());
                                    if chain.contains(&canonical) {
                                        stylesheet.warnings.push(format!(
                                            "@import {path} already read; skipped the loop"
                                        ));
                                    } else {
                                        expand(
                                            &imported_text,
                                            full.parent(),
                                            Some(&full),
                                            stylesheet,
                                            resolve,
                                            chain,
                                            depth + 1,
                                            rank,
                                            media_scheme(&condition.media).or(scheme),
                                        );
                                    }
                                }
                                None => stylesheet
                                    .warnings
                                    .push(format!("could not read the @import {path}")),
                            }
                        }
                    }
                    None => stylesheet
                        .warnings
                        .push(format!("an @import has no readable path: {rest}")),
                }
            } else if at_rule.as_deref() == Some("layer") {
                stylesheet.declare_layers(prelude_after(statement, "layer"));
            }
            index = end + 1;
            chunk_start = index;
            continue;
        }
        index = end + 1;
    }
    let rest = &source[chunk_start..];
    if !rest.trim().is_empty() {
        parse_block(
            rest,
            stylesheet,
            scheme,
            &mut Vec::new(),
            origin.as_deref(),
            layer,
            0,
        );
    }
    if here.is_some() {
        chain.pop();
    }
}

/// How deep blocks may nest before the reader stops, so a file of nothing
/// but `@media {` cannot exhaust the stack.
const MAX_NESTING: usize = 64;

/// Reads a block's rules into `stylesheet`.
#[allow(
    clippy::too_many_arguments,
    reason = "the block's state is threaded through by value, as in `expand`"
)]
fn parse_block(
    text: &str,
    stylesheet: &mut Stylesheet,
    scheme: Option<bool>,
    containers: &mut Vec<String>,
    origin: Option<&str>,
    layer: Option<u32>,
    nesting: usize,
) {
    if nesting > MAX_NESTING {
        stylesheet
            .warnings
            .push("more than 64 nested blocks; stopped".into());
        return;
    }
    let mut index = 0;
    let bytes = text.as_bytes();
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        if bytes[index] == b'}' {
            index += 1;
            continue;
        }
        let start = index;
        let mut depth = 0u32;
        let mut quote: Option<u8> = None;
        let mut escaped = false;
        let mut open = None;
        while index < bytes.len() {
            let byte = bytes[index];
            if escaped {
                escaped = false;
                index += 1;
                continue;
            }
            match byte {
                b'\\' => escaped = true,
                b'"' | b'\'' if quote == Some(byte) => quote = None,
                b'"' | b'\'' if quote.is_none() => quote = Some(byte),
                b'(' if quote.is_none() => depth += 1,
                b')' if quote.is_none() => depth = depth.saturating_sub(1),
                b';' if quote.is_none() && depth == 0 => {
                    let prelude = text[start..index].trim();
                    if let Some(name) = prelude.strip_prefix('@') {
                        let name = name
                            .split(|character: char| character.is_whitespace() || character == '(')
                            .next()
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        if name == "layer" {
                            stylesheet.declare_layers(prelude_after(prelude, "layer"));
                        } else {
                            stylesheet
                                .warnings
                                .push(format!("a statement inside a block is skipped: {prelude}"));
                        }
                    }
                    index += 1;
                    open = None;
                    break;
                }
                b'{' if quote.is_none() && depth == 0 => {
                    open = Some(index);
                    break;
                }
                b'}' if quote.is_none() && depth == 0 => {
                    index += 1;
                    open = None;
                    break;
                }
                _ => {}
            }
            index += 1;
        }
        let Some(brace) = open else {
            continue;
        };
        let prelude = text[start..brace].trim();
        let close = matching_brace(text, brace);
        if close.is_none() {
            // CSS reads a block that ends with the file as if the file closed
            // it, so a theme whose last rule lost its `}` still paints.
            stylesheet.warnings.push(format!(
                "a rule '{prelude}' has no closing brace; read to the end of the file"
            ));
        }
        let body = close.map_or(&text[brace + 1..], |close| &text[brace + 1..close]);
        // A function left open at the end of a block is closed by the block's
        // end, as a browser closes it: `background: rgb(255,0,0` still paints.
        let body = close_open_functions(body);
        index = close.map_or(text.len(), |close| close + 1);
        if let Some(name) = prelude.strip_prefix('@') {
            let name = name
                .split(|character: char| character.is_whitespace() || character == '(')
                .next()
                .unwrap_or("");
            let lower = name.to_ascii_lowercase();
            match lower.as_str() {
                "font-face" => match font_face(&body, origin) {
                    Some(face) => stylesheet.fonts.push(face),
                    None => stylesheet
                        .warnings
                        .push("an @font-face without a font-family is skipped".into()),
                },
                "layer" => {
                    let name = prelude_after(prelude, "layer").trim().to_owned();
                    let rank = stylesheet.layer_rank(&name);
                    parse_block(
                        &body,
                        stylesheet,
                        scheme,
                        containers,
                        origin,
                        Some(rank),
                        nesting + 1,
                    );
                }
                "media" | "supports" | "container" | "scope" | "document" => {
                    if lower == "supports" && !supports_applies(prelude) {
                        continue;
                    }
                    if lower == "media" && !media_applies(prelude) {
                        continue;
                    }
                    let inner = if lower == "media" {
                        media_scheme(prelude)
                    } else {
                        None
                    };
                    let applies = match (inner, scheme) {
                        // `.theme-light` inside a dark media query can never
                        // apply, and neither can the other way around.
                        (Some(inner), Some(outer)) if inner != outer => continue,
                        (Some(inner), _) => Some(inner),
                        (None, outer) => outer,
                    };
                    if lower == "container" {
                        containers.push(prelude_after(prelude, "container").trim().to_owned());
                        parse_block(
                            &body,
                            stylesheet,
                            applies,
                            containers,
                            origin,
                            layer,
                            nesting + 1,
                        );
                        containers.pop();
                    } else {
                        parse_block(
                            &body,
                            stylesheet,
                            applies,
                            containers,
                            origin,
                            layer,
                            nesting + 1,
                        );
                    }
                }
                "property" => {
                    let name = prelude.strip_prefix("@property").unwrap_or(prelude).trim();
                    if name.starts_with("--") {
                        let mut spec = PropertySpec::default();
                        for declaration in declarations(&body) {
                            match declaration.name.as_str() {
                                "syntax" => spec.syntax = Some(unquote(&declaration.value)),
                                "initial-value" => spec.initial = Some(declaration.value),
                                _ => {}
                            }
                        }
                        stylesheet.properties.insert(name.to_owned(), spec);
                    }
                }
                "keyframes" | "-webkit-keyframes" => {
                    let name = prelude
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("")
                        .trim_matches(['"', '\''])
                        .to_owned();
                    if !name.is_empty() {
                        stylesheet
                            .keyframes
                            .insert(name.clone(), read_keyframes(&name, &body));
                    }
                }
                "counter-style" | "font-feature-values" => {}
                _ => {
                    // An at-rule this reader does not know is kept in the
                    // file and not read, as a browser does: `@starting-style`
                    // holds a transition's first frame, `@page` a printed
                    // page, and neither styles the window.
                    stylesheet.warnings.push(format!(
                        "the at-rule '@{lower}' is not read; its rules are left in the file"
                    ));
                }
            }
            continue;
        }
        let selectors: Vec<String> = crate::color::split_top(prelude, ',')
            .into_iter()
            .map(str::to_owned)
            .collect();
        if selectors.is_empty() || body.trim().is_empty() {
            continue;
        }
        // One invalid selector invalidates the whole list, as it does in a
        // browser; the rule is dropped rather than applied to the others.
        let trailing_comma = prelude.trim_end().ends_with(',');
        if trailing_comma
            || selectors
                .iter()
                .any(|selector| selector.trim().is_empty() || !selector_pseudos_are_known(selector))
        {
            stylesheet.warnings.push(format!(
                "a rule's selector list '{prelude}' has a selector that is not valid; the rule is dropped"
            ));
            continue;
        }
        // CSS nesting: a rule's body may hold declarations, further rules,
        // and conditional at-rules. Flatten the nested selectors with `&`
        // replaced, then let this same reader settle the result.
        let (own, nested) = split_rule_body(&body);
        if !own.trim().is_empty() {
            stylesheet.rules.push(Rule {
                selectors: selectors.clone(),
                declarations: declarations(&own),
                color_scheme: scheme,
                containers: containers.clone(),
                origin: origin.map(str::to_owned),
                layer,
            });
        }
        for (nested_prelude, nested_body) in nested {
            let expanded = nested_css(&selectors, &nested_prelude, &nested_body);
            parse_block(
                &expanded,
                stylesheet,
                scheme,
                containers,
                origin,
                layer,
                nesting + 1,
            );
        }
    }
}

/// Splits a rule body at its top-level `;` into declarations and nested
/// rules. Nested at-rules (`@media`, `@container`, ...) appear beside rules
/// and keep their own body. Braces inside strings or parentheses are not
/// structural.
fn split_rule_body(body: &str) -> (String, Vec<(String, String)>) {
    let mut own = String::new();
    let mut nested = Vec::new();
    let bytes = body.as_bytes();
    let mut index = 0;
    let mut chunk = 0;
    let mut depth = 0u32;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b'"' | b'\'' if quote == Some(byte) => quote = None,
            b'"' | b'\'' if quote.is_none() => quote = Some(byte),
            b'(' if quote.is_none() => depth += 1,
            b')' if quote.is_none() => depth = depth.saturating_sub(1),
            b'{' if quote.is_none() && depth == 0 => {
                let Some(close) = matching_brace(body, index) else {
                    break;
                };
                let before = &body[chunk..index];
                let (plain, prelude) = split_last_declaration(before);
                own.push_str(plain);
                let prelude = prelude.trim();
                if !prelude.is_empty() {
                    nested.push((prelude.to_owned(), body[index + 1..close].to_owned()));
                }
                index = close + 1;
                chunk = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    own.push_str(&body[chunk..]);
    (own, nested)
}

/// Splits `text` after its last top-level `;`, so a nested rule's prelude is
/// not mistaken for a declaration.
fn split_last_declaration(text: &str) -> (&str, &str) {
    let bytes = text.as_bytes();
    let mut depth = 0u32;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut last = None;
    for (index, &byte) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b'"' | b'\'' if quote == Some(byte) => quote = None,
            b'"' | b'\'' if quote.is_none() => quote = Some(byte),
            b'(' if quote.is_none() => depth += 1,
            b')' if quote.is_none() => depth = depth.saturating_sub(1),
            b';' if quote.is_none() && depth == 0 => last = Some(index + 1),
            _ => {}
        }
    }
    match last {
        Some(at) => text.split_at(at),
        None => ("", text),
    }
}

/// A nested rule or at-rule, expressed for the flat parser.
fn nested_css(parents: &[String], prelude: &str, body: &str) -> String {
    let mut out = String::new();
    if prelude.trim_start().starts_with('@') {
        out.push_str(prelude.trim());
        out.push('{');
        for parent in parents {
            out.push_str(parent);
            out.push('{');
            out.push_str(body);
            out.push('}');
        }
        out.push('}');
        return out;
    }
    for parent in parents {
        for child in crate::color::split_top(prelude, ',') {
            let child = child.trim();
            let selector = if child.contains('&') {
                child.replace('&', parent)
            } else {
                format!("{parent} {child}")
            };
            out.push_str(&selector);
            out.push('{');
            out.push_str(body);
            out.push('}');
        }
    }
    out
}

/// Rewrites every length with an absolute or viewport unit as its value in
/// CSS pixels at the reference viewport.
///
/// `style-engine` 1.0.0 does not know `pt`, `pc`, `in`, `cm`, `mm`, `Q` or
/// the viewport units beyond `vw` and `vh`, and the reader's own length
/// parsers only take `px`, `rem`, `em` and unitless numbers. A number inside
/// a quoted string, a comment or a `url(...)` is left alone, as is an `em`,
/// `rem`, `%` or a unit this reader does not rewrite. The value keeps at most
/// four decimals, so `9pt` becomes `12px` and `1pt` becomes `1.3333px`.
pub(crate) fn normalize_units(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut out: Option<String> = None;
    let mut written = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' | b'\'' => index = skip_string(bytes, index),
            b'/' if bytes.get(index + 1) == Some(&b'*') => index = skip_comment(bytes, index),
            b'u' | b'U' if starts_url(bytes, index) => index = skip_url(bytes, index),
            _ => match unit_token(text, index) {
                Some((end, pixels)) => {
                    let output = out.get_or_insert_with(|| String::with_capacity(text.len()));
                    output.push_str(&text[written..index]);
                    output.push_str(&format_px(pixels));
                    output.push_str("px");
                    written = end;
                    index = end;
                }
                None => index += 1,
            },
        }
    }
    match out {
        Some(mut output) => {
            output.push_str(&text[written..]);
            Cow::Owned(output)
        }
        None => Cow::Borrowed(text),
    }
}

/// Whether `byte` continues a CSS identifier, so a number right after it is
/// part of the same name rather than a length of its own.
fn identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') || !byte.is_ascii()
}

/// The byte after the quoted string that starts at `start`.
fn skip_string(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            byte if byte == quote => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

/// The byte after the comment that starts at `start`.
fn skip_comment(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 2;
    while index + 1 < bytes.len() {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

/// Whether `url(` starts at `start`, as its own token.
fn starts_url(bytes: &[u8], start: usize) -> bool {
    if start > 0 && identifier_byte(bytes[start - 1]) {
        return false;
    }
    bytes
        .get(start..start + 4)
        .is_some_and(|word| word.eq_ignore_ascii_case(b"url("))
}

/// The byte after the `url(...)` that starts at `start`.
fn skip_url(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 3;
    let mut depth = 0u32;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(_) if byte == b'\\' => index += 2,
            Some(active) if byte == active => {
                quote = None;
                index += 1;
            }
            Some(_) => index += 1,
            None => match byte {
                b'"' | b'\'' => {
                    quote = Some(byte);
                    index += 1;
                }
                b'(' => {
                    depth += 1;
                    index += 1;
                }
                b')' => {
                    depth = depth.saturating_sub(1);
                    index += 1;
                    if depth == 0 {
                        return index;
                    }
                }
                _ => index += 1,
            },
        }
    }
    bytes.len()
}

/// The end and pixel value of the length starting at `start`, when its unit
/// is one this reader rewrites.
fn unit_token(text: &str, start: usize) -> Option<(usize, f32)> {
    let bytes = text.as_bytes();
    if start > 0 && identifier_byte(bytes[start - 1]) {
        return None;
    }
    let mut index = start;
    let sign = match bytes.get(index) {
        Some(b'+') => {
            index += 1;
            1.0
        }
        Some(b'-') => {
            index += 1;
            -1.0
        }
        _ => 1.0,
    };
    let number_start = index;
    let mut digits = 0usize;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
        digits += 1;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    // An exponent, as in `1e3pt`; a unit starting with `e` is not one.
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let mut cursor = index + 1;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        let exponent_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor > exponent_start {
            index = cursor;
        }
    }
    let number_end = index;
    let unit_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_alphabetic) {
        index += 1;
    }
    if index == unit_start {
        return None;
    }
    let unit = text.get(unit_start..index)?.to_ascii_lowercase();
    let factor = unit_factor(&unit)?;
    if factor == 1.0 {
        // `px` already is the unit the engine reads.
        return None;
    }
    let number = text.get(number_start..number_end)?.parse::<f32>().ok()?;
    let pixels = sign * number * factor;
    pixels.is_finite().then_some((index, pixels))
}

/// The pixel value of one unit at the reference viewport.
fn unit_factor(unit: &str) -> Option<f32> {
    let factor = match unit {
        "in" => 96.0,
        "cm" => 96.0 / 2.54,
        "mm" => 96.0 / 25.4,
        "q" => 96.0 / 101.6,
        "pt" => 96.0 / 72.0,
        "pc" => 16.0,
        "vw" | "svw" | "lvw" | "dvw" => VIEWPORT.0 / 100.0,
        "vh" | "svh" | "lvh" | "dvh" => VIEWPORT.1 / 100.0,
        "vmin" | "svmin" | "lvmin" | "dvmin" => VIEWPORT.0.min(VIEWPORT.1) / 100.0,
        "vmax" | "svmax" | "lvmax" | "dvmax" => VIEWPORT.0.max(VIEWPORT.1) / 100.0,
        _ => return None,
    };
    Some(factor)
}

/// A pixel count with at most four decimals and no trailing zeros.
fn format_px(value: f32) -> String {
    let mut text = format!("{value:.4}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        text.clear();
        text.push('0');
    }
    text
}

/// The byte offset of `needle` in `text`, ignoring ASCII case, as CSS
/// keywords are case-insensitive.
pub(crate) fn find_ignore_case(text: &str, needle: &str) -> Option<usize> {
    let text = text.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || needle.len() > text.len() {
        return None;
    }
    text.windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

/// What follows an at-rule's name, ignoring the case CSS keywords allow.
fn prelude_after<'a>(prelude: &'a str, name: &str) -> &'a str {
    let rest = prelude.strip_prefix('@').unwrap_or(prelude).trim_start();
    rest.get(..name.len())
        .filter(|start| start.eq_ignore_ascii_case(name))
        .and_then(|_| rest.get(name.len()..))
        .unwrap_or("")
}

/// The path in `@import "x"`, `@import url(x)` or `@import url("x")`.
fn import_path(rest: &str) -> Option<(String, String)> {
    let rest = rest.trim();
    if let Some(offset) = find_ignore_case(rest, "url(") {
        if offset != 0 {
            return None;
        }
        let inside = &rest[4..];
        let end = inside.find(')')?;
        return Some((
            unquote(inside[..end].trim()),
            inside[end + 1..].trim().to_owned(),
        ));
    }
    let first = rest.split_whitespace().next()?;
    if first.starts_with('"') || first.starts_with('\'') {
        let quote = first.chars().next()?;
        let inner = first.strip_prefix(quote)?.strip_suffix(quote)?;
        return Some((inner.to_owned(), rest[first.len()..].trim().to_owned()));
    }
    None
}

/// The `layer()`, `supports()` and media conditions an `@import` names,
/// in the order CSS allows them.
struct ImportCondition {
    layer: Option<String>,
    supports: bool,
    media: String,
}

impl ImportCondition {
    fn read(condition: &str) -> Self {
        let mut condition = condition.trim();
        let mut layer = None;
        let mut supports = true;
        loop {
            if let Some(rest) = strip_prefix_ignore_case(condition, "layer") {
                let rest = rest.trim_start();
                if let Some(after) = rest.strip_prefix('(') {
                    let name = matching_paren(after)
                        .map(|end| after[..end].trim().to_owned())
                        .unwrap_or_default();
                    layer = Some(name);
                    condition = matching_paren(after)
                        .map(|end| after[end + 1..].trim_start())
                        .unwrap_or("");
                } else {
                    layer = Some(String::new());
                    condition = rest;
                }
                continue;
            }
            if let Some(rest) = strip_prefix_ignore_case(condition, "supports") {
                let rest = rest.trim_start();
                if let Some(after) = rest.strip_prefix('(') {
                    let Some(end) = matching_paren(after) else {
                        break;
                    };
                    supports &= supports_value(&after[..end]);
                    condition = after[end + 1..].trim_start();
                    continue;
                }
            }
            break;
        }
        Self {
            layer,
            supports,
            media: condition.trim().to_owned(),
        }
    }
}

/// `text` after `prefix`, compared without ASCII case.
fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.get(..prefix.len())
        .filter(|start| start.eq_ignore_ascii_case(prefix))
        .and_then(|_| text.get(prefix.len()..))
}

/// Whether a media query applies to the window at its reference size. A
/// feature the window cannot know is left applicable, so a theme keeps the
/// rules a current browser would show rather than losing them silently.
fn media_applies(prelude: &str) -> bool {
    let query = media_query_text(prelude);
    query.is_empty()
        || crate::color::split_top(query, ',')
            .into_iter()
            .any(media_query_could_apply)
}

/// The query text of an `@media` prelude or of a bare `@import` condition.
fn media_query_text(prelude: &str) -> &str {
    let prelude = prelude.trim();
    if prelude.starts_with('@') {
        prelude_after(prelude, "media").trim()
    } else {
        prelude
    }
}

/// Whether a query could apply for either appearance. A colour-scheme
/// condition is unknown when the stylesheet is parsed, so it is tried both
/// ways; a rule that relies on a wide window still fails at the reference
/// size, as it should.
fn media_query_could_apply(query: &str) -> bool {
    if query.to_ascii_lowercase().contains("prefers-color-scheme") {
        media_query_applies_for(query, Some(true)) || media_query_applies_for(query, Some(false))
    } else {
        media_query_applies_for(query, None)
    }
}

fn media_query_applies_for(query: &str, scheme: Option<bool>) -> bool {
    let mut text = query.trim();
    if text.is_empty() {
        return true;
    }
    let lower = text.to_ascii_lowercase();
    let negated = lower.starts_with("not ");
    if negated {
        text = text[4..].trim_start();
    } else if lower.starts_with("only ") {
        text = text[5..].trim_start();
    }
    let mut result = true;
    // A media type comes first: `screen and (...)`, `print`, `all`.
    if !text.starts_with('(') {
        let (kind, rest) = split_first_word(text);
        if !kind.is_empty() {
            let kind = kind.to_ascii_lowercase();
            if matches!(kind.as_str(), "and" | "or") {
                return false;
            }
            result = !matches!(kind.as_str(), "print" | "speech");
            let rest = rest.trim_start();
            if rest.is_empty() {
                text = rest;
            } else if let Some(after) = strip_prefix_ignore_case(rest, "and").filter(|after| {
                after.is_empty() || after.starts_with(char::is_whitespace) || after.starts_with('(')
            }) {
                let after = after.trim_start();
                // `screen and` names no term after its operator.
                if after.is_empty() {
                    return false;
                }
                text = after;
            } else {
                // A media type combines with its conditions only through `and`.
                return false;
            }
        }
    }
    if !text.is_empty() {
        for part in split_condition(text, "and") {
            result &= media_condition(part, scheme);
        }
    }
    result != negated
}

fn media_condition(condition: &str, scheme: Option<bool>) -> bool {
    let condition = condition.trim();
    if condition.is_empty() {
        // An empty term means the query had an operator without a side.
        return false;
    }
    if let Some(rest) = strip_prefix_ignore_case(condition, "not")
        && (rest.starts_with(char::is_whitespace) || rest.starts_with('('))
    {
        return !media_condition(rest.trim_start(), scheme);
    }
    let or_parts = split_condition(condition, "or");
    if or_parts.len() > 1 {
        return or_parts.iter().all(|part| !part.is_empty())
            && or_parts.iter().any(|part| media_condition(part, scheme));
    }
    let and_parts = split_condition(condition, "and");
    if and_parts.len() > 1 {
        return and_parts.iter().all(|part| !part.is_empty())
            && and_parts.iter().all(|part| media_condition(part, scheme));
    }
    let inner = outer_parens(condition).unwrap_or(condition).trim();
    if inner.is_empty() {
        // `()` is an empty feature, not a condition any window matches.
        return false;
    }
    if let Some(rest) = strip_prefix_ignore_case(inner, "not")
        && (rest.starts_with(char::is_whitespace) || rest.starts_with('('))
    {
        return !media_condition(rest.trim_start(), scheme);
    }
    media_feature(inner, scheme)
}

fn media_feature(feature: &str, scheme: Option<bool>) -> bool {
    let feature = feature.trim();
    if let Some(value) = media_range(feature) {
        return value;
    }
    let Some((name, value)) = feature.split_once(':') else {
        // A boolean feature, such as `(hover)` or `(color)`, applies when the
        // window has the thing at all. The zero-valued ones do not. Anything
        // that is not a feature's own name — a stray operator, punctuation —
        // is a malformed condition, which no window matches.
        let name = feature.trim().to_ascii_lowercase();
        if name.is_empty()
            || matches!(name.as_str(), "and" | "or" | "not" | "only")
            || !name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return false;
        }
        return !matches!(name.as_str(), "monochrome" | "grid" | "color-index");
    };
    let name = name.trim().to_ascii_lowercase();
    let value = value.trim().to_ascii_lowercase();
    match name.as_str() {
        "width" | "device-width" => compare(width(), &value),
        "height" | "device-height" => compare(height(), &value),
        "min-width" | "min-device-width" => {
            media_length(&value).is_some_and(|wanted| width() >= wanted)
        }
        "max-width" | "max-device-width" => {
            media_length(&value).is_some_and(|wanted| width() <= wanted)
        }
        "min-height" | "min-device-height" => {
            media_length(&value).is_some_and(|wanted| height() >= wanted)
        }
        "max-height" | "max-device-height" => {
            media_length(&value).is_some_and(|wanted| height() <= wanted)
        }
        "aspect-ratio" => ratio(value.as_str()).is_none_or(|ratio| close(viewport_ratio(), ratio)),
        "min-aspect-ratio" => ratio(value.as_str()).is_none_or(|ratio| viewport_ratio() >= ratio),
        "max-aspect-ratio" => ratio(value.as_str()).is_none_or(|ratio| viewport_ratio() <= ratio),
        "orientation" => match value.as_str() {
            "landscape" => width() >= height(),
            "portrait" => height() > width(),
            _ => true,
        },
        "prefers-color-scheme" => scheme.unwrap_or(true),
        "prefers-reduced-motion" => value != "reduce",
        "prefers-reduced-transparency" => value == "no-preference",
        "prefers-reduced-data" => value != "reduce",
        "prefers-contrast" => value == "no-preference",
        "forced-colors" => value == "none",
        "inverted-colors" => value != "inverted",
        "hover" | "any-hover" => value != "none",
        "pointer" | "any-pointer" => value == "fine",
        "color" => compare(8.0, value.as_str()),
        "min-color" => 8.0 >= media_number(&value).unwrap_or(0.0),
        "max-color" => 8.0 <= media_number(&value).unwrap_or(f32::MAX),
        "color-index" | "monochrome" => compare(0.0, value.as_str()),
        "grid" => value == "0",
        "resolution" | "min-resolution" => {
            let wanted = resolution(&value).unwrap_or(0.0);
            if name == "min-resolution" {
                1.0 >= wanted
            } else {
                close(1.0, wanted)
            }
        }
        "max-resolution" => 1.0 <= resolution(&value).unwrap_or(f32::MAX),
        "color-gamut" => value == "srgb",
        "dynamic-range" | "video-dynamic-range" => value == "standard",
        "scan" => value == "progressive",
        "update" => value != "slow",
        "scripting" => value != "none",
        "display-mode" => value == "browser",
        "overflow-block" | "overflow-inline" => value != "none",
        "environment-blending" => value == "opaque",
        _ => true,
    }
}

fn media_range(feature: &str) -> Option<bool> {
    let words = range_words(feature);
    if !(3..=5).contains(&words.len()) {
        return None;
    }
    let operator = |text: &str| matches!(text, ">=" | "<=" | ">" | "<" | "=");
    let compare = |left: f32, op: &str, right: f32| match op {
        ">=" => left >= right,
        "<=" => left <= right,
        ">" => left > right,
        "<" => left < right,
        "=" => close(left, right),
        _ => true,
    };
    if words.len() == 3 && operator(words[1]) {
        let (name, value, flip) = if feature_value(words[0]).is_some() {
            (words[0], words[2], false)
        } else if feature_value(words[2]).is_some() {
            (words[2], words[0], true)
        } else {
            return None;
        };
        let target = feature_value(name).unwrap_or(0.0);
        let wanted = feature_number(name, value)?;
        return Some(if flip {
            compare(wanted, words[1], target)
        } else {
            compare(target, words[1], wanted)
        });
    }
    if words.len() == 5 && operator(words[1]) && operator(words[3]) {
        let name = words[2];
        let target = feature_value(name)?;
        let low = feature_number(name, words[0])?;
        let high = feature_number(name, words[4])?;
        return Some(compare(low, words[1], target) && compare(target, words[3], high));
    }
    None
}

/// A range condition split into its parts, a comparison kept whole:
/// `(400px <= width <= 1000px)` gives `["400px", "<=", "width", "<=",
/// "1000px"]`. Spacing beside an operator is optional, as CSS allows.
fn range_words(feature: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let bytes = feature.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let start = index;
        if matches!(bytes[index], b'<' | b'>' | b'=') {
            index += 1;
            if index < bytes.len() && bytes[index] == b'=' && bytes[start] != b'=' {
                index += 1;
            }
            words.push(&feature[start..index]);
            continue;
        }
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && !matches!(bytes[index], b'<' | b'>' | b'=')
        {
            index += 1;
        }
        words.push(&feature[start..index]);
    }
    words
}

fn feature_value(name: &str) -> Option<f32> {
    match name.to_ascii_lowercase().as_str() {
        "width" | "device-width" | "min-width" | "max-width" | "min-device-width"
        | "max-device-width" => Some(width()),
        "height" | "device-height" | "min-height" | "max-height" | "min-device-height"
        | "max-device-height" => Some(height()),
        "aspect-ratio" | "device-aspect-ratio" | "min-aspect-ratio" | "max-aspect-ratio" => {
            Some(viewport_ratio())
        }
        "resolution" | "min-resolution" | "max-resolution" => Some(1.0),
        "color" | "min-color" | "max-color" => Some(8.0),
        "color-index" | "monochrome" | "grid" => Some(0.0),
        _ => None,
    }
}

fn feature_number(feature: &str, value: &str) -> Option<f32> {
    match feature.to_ascii_lowercase().as_str() {
        "aspect-ratio" | "device-aspect-ratio" | "min-aspect-ratio" | "max-aspect-ratio" => {
            ratio(value)
        }
        "resolution" | "min-resolution" | "max-resolution" => resolution(value),
        "color" | "color-index" | "monochrome" | "grid" | "min-color" | "max-color" => {
            media_number(value)
        }
        _ => media_length(value),
    }
}

fn compare(actual: f32, value: &str) -> bool {
    media_number(value).is_some_and(|wanted| close(actual, wanted))
}

fn close(left: f32, right: f32) -> bool {
    (left - right).abs() <= 0.001 * left.abs().max(right.abs()).max(1.0)
}

fn width() -> f32 {
    VIEWPORT.0
}

fn height() -> f32 {
    VIEWPORT.1
}

fn viewport_ratio() -> f32 {
    width() / height().max(1.0)
}

fn media_number(value: &str) -> Option<f32> {
    value.trim().parse().ok()
}

fn media_length(value: &str) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    let parse = |suffix: &str, scale: f32| {
        value
            .strip_suffix(suffix)
            .and_then(|number| number.trim().parse::<f32>().ok())
            .map(|number| number * scale)
    };
    parse("px", 1.0)
        .or_else(|| parse("rem", 16.0))
        .or_else(|| parse("em", 16.0))
        .or_else(|| parse("vw", width() / 100.0))
        .or_else(|| parse("vh", height() / 100.0))
        .or_else(|| parse("pt", 96.0 / 72.0))
        .or_else(|| parse("pc", 16.0))
        .or_else(|| parse("in", 96.0))
        .or_else(|| parse("cm", 96.0 / 2.54))
        .or_else(|| parse("mm", 96.0 / 25.4))
        .or_else(|| value.parse().ok())
}

fn ratio(value: &str) -> Option<f32> {
    if let Some((left, right)) = value.split_once('/') {
        let left: f32 = left.trim().parse().ok()?;
        let right: f32 = right.trim().parse().ok()?;
        return (right.abs() > f32::EPSILON).then_some(left / right);
    }
    value.trim().parse().ok()
}

fn resolution(value: &str) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    let parse = |suffix: &str, scale: f32| {
        value
            .strip_suffix(suffix)
            .and_then(|number| number.trim().parse::<f32>().ok())
            .map(|number| number * scale)
    };
    parse("dppx", 1.0)
        .or_else(|| parse("dpi", 1.0 / 96.0))
        .or_else(|| parse("dpcm", 2.54 / 96.0))
        .or_else(|| value.parse().ok())
}

/// Whether a `@container` condition applies. A style query reads the
/// theme's settled custom properties; a size query uses the same reference
/// window as a media query. `scroll-state()` and names a native window has
/// no equivalent for stay applicable, as unknown media features do.
pub(crate) fn container_applies(condition: &str, vars: &Vars) -> bool {
    let condition = condition.trim();
    // A named container puts its name before the query: `card style(...)`.
    let condition = if condition.starts_with('(')
        || strip_prefix_ignore_case(condition, "style")
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['(', ' ', '\t', '\n']))
        || strip_prefix_ignore_case(condition, "scroll-state")
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['(', ' ', '\t', '\n']))
        || strip_prefix_ignore_case(condition, "not")
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['(', ' ', '\t', '\n']))
    {
        condition
    } else {
        let (_, rest) = split_first_word(condition);
        rest.trim_start()
    };
    container_condition(condition, vars)
}

fn container_condition(condition: &str, vars: &Vars) -> bool {
    let condition = condition.trim();
    if condition.is_empty() {
        return true;
    }
    if let Some(rest) = strip_prefix_ignore_case(condition, "not")
        && (rest.starts_with(char::is_whitespace) || rest.starts_with('('))
    {
        return !container_condition(rest.trim_start(), vars);
    }
    let or_parts = split_condition(condition, "or");
    if or_parts.len() > 1 {
        return or_parts.iter().any(|part| container_condition(part, vars));
    }
    let and_parts = split_condition(condition, "and");
    if and_parts.len() > 1 {
        return and_parts.iter().all(|part| container_condition(part, vars));
    }
    let inner = outer_parens(condition).unwrap_or(condition).trim();
    if let Some(rest) = strip_prefix_ignore_case(inner, "style") {
        let rest = rest.trim_start();
        return rest
            .strip_prefix('(')
            .and_then(|after| matching_paren(after).map(|end| &after[..end]))
            .is_none_or(|query| style_query(query, vars));
    }
    if let Some(rest) = strip_prefix_ignore_case(inner, "scroll-state") {
        let _ = rest;
        return true;
    }
    media_feature(inner, None)
}

fn style_query(query: &str, vars: &Vars) -> bool {
    for part in split_condition(query, "and") {
        let part = outer_parens(part).unwrap_or(part).trim();
        let Some(name) = part
            .split([':', ' '])
            .next()
            .filter(|name| name.starts_with("--"))
        else {
            continue;
        };
        let actual = vars
            .get(name)
            .and_then(|value| vars.resolve(value))
            .map(|value| value.trim().to_owned());
        let Some((_, expected)) = part.split_once(':') else {
            if actual.is_none() {
                return false;
            }
            continue;
        };
        let expected = vars
            .resolve(expected.trim())
            .map(|value| value.trim().to_owned())
            .unwrap_or_else(|| expected.trim().to_owned());
        let Some(actual) = actual else {
            return false;
        };
        if !same_tokens(&actual, &expected) {
            return false;
        }
    }
    true
}

/// Compares two custom property values as token sequences: whitespace is
/// not significant, and a theme's keyword case is not either.
fn same_tokens(left: &str, right: &str) -> bool {
    let normalize = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    };
    normalize(left) == normalize(right)
}

fn split_first_word(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    (&text[..end], &text[end..])
}

fn outer_parens(text: &str) -> Option<&str> {
    let text = text.trim();
    if !text.starts_with('(') {
        return None;
    }
    matching_paren(&text[1..])
        .filter(|end| end + 2 == text.len())
        .map(|end| &text[1..end + 1])
}

/// Whether an `@supports` prelude applies. Features ReCore's native selector
/// engine cannot match are treated as unsupported; the rest of CSS's modern
/// features are treated as present, as a current browser would.
fn supports_applies(prelude: &str) -> bool {
    supports_value(prelude_after(prelude, "supports").trim())
}

fn supports_value(condition: &str) -> bool {
    let condition = condition.trim();
    // A pair of parentheses may wrap a complete condition rather than a
    // declaration: `((display: grid) and (not (display: bogus)))`. Peeling
    // that level before the operators makes the tree visible.
    if let Some(inner) = outer_parens(condition) {
        return supports_value(inner);
    }
    if let Some(rest) = strip_prefix_ignore_case(condition, "not")
        && (rest.starts_with(char::is_whitespace) || rest.starts_with('('))
    {
        let rest = rest.trim_start();
        return !rest.is_empty() && !supports_value(rest);
    }
    let or_parts = split_condition(condition, "or");
    if or_parts.len() > 1 {
        return or_parts.iter().all(|part| !part.is_empty())
            && or_parts.iter().any(|part| supports_value(part));
    }
    let and_parts = split_condition(condition, "and");
    if and_parts.len() > 1 {
        return and_parts.iter().all(|part| !part.is_empty())
            && and_parts.iter().all(|part| supports_value(part));
    }
    if let Some(selector) = strip_prefix_ignore_case(condition, "selector") {
        let selector = selector.trim_start();
        if let Some(after) = selector.strip_prefix('(')
            && let Some(end) = matching_paren(after)
            && after[end + 1..].trim().is_empty()
        {
            let argument = after[..end].trim();
            return !argument.is_empty() && selector_pseudos_are_known(argument);
        }
    }
    let inner = condition
        .strip_prefix('(')
        .and_then(|text| text.strip_suffix(')'))
        .unwrap_or(condition)
        .trim();
    // A native window paints no blur, mask, container query or view
    // transition, so a theme's branch that needs one is not taken; its
    // `not (...)` fallback is.
    const UNSUPPORTED: &[&str] = &[
        "backdrop-filter",
        "-webkit-backdrop-filter",
        "mask",
        "-webkit-mask",
        "container-type",
        "view-transition-name",
        "animation-timeline",
        "scroll-timeline",
    ];
    let property = inner
        .split_once(':')
        .map(|(property, _)| property.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if UNSUPPORTED.contains(&property.as_str()) {
        return false;
    }
    // Everything else is asked of the same engine that will read the sheet:
    // a declaration it drops is one this window cannot use either, so a
    // theme's `@supports` fallback takes the branch a browser would.
    declaration_is_valid(inner)
}

/// Whether every pseudo-class and pseudo-element in `selector` is one the
/// native matcher reads. `selector()` asks about syntax, so a selector that
/// names an unknown pseudo is unsupported even when its subject is a region.
fn selector_pseudos_are_known(selector: &str) -> bool {
    // `:is()` and `:where()` forgive an argument that is not a valid
    // selector; the rule still applies to the others. Their arguments are
    // therefore left out before the names are checked.
    let selector = without_forgiving_arguments(selector);
    let selector = selector.as_str();
    let bytes = selector.as_bytes();
    let mut index = 0;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut brackets = 0u32;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b'"' | b'\'' if quote == Some(byte) => quote = None,
            b'"' | b'\'' if quote.is_none() => quote = Some(byte),
            b'[' if quote.is_none() => brackets += 1,
            b']' if quote.is_none() => brackets = brackets.saturating_sub(1),
            b':' if quote.is_none() && brackets == 0 => {
                let mut start = index + 1;
                if bytes.get(start) == Some(&b':') {
                    start += 1;
                }
                let mut end = start;
                for (offset, character) in selector[start..].char_indices() {
                    if character.is_alphanumeric() || matches!(character, '-' | '_') {
                        end = start + offset + character.len_utf8();
                    } else {
                        break;
                    }
                }
                if end == start || !pseudo_name_is_known(&selector[start..end]) {
                    return false;
                }
                index = end;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    true
}

/// `selector` with the arguments of every `:is()` and `:where()` blanked
/// out, so a forgiving list does not make the rule invalid.
fn without_forgiving_arguments(selector: &str) -> String {
    let mut out = String::with_capacity(selector.len());
    let mut rest = selector;
    loop {
        let lower = rest.to_ascii_lowercase();
        let next = [":is(", ":where("]
            .iter()
            .filter_map(|function| lower.find(function).map(|at| (at, function.len())))
            .min_by_key(|(at, _)| *at);
        let Some((at, length)) = next else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..at]);
        let after = &rest[at + length..];
        let Some(end) = matching_paren(after) else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[at..at + length]);
        out.push(')');
        rest = &after[end + 1..];
    }
}

/// The pseudo names ReCore's own selector matcher knows: the interaction and
/// structural states, the functional selector lists, the window-wide
/// pseudo-elements, and the scrollbar family themes skin.
fn pseudo_name_is_known(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    // A vendor pseudo (`::-webkit-scrollbar-thumb`) is valid syntax even
    // where the window cannot paint it; a name that is not a CSS pseudo at
    // all makes its selector invalid.
    if name.starts_with('-') {
        return true;
    }
    matches!(
        name.as_str(),
        // States and structures a theme uses on a region.
        "hover"
            | "focus"
            | "focus-visible"
            | "focus-within"
            | "active"
            | "not"
            | "is"
            | "where"
            | "has"
            | "root"
            | "empty"
            | "first-child"
            | "last-child"
            | "only-child"
            | "first-of-type"
            | "last-of-type"
            | "only-of-type"
            | "nth-child"
            | "nth-last-child"
            | "nth-of-type"
            | "nth-last-of-type"
            | "before"
            | "after"
            | "selection"
            | "placeholder"
            | "-moz-selection"
            | "-moz-placeholder"
            | "-webkit-input-placeholder"
            // States CSS defines beyond the window's own matcher. They are
            // valid syntax, so a list containing one is not invalid.
            | "any-link"
            | "autofill"
            | "blank"
            | "checked"
            | "default"
            | "defined"
            | "dir"
            | "disabled"
            | "enabled"
            | "first"
            | "first-letter"
            | "first-line"
            | "fullscreen"
            | "future"
            | "in-range"
            | "indeterminate"
            | "invalid"
            | "lang"
            | "last"
            | "left"
            | "link"
            | "local-link"
            | "marker"
            | "modal"
            | "optional"
            | "out-of-range"
            | "past"
            | "paused"
            | "picture-in-picture"
            | "placeholder-shown"
            | "playing"
            | "popover-open"
            | "read-only"
            | "read-write"
            | "required"
            | "right"
            | "scope"
            | "state"
            | "target"
            | "target-within"
            | "user-invalid"
            | "user-valid"
            | "valid"
            | "visited"
            | "backdrop"
            | "cue"
            | "file-selector-button"
            | "part"
            | "slotted"
            | "spelling-error"
            | "grammar-error"
            | "target-text"
            | "view-transition"
            | "view-transition-group"
            | "view-transition-image-pair"
            | "view-transition-old"
            | "view-transition-new"
    ) || name.starts_with("-webkit-scrollbar")
}

/// Whether the engine reads `condition` (`display: grid`) as a declaration.
fn declaration_is_valid(condition: &str) -> bool {
    let Some((property, value)) = condition.split_once(':') else {
        return false;
    };
    // A custom property is always a supported value, and `var()` is resolved
    // where it is used, not here.
    if property.trim().starts_with("--") || value.contains("var(") {
        return true;
    }
    // The colour functions the reader reduces before the engine sees them
    // are supported here even though the engine alone drops them.
    let reduced = crate::semantic::replace_colors(value, false).unwrap_or_else(|| value.to_owned());
    let sheet = crate::style_engine::css::stylesheet::parse_stylesheet(&format!(
        "x {{ {property}: {reduced} }}"
    ));
    !sheet.report.warnings.iter().any(|warning| {
        matches!(
            warning.severity,
            crate::style_engine::error::ParseSeverity::Dropped
        )
    })
}

/// Splits on `and`/`or` outside parentheses. A keyword with no term after it
/// leaves an empty last part, so callers can reject `(x) and` as malformed
/// rather than reading it as `(x)`.
fn split_condition<'a>(text: &'a str, keyword: &str) -> Vec<&'a str> {
    let keyword_len = keyword.len();
    let mut parts = Vec::new();
    let mut depth = 0u32;
    let mut start = 0;
    let mut trailing = false;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index + keyword_len <= bytes.len() {
        match bytes[index] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        // A keyword is only an operator between two terms: whitespace before
        // it, whitespace or the end after it, and no parenthesis around it.
        if depth == 0
            && index > 0
            && bytes[index - 1].is_ascii_whitespace()
            && bytes[index..index + keyword_len].eq_ignore_ascii_case(keyword.as_bytes())
            && bytes
                .get(index + keyword_len)
                .is_none_or(|byte| byte.is_ascii_whitespace())
        {
            parts.push(text[start..index].trim());
            let after = index + keyword_len;
            if text[after..].trim().is_empty() {
                trailing = true;
                break;
            }
            start = after;
            index = after;
            continue;
        }
        index += 1;
    }
    if trailing {
        parts.push("");
    } else if start > 0 {
        parts.push(text[start..].trim());
    } else {
        parts.push(text);
    }
    parts
}

/// Closes the functions a block left open at its end, as a browser closes
/// them at the end of a stylesheet: `rgb(255,0,0` is read as `rgb(255,0,0)`.
fn close_open_functions(text: &str) -> Cow<'_, str> {
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for &byte in text.as_bytes() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b'"' | b'\'' if quote == Some(byte) => quote = None,
            b'"' | b'\'' if quote.is_none() => quote = Some(byte),
            b'(' if quote.is_none() => depth += 1,
            b')' if quote.is_none() => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    if depth == 0 {
        return Cow::Borrowed(text);
    }
    let mut closed = String::with_capacity(text.len() + depth);
    closed.push_str(text);
    for _ in 0..depth {
        closed.push(')');
    }
    Cow::Owned(closed)
}

/// Everything between `{` and its `}`.
fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0u32;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (offset, &byte) in bytes.iter().enumerate().skip(open) {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b'"' | b'\'' if quote == Some(byte) => quote = None,
            b'"' | b'\'' if quote.is_none() => quote = Some(byte),
            b'{' if quote.is_none() => depth += 1,
            b'}' if quote.is_none() => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Reads the comments that describe custom properties and the sections they
/// sit under, as many Discord themes write them.
fn collect_notes(text: &str, notes: &mut BTreeMap<String, Note>) {
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut line = 0usize;
    let mut depth = 0u32;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut last_prop: Option<(String, usize)> = None;
    let mut section: Option<String> = None;
    let mut pending: Option<String> = None;
    let mut significant = 0u8;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active {
                quote = None;
            } else if byte == b'\n' {
                line += 1;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' | b'\'' => {
                quote = Some(byte);
                significant = byte;
                index += 1;
            }
            b'\n' => {
                line += 1;
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let start_line = line;
                index += 2;
                let body_start = index;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    if bytes[index] == b'\n' {
                        line += 1;
                    }
                    index += 1;
                }
                let body = text.get(body_start..index).unwrap_or("");
                index = (index + 2).min(bytes.len());
                let note = comment_note(body);
                if let Some((name, prop_line)) = &last_prop
                    && *prop_line == start_line
                {
                    if let Some(description) = note.description {
                        notes.entry(name.clone()).or_default().description = Some(description);
                    }
                    if let Some(default) = note.default_value {
                        notes.entry(name.clone()).or_default().default_value = Some(default);
                    }
                    continue;
                }
                if let Some(group) = note.group {
                    section = Some(group.clone());
                    pending = Some(group);
                } else if let Some(description) = note.description {
                    pending = Some(description);
                }
                continue;
            }
            b'{' => {
                depth += 1;
                significant = b'{';
                index += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    section = None;
                    pending = None;
                    last_prop = None;
                }
                significant = b'}';
                index += 1;
            }
            b';' => {
                significant = b';';
                index += 1;
            }
            b'-' if bytes.get(index + 1) == Some(&b'-') && matches!(significant, b'{' | b';') => {
                if let Some((name, after)) = property_at(text, index) {
                    let note = notes.entry(name.clone()).or_default();
                    if note.group.is_none()
                        && let Some(group) = &section
                    {
                        note.group = Some(group.clone());
                    }
                    if note.description.is_none()
                        && let Some(description) = pending.take()
                    {
                        note.description = Some(description);
                    }
                    last_prop = Some((name, line));
                    index = after;
                    significant = b':';
                    continue;
                }
                index += 1;
            }
            byte if byte.is_ascii_whitespace() => index += 1,
            byte => {
                significant = byte;
                index += 1;
            }
        }
    }
}

/// The custom property starting at `start`, with the byte after its colon.
fn property_at(text: &str, start: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    let mut index = start + 2;
    let name_start = index;
    while index < bytes.len()
        && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'-' | b'_'))
    {
        index += 1;
    }
    if index == name_start {
        return None;
    }
    let name = &text[name_start..index];
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    (bytes.get(index) == Some(&b':')).then(|| (format!("--{name}"), index + 1))
}

/// What one `/* ... */` comment says: a description, a section, or both.
fn comment_note(body: &str) -> Note {
    let words = body
        .lines()
        .map(|line| line.trim().trim_start_matches('*').trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let text = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = text
        .trim_matches(|character: char| matches!(character, '-' | '!' | '/' | ' '))
        .trim();
    let text = text.trim_end_matches(':').trim();
    if text.is_empty() {
        return Note::default();
    }
    let description = description_text(text);
    let group = section_text(text);
    let default_value = comment_default(text);
    Note {
        description,
        group,
        default_value,
    }
}

/// The value a comment writes after `[default:` or `- default:`.
fn comment_default(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let mut start = None;
    for marker in ["[default:", "- default:", " default:"] {
        if let Some(index) = lower.find(marker) {
            start = Some(index + marker.len());
            break;
        }
    }
    let rest = &text[start?..];
    let rest = rest
        .split([']', ';'])
        .next()
        .unwrap_or(rest)
        .split(" - ")
        .next()
        .unwrap_or(rest)
        .trim();
    let value = rest.trim_matches(['"', '\'', ' ']);
    (!value.is_empty() && !value.eq_ignore_ascii_case("default")).then(|| value.to_owned())
}

fn description_text(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("change to")
        || lower.starts_with("default")
        || lower.starts_with("note")
        || lower.starts_with("todo")
        || lower.starts_with("important")
        || lower.starts_with('@')
    {
        return None;
    }
    let mut text = text;
    for marker in ["[default", " - default", " - hex", " default:"] {
        if let Some(index) = text.to_ascii_lowercase().find(marker) {
            text = &text[..index];
        }
    }
    let text = text.trim_end_matches([' ', ':', '-', ',']).trim();
    if text.is_empty() {
        return None;
    }
    let mut text = text.to_owned();
    if text.chars().count() > 240 {
        text = text.chars().take(240).collect::<String>();
        text.push('…');
    }
    Some(text)
}

fn section_text(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if text.chars().count() > 60
        || text.chars().count() < 2
        || lower.starts_with("note")
        || lower.starts_with("todo")
        || lower.starts_with("change")
        || lower.starts_with("default")
        || lower.starts_with("important")
        || lower.contains("http")
        || lower.contains(".css")
        || lower.contains("import")
        || lower.contains("settings")
        || lower.contains("tokens")
        || lower.contains("palette")
        || text.contains("--")
        || text.chars().any(|character| {
            matches!(
                character,
                '@' | ';' | '{' | '}' | '=' | '[' | ']' | ':' | '(' | ')'
            )
        })
    {
        return None;
    }
    let mut group = text
        .trim_matches(|character: char| matches!(character, '.' | ':' | '-' | ' '))
        .to_owned();
    if group.is_empty() {
        return None;
    }
    // `APP BACKGROUND` and `text colors` both read as a heading.
    if group
        .chars()
        .filter(|character| character.is_alphabetic())
        .all(|character| !character.is_lowercase())
    {
        let mut characters = group.chars();
        if let Some(first) = characters.next() {
            group = first.to_uppercase().collect::<String>() + &characters.as_str().to_lowercase();
        }
    } else if group
        .chars()
        .next()
        .is_some_and(|character| character.is_lowercase())
    {
        let mut characters = group.chars();
        if let Some(first) = characters.next() {
            group = first.to_uppercase().collect::<String>() + characters.as_str();
        }
    }
    Some(group)
}

/// The declarations of a block, each name and value as written.
/// Reads the steps of a `@keyframes` block. The selectors are `from`, `to`
/// or percentages, possibly several to a block.
fn read_keyframes(name: &str, body: &str) -> Keyframes {
    let mut frames = Vec::new();
    let bytes = body.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let start = index;
        let mut depth = 0u32;
        let mut quote: Option<u8> = None;
        let mut escaped = false;
        let mut open = None;
        while index < bytes.len() {
            let byte = bytes[index];
            if escaped {
                escaped = false;
                index += 1;
                continue;
            }
            match byte {
                b'\\' => escaped = true,
                b'"' | b'\'' if quote == Some(byte) => quote = None,
                b'"' | b'\'' if quote.is_none() => quote = Some(byte),
                b'(' if quote.is_none() => depth += 1,
                b')' if quote.is_none() => depth = depth.saturating_sub(1),
                b'{' if quote.is_none() && depth == 0 => {
                    open = Some(index);
                    break;
                }
                _ => {}
            }
            index += 1;
        }
        let Some(brace) = open else {
            break;
        };
        let selector = body[start..brace].trim();
        let Some(close) = matching_brace(body, brace) else {
            break;
        };
        let steps = declarations(&body[brace + 1..close]);
        for offset in crate::color::split_top(selector, ',') {
            let offset = offset.trim().to_ascii_lowercase();
            let offset = if offset == "from" {
                Some(0.0)
            } else if offset == "to" {
                Some(1.0)
            } else {
                offset
                    .strip_suffix('%')
                    .and_then(|value| value.trim().parse::<f32>().ok())
                    .map(|value| (value / 100.0).clamp(0.0, 1.0))
            };
            if let Some(offset) = offset {
                frames.push(Keyframe {
                    offset,
                    declarations: steps.clone(),
                });
            }
        }
        index = close + 1;
    }
    frames.sort_by(|left, right| left.offset.total_cmp(&right.offset));
    Keyframes {
        name: name.to_owned(),
        frames,
    }
}

fn declarations(body: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    for piece in crate::color::split_top(body, ';') {
        if piece.trim().is_empty() {
            continue;
        }
        let Some((name, value)) = piece.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let mut value = value.trim();
        let mut important = false;
        let lower = value.to_ascii_lowercase();
        for suffix in ["!important", "! important"] {
            if lower.ends_with(suffix) {
                value = value[..value.len() - suffix.len()].trim_end();
                important = true;
                break;
            }
        }
        declarations.push(Declaration {
            name: name.to_owned(),
            value: value.to_owned(),
            important,
        });
    }
    declarations
}

fn font_face(body: &str, origin: Option<&str>) -> Option<FontFace> {
    let mut family = String::new();
    let mut weight = 400;
    let mut italic = false;
    let mut data = None;
    let mut source = String::new();
    for declaration in declarations(body) {
        match declaration.name.to_ascii_lowercase().as_str() {
            "font-family" => family = unquote(declaration.value.trim()),
            "font-weight" => {
                let value = declaration.value.trim().to_ascii_lowercase();
                weight = match value.as_str() {
                    "normal" => 400,
                    "bold" => 700,
                    _ => value
                        .split_whitespace()
                        .next()
                        .and_then(|value| value.parse::<u16>().ok())
                        .unwrap_or(400),
                };
            }
            "font-style" => {
                let value = declaration.value.trim().to_ascii_lowercase();
                italic = value.starts_with("italic") || value.starts_with("oblique");
            }
            "src" => {
                let mut fallback = String::new();
                for candidate in crate::color::split_top(&declaration.value, ',') {
                    let candidate = candidate.trim();
                    if let Some(bytes) = data_uri(candidate) {
                        data = Some(bytes);
                        candidate.clone_into(&mut source);
                        break;
                    }
                    if fallback.is_empty() {
                        candidate.clone_into(&mut fallback);
                    }
                    if source.is_empty() && find_ignore_case(candidate, "url(").is_some() {
                        candidate.clone_into(&mut source);
                    }
                }
                if source.is_empty() {
                    source = fallback;
                }
            }
            _ => {}
        }
    }
    // A face without a family can never be selected; the CSS is invalid and
    // the reader leaves it out rather than carrying an empty name.
    if family.is_empty() {
        return None;
    }
    Some(FontFace {
        family,
        weight,
        italic,
        data,
        source,
        origin: origin.map(str::to_owned),
    })
}

/// The bytes of a `data:` URI, when it carries them.
fn data_uri(text: &str) -> Option<Vec<u8>> {
    let start = find_ignore_case(text, "url(")?;
    let rest = &text[start + 4..];
    let end = rest.find(')')?;
    let inside = unquote(&rest[..end]);
    let inside = inside.strip_prefix("data:")?;
    let (meta, data) = inside.split_once(',')?;
    if meta.contains(";base64") {
        crate::base64::decode(data.trim())
    } else {
        Some(percent_decode(data))
    }
}

fn unquote(text: &str) -> String {
    let text = text.trim();
    if text.len() >= 2 {
        let bytes = text.as_bytes();
        if (bytes[0] == b'"' && bytes[text.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[text.len() - 1] == b'\'')
        {
            return text[1..text.len() - 1]
                .replace("\\\"", "\"")
                .replace("\\'", "'");
        }
    }
    text.to_owned()
}

fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (
                (bytes[index + 1] as char).to_digit(16),
                (bytes[index + 2] as char).to_digit(16),
            )
        {
            decoded.push(((high << 4) | low) as u8);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    decoded
}

/// Removes comments, keeping line breaks and string contents.
/// `text` without its comments, and whether one was left open.
fn strip_comments(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut quote: Option<char> = None;
    let mut open = false;
    while let Some(character) = chars.next() {
        if let Some(active) = quote {
            out.push(character);
            if character == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
                continue;
            }
            if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => {
                quote = Some(character);
                out.push(character);
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut closed = false;
                while let Some(character) = chars.next() {
                    if character == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        closed = true;
                        break;
                    }
                    if character == '\n' {
                        out.push('\n');
                    }
                }
                open |= !closed;
            }
            _ => out.push(character),
        }
    }
    (out, open)
}

/// `prefers-color-scheme: dark` and its light sibling in a media query.
fn media_scheme(prelude: &str) -> Option<bool> {
    let query = media_query_text(prelude);
    let mut required = None;
    for branch in crate::color::split_top(query, ',') {
        if !media_query_could_apply(branch) {
            continue;
        }
        let scheme = media_query_scheme(branch)?;
        match required {
            None => required = Some(scheme),
            Some(known) if known == scheme => {}
            Some(_) => return None,
        }
    }
    required.flatten()
}

/// The appearance one media query asks for; `None` when it applies to both.
fn media_query_scheme(query: &str) -> Option<Option<bool>> {
    let lower = query.to_ascii_lowercase();
    let at = lower.find("prefers-color-scheme")?;
    let after = &lower[at + "prefers-color-scheme".len()..];
    let after = after.trim_start().strip_prefix(':')?.trim_start();
    let (value, _) = split_first_word(after);
    let value = value.trim_matches([')', ';', ' ']);
    let dark = match value {
        "dark" => true,
        "light" => false,
        _ => return Some(None),
    };
    let prefix = lower[..at].trim_end();
    let prefix = prefix.strip_suffix('(').unwrap_or(prefix).trim_end();
    let negated = prefix
        .rsplit_once(' ')
        .is_some_and(|(_, word)| word.eq_ignore_ascii_case("not"))
        || prefix.eq_ignore_ascii_case("not");
    Some(Some(dark != negated))
}

/// The largest a value may grow to while its `var()`s are filled in. A
/// colour or a picture is far below this; a theme that reaches it is broken
/// or hostile, and is read as if the value were missing.
const MAX_RESOLVED: usize = 1 << 20;

/// How many `var()`s one value may read, across all its levels.
const SUBSTITUTIONS: usize = 4096;

/// Resolves custom properties and the colours that use them.
#[derive(Clone, Debug, Default)]
pub struct Vars {
    values: BTreeMap<String, String>,
    important: BTreeMap<String, bool>,
    /// The specificity and source order that won each property.
    weight: BTreeMap<String, (u32, usize)>,
    /// The folder or web address each property came from, for the files it
    /// names relatively.
    origin: BTreeMap<String, Option<String>>,
    /// Values already filled in, so a palette and the options that name the
    /// same variables resolve each chain once.
    resolved: RefCell<HashMap<String, Option<String>>>,
}

impl Vars {
    /// Writes a declaration unconditionally, as an override the caller chose
    /// does; only an `!important` declaration in the theme can hold it off.
    pub fn insert(&mut self, declaration: &Declaration) {
        let replace = !matches!(
            (
                self.important.get(&declaration.name).copied(),
                declaration.important
            ),
            (Some(true), false)
        );
        if replace {
            self.values
                .insert(declaration.name.clone(), declaration.value.clone());
            self.important
                .insert(declaration.name.clone(), declaration.important);
            self.weight
                .insert(declaration.name.clone(), (u32::MAX, usize::MAX));
            self.origin.insert(declaration.name.clone(), None);
            self.resolved.borrow_mut().clear();
        }
    }

    /// Writes a declaration in the theme's cascade. `!important` beats
    /// specificity beats source order, as in a browser.
    pub fn insert_weighted(
        &mut self,
        declaration: &Declaration,
        specificity: u32,
        order: usize,
        origin: Option<&str>,
    ) {
        let old_important = self.important.get(&declaration.name).copied();
        let replace = match (old_important, declaration.important) {
            (Some(true), false) => false,
            (Some(false), true) | (None, true) => true,
            _ => {
                let (old_specificity, old_order) = self
                    .weight
                    .get(&declaration.name)
                    .copied()
                    .unwrap_or((0, 0));
                (specificity, order) >= (old_specificity, old_order)
            }
        };
        if replace {
            self.values
                .insert(declaration.name.clone(), declaration.value.clone());
            self.important
                .insert(declaration.name.clone(), declaration.important);
            self.weight
                .insert(declaration.name.clone(), (specificity, order));
            self.origin
                .insert(declaration.name.clone(), origin.map(str::to_owned));
            self.resolved.borrow_mut().clear();
        }
    }

    /// The folder or web address `name` came from, for the files it names
    /// relatively.
    pub fn origin(&self, name: &str) -> Option<&str> {
        self.origin.get(name).and_then(Option::as_deref)
    }

    /// The value `name` settled to, before its `var()`s are filled in.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// Whether the theme names `name` at all, whatever its value.
    pub fn contains(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    /// Every custom property, by name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    /// `value` with every `var()` replaced, or `None` when one has neither a
    /// value nor a fallback. A value without `var()` comes back borrowed, as
    /// most declarations are literals.
    pub fn resolve<'a>(&'a self, value: &'a str) -> Option<Cow<'a, str>> {
        if find_ignore_case(value, "var(").is_none() {
            return Some(Cow::Borrowed(value));
        }
        if let Some(cached) = self.resolved.borrow().get(value) {
            return cached.clone().map(Cow::Owned);
        }
        let mut left = SUBSTITUTIONS;
        let result = self.resolve_depth(value, 0, &mut left);
        if value.len() <= 4096 {
            let mut cache = self.resolved.borrow_mut();
            if cache.len() >= 1024 {
                cache.clear();
            }
            cache.insert(value.to_owned(), result.clone());
        }
        result.map(Cow::Owned)
    }

    /// The colour a declaration names, after its `var()`s are filled in.
    pub fn color(&self, name: &str, light: bool) -> Option<Rgba> {
        let value = self.resolve(self.get(name)?)?;
        parse_color(&value, light)
    }

    /// The colour of the first of `names` that resolves, after filling in
    /// `var()`s.
    pub fn first_color(&self, names: &[&str], light: bool) -> Option<Rgba> {
        names.iter().find_map(|name| self.color(name, light))
    }

    fn resolve_depth(&self, value: &str, depth: usize, left: &mut usize) -> Option<String> {
        // A theme can name the same variable twice at every level, as
        // `--a: var(--b) var(--b)`, which is 2^n substitutions for n levels.
        // The depth alone doesn't bound that, so each read also spends from
        // one budget for the whole value.
        if depth > 32 || *left == 0 || value.len() > MAX_RESOLVED {
            return None;
        }
        *left -= 1;
        let mut out = String::with_capacity(value.len());
        let mut rest = value;
        while let Some(start) = find_ignore_case(rest, "var(") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 4..];
            let Some(end) = matching_paren(after) else {
                // Not a `var()` we can read; keep it as text.
                out.push_str(&rest[start..]);
                return (out.len() <= MAX_RESOLVED).then_some(out);
            };
            let inside = &after[..end];
            let arguments = crate::color::split_top(inside, ',');
            let name = arguments.first()?.trim();
            let resolved = self
                .values
                .get(name)
                .and_then(|value| self.resolve_depth(value, depth + 1, left))
                .or_else(|| {
                    arguments
                        .get(1)
                        .and_then(|fallback| self.resolve_depth(fallback, depth + 1, left))
                });
            let resolved = resolved?;
            if out.len() + resolved.len() > MAX_RESOLVED {
                return None;
            }
            out.push_str(&resolved);
            rest = &after[end + 1..];
        }
        if out.len() + rest.len() > MAX_RESOLVED {
            return None;
        }
        out.push_str(rest);
        Some(out)
    }
}

/// The resolved custom properties a palette reads. [`Vars`] applies the
/// theme's own root cascade; the semantic reader takes the computed style the
/// CSS engine settled, so a palette can be built from either without copying
/// its values.
pub trait Variables {
    /// The value `name` settled to, before `var()` substitution.
    fn get(&self, name: &str) -> Option<&str>;

    /// Whether the theme names `name` at all.
    fn contains(&self, name: &str) -> bool;

    /// `value` with its `var()`s filled in, or `None` when one has no value.
    fn resolve<'a>(&'a self, value: &'a str) -> Option<Cow<'a, str>>;

    /// The folder or web address `name` came from, for relative files.
    fn origin(&self, _name: &str) -> Option<&str> {
        None
    }

    /// The colour `name` settles to, after `var()` substitution.
    fn color(&self, name: &str, light: bool) -> Option<Rgba> {
        let value = self.resolve(self.get(name)?)?;
        parse_color(&value, light)
    }

    /// The colour of the first of `names` that resolves.
    fn first_color(&self, names: &[&str], light: bool) -> Option<Rgba> {
        names.iter().find_map(|name| self.color(name, light))
    }
}

impl Variables for Vars {
    fn get(&self, name: &str) -> Option<&str> {
        Vars::get(self, name)
    }

    fn contains(&self, name: &str) -> bool {
        Vars::contains(self, name)
    }

    fn resolve<'a>(&'a self, value: &'a str) -> Option<Cow<'a, str>> {
        Vars::resolve(self, value)
    }

    fn origin(&self, name: &str) -> Option<&str> {
        Vars::origin(self, name)
    }

    fn color(&self, name: &str, light: bool) -> Option<Rgba> {
        Vars::color(self, name, light)
    }

    fn first_color(&self, names: &[&str], light: bool) -> Option<Rgba> {
        Vars::first_color(self, names, light)
    }
}

/// The closing parenthesis of `var(`'s arguments.
pub(crate) fn matching_paren(text: &str) -> Option<usize> {
    let mut depth = 0u32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' | '\'' if quote == Some(character) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(character),
            '(' if quote.is_none() => depth += 1,
            ')' if quote.is_none() => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Whether a selector writes variables on the page's root, and for which
/// colour scheme, with its specificity for the cascade. Descendant selectors
/// that style a part of Discord's DOM don't set the window's palette.
pub fn root_selector(selector: &str, theme_dark: bool) -> Option<bool> {
    root_selector_with_specificity(selector, theme_dark).map(|(dark, _)| dark)
}

/// [`root_selector`] with the specificity that orders two declarations of the
/// same property.
pub fn root_selector_with_specificity(selector: &str, theme_dark: bool) -> Option<(bool, u32)> {
    let selector = selector.trim();
    if selector.is_empty() || selector.contains("::") {
        return None;
    }
    // `:not(...)` names what the selector is *not*; its classes and attributes
    // must not decide the colour scheme.
    let positive = without_negations(selector);
    let mut dark = theme_dark;
    let mut saw_root = false;
    let mut saw_theme = false;
    for compound in selector_compounds(&positive) {
        let lower = compound.to_ascii_lowercase();
        if lower.contains("theme-light")
            || lower.contains("data-rl-mode=\"light\"")
            || lower.contains("data-rl-mode='light'")
        {
            dark = false;
            saw_root = true;
            saw_theme = true;
        } else if lower.contains("theme-dark")
            || lower.contains("theme-darker")
            || lower.contains("theme-midnight")
            || lower.contains("theme-onxy")
            || lower.contains("theme-ash")
            || lower.contains("data-rl-mode=\"dark\"")
            || lower.contains("data-rl-mode='dark'")
        {
            dark = true;
            saw_root = true;
            saw_theme = true;
        } else if lower.contains(":root")
            || lower.starts_with("html")
            || lower.starts_with("body")
            || lower.starts_with("#app-mount")
            || lower.starts_with(":where(html")
            || lower.starts_with(":where(:root")
            || lower.starts_with(":is(html")
            || lower.starts_with(":is(:root")
        {
            saw_root = true;
        } else {
            return None;
        }
    }
    if !saw_theme {
        // `html:not(.theme-dark)` is the light palette, and the light
        // selector's negative the dark one.
        let negated = selector.to_ascii_lowercase();
        let not_dark = negated.contains(":not(.theme-dark")
            || negated.contains(":not([data-rl-mode=\"dark\"")
            || negated.contains(":not([data-rl-mode='dark'");
        let not_light = negated.contains(":not(.theme-light")
            || negated.contains(":not([data-rl-mode=\"light\"")
            || negated.contains(":not([data-rl-mode='light'");
        if not_dark {
            dark = false;
        } else if not_light {
            dark = true;
        }
    }
    saw_root.then_some((dark, specificity(selector)))
}

/// `selector` without its `:not(...)` arguments, so a class inside a negation
/// can't set the palette's colour scheme.
fn without_negations(selector: &str) -> String {
    let lower = selector.to_ascii_lowercase();
    let mut out = String::with_capacity(selector.len());
    let mut index = 0;
    while index < selector.len() {
        match lower[index..].find(":not(") {
            Some(offset) => {
                let start = index + offset;
                out.push_str(&selector[index..start]);
                // `:not(` is five bytes, not four: the open parenthesis is
                // part of the prefix. Starting one byte early made
                // `matching_paren` treat the whole remainder as nested and
                // drop every class after a negation.
                let after = start + 5;
                match matching_paren(&lower[after..]) {
                    Some(end) => index = after + end + 1,
                    None => break,
                }
            }
            None => {
                out.push_str(&selector[index..]);
                break;
            }
        }
    }
    out
}

/// A rough CSS specificity: ids, classes/attributes/pseudo-classes, tags.
/// Root selectors rarely need more than ordering ties by source order.
fn specificity(selector: &str) -> u32 {
    let mut specificity = 0;
    for compound in selector_compounds(selector) {
        let bytes = compound.as_bytes();
        let mut index = 0;
        if bytes.first().is_some_and(u8::is_ascii_alphabetic) {
            specificity += 1;
        }
        while index < bytes.len() {
            match bytes[index] {
                b'#' => {
                    specificity += 100;
                    index += 1;
                }
                b'.' | b'[' => {
                    specificity += 10;
                    index += 1;
                }
                b':' if bytes.get(index + 1) != Some(&b':') => {
                    specificity += 10;
                    index += 1;
                }
                _ => index += 1,
            }
        }
    }
    specificity
}

/// Splits a selector at its combinators, keeping the pieces that matter.
pub(crate) fn selector_compounds(selector: &str) -> Vec<String> {
    let mut compounds = Vec::new();
    let mut current = String::new();
    let mut depth = 0u32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for character in selector.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' => {
                current.push(character);
                escaped = true;
            }
            '"' | '\'' if quote == Some(character) => {
                quote = None;
                current.push(character);
            }
            '"' | '\'' if quote.is_none() => {
                quote = Some(character);
                current.push(character);
            }
            '(' if quote.is_none() => {
                depth += 1;
                current.push(character);
            }
            ')' if quote.is_none() => {
                depth = depth.saturating_sub(1);
                current.push(character);
            }
            '>' | '+' | '~' if quote.is_none() && depth == 0 => {
                if !current.trim().is_empty() {
                    compounds.push(current.trim().to_owned());
                }
                current.clear();
            }
            character if character.is_whitespace() && quote.is_none() && depth == 0 => {
                if !current.trim().is_empty() {
                    compounds.push(current.trim().to_owned());
                }
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        compounds.push(current.trim().to_owned());
    }
    compounds
}

/// One BetterDiscord `@var` setting declared in a theme's header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetaVar {
    /// `checkbox`, `text`, `color`, `select` or `slider`.
    pub kind: String,
    /// The setting's name, which usually matches a custom property without
    /// its `--`, sometimes with a `_s` suffix.
    pub name: String,
    /// The label the theme gives it.
    pub label: String,
    /// Everything after the label: the default and a `select`'s choices.
    pub rest: String,
}

/// The theme's header comment, which Vencord themes use as metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    /// The theme's name, from `@name`.
    pub name: Option<String>,
    /// Who wrote it, from `@author`.
    pub author: Option<String>,
    /// Its one-line summary, from `@description`.
    pub description: Option<String>,
    /// Its version string, from `@version`.
    pub version: Option<String>,
    /// The licence it is offered under, from `@license`.
    pub license: Option<String>,
    /// Where its source lives, from `@source`.
    pub source: Option<String>,
    /// The theme's home page, from `@website`.
    pub website: Option<String>,
    /// BetterDiscord's `@var` settings, which name and explain some options.
    pub vars: Vec<MetaVar>,
}

/// The CSS before its first `{` outside comments and strings, where a
/// theme's metadata comments live.
fn before_first_rule(css: &str) -> &str {
    let bytes = css.as_bytes();
    let mut index = 0;
    let mut quote = None;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(opening) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == opening {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index < bytes.len() {
                    if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
            }
            b'"' | b'\'' => {
                quote = Some(byte);
                index += 1;
            }
            b'{' => return &css[..index],
            _ => index += 1,
        }
    }
    css
}

impl Meta {
    /// Reads the metadata comments before `css`'s first rule. Blocks may be
    /// several, and a licence banner may come before the `@name` block.
    pub fn read(css: &str) -> Self {
        let mut meta = Self::default();
        // Metadata lives in the comments before the first rule. A license
        // banner may come before the `@name` block, and a theme may write
        // several blocks, so all of them are read.
        let head = before_first_rule(css);
        let mut rest = head;
        while let Some(start) = rest.find("/*") {
            let after = &rest[start + 2..];
            let Some(end) = after.find("*/") else {
                break;
            };
            meta.read_block(&after[..end]);
            rest = &after[end + 2..];
        }
        meta
    }

    fn read_block(&mut self, header: &str) {
        for line in header.lines() {
            let line = line.trim().trim_start_matches('*').trim();
            // A theme may write several `@key value` pairs on one line; each
            // pair runs to the next ` @`.
            let mut pieces: Vec<String> = line
                .split(" @")
                .map(|piece| piece.trim().to_owned())
                .collect();
            for piece in pieces.iter_mut().skip(1) {
                piece.insert(0, '@');
            }
            for piece in pieces {
                let Some((key, value)) = piece.split_once(char::is_whitespace) else {
                    continue;
                };
                let key = key.trim_start_matches('@').to_ascii_lowercase();
                let value = value.trim().to_owned();
                if value.is_empty() {
                    continue;
                }
                match key.as_str() {
                    "name" => self.name = Some(value),
                    "author" => self.author = Some(value),
                    "description" => self.description = Some(value),
                    "version" => self.version = Some(value),
                    "license" => self.license = Some(value),
                    "source" => self.source = Some(value),
                    "website" => self.website = Some(value),
                    "var" => {
                        if let Some(var) = MetaVar::parse(&value) {
                            self.vars.push(var);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

impl MetaVar {
    /// `@var text accentcolor "Accent colour" #5865f2`: its kind, name and
    /// quoted label, with the default and choices after them.
    fn parse(text: &str) -> Option<Self> {
        let rest = text.trim_start();
        let (kind, rest) = take_word(rest)?;
        let (name, rest) = take_word(rest)?;
        let (label, rest) = take_quoted(rest).unwrap_or_else(|| (name.clone(), rest.trim()));
        Some(Self {
            kind: kind.to_ascii_lowercase(),
            name,
            label,
            rest: rest.trim().to_owned(),
        })
    }
}

/// The first whitespace-separated word and what follows it.
fn take_word(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    (end > 0).then(|| (text[..end].to_owned(), &text[end..]))
}

/// The quoted text at the start of `text` and what follows its closing quote.
fn take_quoted(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    let quote = text
        .chars()
        .next()
        .filter(|quote| matches!(quote, '"' | '\''))?;
    let rest = text.get(quote.len_utf8()..)?;
    let mut escaped = false;
    for (index, character) in rest.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            character if character == quote => {
                let label = rest[..index].replace("\\\"", "\"").replace("\\'", "'");
                return Some((label, &rest[index + character.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_resolve_through_each_other_and_take_fallbacks() {
        let mut vars = Vars::default();
        vars.insert(&Declaration {
            name: "--a".into(),
            value: "color-mix(in srgb, var(--b) 50%, transparent)".into(),
            important: false,
        });
        vars.insert(&Declaration {
            name: "--b".into(),
            value: "#e94221".into(),
            important: false,
        });
        assert_eq!(
            vars.color("--a", true),
            Some(Rgba::rgba(0xe9, 0x42, 0x21, 128))
        );
        assert_eq!(
            vars.resolve("var(--missing, 12px)").as_deref(),
            Some("12px")
        );
        assert_eq!(vars.resolve("var(--missing)"), None);
    }

    #[test]
    fn a_value_that_doubles_itself_stops_at_its_budget() {
        // `--v63` is 2^63 leaves if every level copies its parent twice.
        let mut vars = Vars::default();
        vars.insert(&Declaration {
            name: "--v0".into(),
            value: "#111".into(),
            important: false,
        });
        for step in 1..64 {
            vars.insert(&Declaration {
                name: format!("--v{step}"),
                value: format!("var(--v{}) var(--v{})", step - 1, step - 1),
                important: false,
            });
        }
        let started = std::time::Instant::now();
        assert_eq!(vars.resolve("var(--v63)"), None, "too large to fill in");
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        // A reference loop also ends.
        vars.insert(&Declaration {
            name: "--loop".into(),
            value: "var(--loop)".into(),
            important: false,
        });
        assert_eq!(vars.resolve("var(--loop)"), None);
    }

    #[test]
    fn important_declarations_win_over_later_ones() {
        let mut vars = Vars::default();
        vars.insert(&Declaration {
            name: "--x".into(),
            value: "red".into(),
            important: true,
        });
        vars.insert(&Declaration {
            name: "--x".into(),
            value: "blue".into(),
            important: false,
        });
        assert_eq!(vars.get("--x"), Some("red"));
    }

    #[test]
    fn rules_keep_their_media_colour_scheme() {
        let sheet = parse(
            "@media (prefers-color-scheme: light) { :root { --x: #fff; } } :root { --y: #000; }",
            |_, _| None,
        );
        assert_eq!(sheet.rules.len(), 2);
        assert_eq!(sheet.rules[0].color_scheme, Some(false));
        assert_eq!(sheet.rules[1].color_scheme, None);
    }

    #[test]
    fn negated_media_conditions_flip_their_sense() {
        let sheet = parse(
            "@media not (prefers-color-scheme: dark) { :root { --x: #fff; } } @media not print { :root { --y: #000; } }",
            |_, _| None,
        );
        assert_eq!(sheet.rules.len(), 2);
        assert_eq!(sheet.rules[0].color_scheme, Some(false));
    }

    #[test]
    fn media_queries_measure_the_reference_window() {
        let sheet = parse(
            r#"
            @media (max-width: 600px) { .narrow { --x: 1; } }
            @media (min-width: 600px) and (orientation: landscape) { .wide { --x: 2; } }
            @media (pointer: coarse) { .touch { --x: 3; } }
            @media (prefers-reduced-motion: reduce) { .still { --x: 4; } }
            @media (hover: hover) { .mouse { --x: 5; } }
            "#,
            |_, _| None,
        );
        let selected: Vec<&str> = sheet
            .rules
            .iter()
            .map(|rule| rule.selectors[0].as_str())
            .collect();
        assert_eq!(selected, [".wide", ".mouse"]);
    }

    #[test]
    fn a_range_media_query_measures_the_reference_window() {
        // MQ4 §2.4: `<=`, `>=`, `<`, `>` and `=`, with or without spaces
        // beside them, and either side naming the feature.
        let applies = |query: &str| media_applies(&format!("({query})"));
        assert!(applies("width >= 1000px"));
        assert!(applies("width>=1000px"));
        assert!(applies("1000px <= width"));
        assert!(applies("1280px = width"));
        assert!(applies("400px <= width <= 2000px"));
        assert!(applies("2000px >= width >= 400px"));
        assert!(applies("400px < width < 2000px"));
        assert!(!applies("width < 1000px"));
        assert!(!applies("width <= 1000px"));
        assert!(!applies("1000px >= width"));
        assert!(!applies("width > 2000px"));
        assert!(!applies("400px <= width <= 1000px"));
        assert!(!applies("1000px >= width >= 400px"));
        assert!(applies("min-height <= 720px"));
        assert!(applies("height <= 720px"));
        assert!(!applies("height <= 100px"));
        // A condition with no feature, or an operator without a side, is no
        // condition and keeps a theme's rule out.
        assert!(!applies("<= 1000px"));
        assert!(!applies("width <="));
        assert!(!applies("bogus-feature >= 1"));
    }

    #[test]
    fn a_layer_statement_orders_layers_before_their_blocks() {
        let theme = crate::load_str(
            r#"
            @layer theme, base;
            @layer base { :root { --background-primary: #222222; } }
            @layer theme { :root { --background-primary: #111111; } }
            "#,
            None,
            &crate::Options::with_mode(crate::Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.palette.chat, Rgba::rgb(0x22, 0x22, 0x22));
    }

    #[test]
    fn a_container_style_query_keeps_its_rules_for_later() {
        let sheet = parse(
            "@container style(--mode: on) { :root { --x: 1; } } \
             @container not style(--mode: on) { :root { --y: 2; } }",
            |_, _| None,
        );
        assert_eq!(sheet.rules.len(), 2);
        assert_eq!(sheet.rules[0].containers, ["style(--mode: on)"]);
        assert_eq!(sheet.rules[1].containers, ["not style(--mode: on)"]);
    }

    #[test]
    fn font_faces_carry_their_data_uri() {
        let sheet = parse(
            r#"@font-face { font-family: "Test"; font-weight: 700; src: url("data:font/ttf;base64,AAEC") format("truetype"); }"#,
            |_, _| None,
        );
        assert_eq!(sheet.fonts.len(), 1);
        assert_eq!(sheet.fonts[0].family, "Test");
        assert_eq!(sheet.fonts[0].weight, 700);
        assert_eq!(sheet.fonts[0].data.as_deref(), Some(&[0, 1, 2][..]));
    }

    #[test]
    fn an_unclosed_comment_is_named() {
        let sheet = parse("a { color: red; } /* never closed", |_, _| None);
        assert_eq!(sheet.rules.len(), 1);
        assert!(
            sheet
                .warnings
                .iter()
                .any(|warning| warning.contains("comment is never closed")),
            "{:?}",
            sheet.warnings
        );
    }

    #[test]
    fn a_font_face_without_a_family_is_skipped() {
        let sheet = parse(
            r#"@font-face { src: url("data:font/ttf;base64,AAEC"); }
               @font-face { font-family: ""; src: url("data:font/ttf;base64,AAEC"); }"#,
            |_, _| None,
        );
        assert!(sheet.fonts.is_empty());
        assert!(
            sheet
                .warnings
                .iter()
                .filter(|warning| warning.contains("@font-face"))
                .count()
                >= 2,
            "{:?}",
            sheet.warnings
        );
    }

    #[test]
    fn a_selector_list_with_an_invalid_member_drops_the_rule() {
        let sheet = parse(
            ".a, :bogus-pseudo { color: red } .b { color: blue } .c, { color: green }",
            |_, _| None,
        );
        let selectors: Vec<&str> = sheet
            .rules
            .iter()
            .map(|rule| rule.selectors[0].as_str())
            .collect();
        assert_eq!(selectors, [".b"]);
        assert!(
            sheet
                .warnings
                .iter()
                .filter(|warning| warning.contains("not valid"))
                .count()
                >= 2,
            "{:?}",
            sheet.warnings
        );

        // A pseudo-element CSS defines is valid syntax even where the window
        // cannot paint it, so its rule stays.
        let sheet = parse(".a:checked, .b::marker { color: red }", |_, _| None);
        assert_eq!(sheet.rules.len(), 1);
    }

    #[test]
    fn a_media_feature_without_a_value_matches_nothing() {
        for query in [
            "(min-width:)",
            "(max-width:)",
            "(min-height:)",
            "(max-height:)",
        ] {
            assert!(!media_applies(query), "{query} applies");
        }
        assert!(media_applies("(min-width: 1000px)"));
        assert!(media_applies("(max-height: 800px)"));
    }

    #[test]
    fn only_root_selectors_count_for_the_palette() {
        assert_eq!(root_selector(":root", true), Some(true));
        assert_eq!(
            root_selector("html.theme-light:not([data-x])", true),
            Some(false)
        );
        assert_eq!(root_selector(".theme-dark", false), Some(true));
        assert_eq!(root_selector(".theme-dark .container", true), None);
        assert_eq!(root_selector("body", true), Some(true));
        assert_eq!(root_selector("#app-mount", true), Some(true));
        assert_eq!(root_selector(".container", true), None);
        assert_eq!(root_selector("html:not(.theme-light)", true), Some(true));
        assert_eq!(root_selector("html:not(.theme-dark)", true), Some(false));
        assert_eq!(
            root_selector("html:not([data-rl-mode=\"dark\"])", true),
            Some(false)
        );
        assert_eq!(
            root_selector("html:not(.custom).theme-dark", true),
            Some(true)
        );
        assert_eq!(
            root_selector("html:not(.custom).theme-light", true),
            Some(false)
        );
        assert_eq!(root_selector(":where(.theme-light)", true), Some(false));
    }

    #[test]
    fn comments_keep_their_utf8_text() {
        let (text, open) = strip_comments("/* café */ a{content:\"café…\";color:#fff}");
        assert!(text.contains("café"), "{text}");
        assert!(text.contains("\"café…\""), "{text}");
        assert!(!open);
    }

    #[test]
    fn header_vars_are_read_with_their_labels() {
        let meta = Meta::read(
            "/**\n * @name DiscordRecolor\n * @var checkbox settingsicons_s \"User Settings Icons\" 1\n * @var text accentcolor_s \"Blurple Color: [default] = 88, 101, 242\" 88, 101, 242\n * @var select corner \"Corners\" round {Round: round, Square: square}\n */\n:root{}",
        );
        assert_eq!(meta.vars.len(), 3);
        assert_eq!(meta.vars[0].kind, "checkbox");
        assert_eq!(meta.vars[0].name, "settingsicons_s");
        assert_eq!(meta.vars[0].label, "User Settings Icons");
        assert_eq!(
            meta.vars[1].label,
            "Blurple Color: [default] = 88, 101, 242"
        );
        assert_eq!(meta.vars[2].rest, "round {Round: round, Square: square}");
    }

    #[test]
    fn a_license_banner_does_not_hide_the_header() {
        let meta = Meta::read(
            "/* Copyright 2026 Someone, MIT licensed */\n/**\n * @name After Dark\n * @author Someone\n */\n:root{}",
        );
        assert_eq!(meta.name.as_deref(), Some("After Dark"));
        assert_eq!(meta.author.as_deref(), Some("Someone"));
    }

    #[test]
    fn comments_keep_descriptions_and_sections_for_options() {
        let sheet = parse(
            r#"
            :root {
                /* text colors */
                --text-main: #ffffff; /* normal text */
                --text-muted: #888888; /* quieter text */
                /* accent colors */
                --accent: #ff0000; /* buttons and links */
            }
            "#,
            |_, _| None,
        );
        let main = &sheet.notes["--text-main"];
        assert_eq!(main.group.as_deref(), Some("Text colors"));
        assert_eq!(main.description.as_deref(), Some("normal text"));
        let accent = &sheet.notes["--accent"];
        assert_eq!(accent.group.as_deref(), Some("Accent colors"));
        assert_eq!(accent.description.as_deref(), Some("buttons and links"));
    }

    #[test]
    fn supports_conditions_that_fail_are_skipped() {
        let sheet = parse(
            r#"
            @supports not (color: color-mix(in srgb, red, blue)) {
                :root { --no: #fff; }
            }
            @supports (color: color-mix(in srgb, red, blue)) {
                :root { --yes: #000; }
            }
            "#,
            |_, _| None,
        );
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].declarations[0].name, "--yes");
    }

    #[test]
    fn a_supports_condition_asks_what_the_engine_can_read() {
        let applies = |condition: &str| supports_value(condition);
        // A declaration the engine reads, or the reader reduces, is there.
        assert!(applies("(display: grid)"));
        assert!(applies("(color: color-mix(in srgb, red, blue))"));
        assert!(applies("(color: var(--brand, red))"));
        assert!(applies("(--brand: red)"));
        assert!(applies("(box-shadow: 0 1px 2px #00000080)"));
        // A property or a value it does not know is not, and `not` takes the
        // other branch, which is what a theme's fallback expects.
        assert!(!applies("(bogus-prop: 1)"));
        assert!(!applies("(display: bogus)"));
        assert!(applies("not (bogus-prop: 1)"));
        assert!(!applies("(display: grid) and (color: bogus)"));
        assert!(applies("(display: grid) or (color: bogus)"));
        // What this window cannot paint stays out, as the docs say.
        assert!(!applies("(backdrop-filter: blur(2px))"));
        assert!(applies("not (backdrop-filter: blur(2px))"));
    }

    #[test]
    fn supports_reads_the_selector_features_the_window_can_match() {
        let sheet = parse(
            r#"
            @supports selector(:has(.child)) { .has { --x: 1; } }
            @supports selector(:nth-child(2)) { .nth { --x: 2; } }
            @supports selector(:bogus-pseudo) { .bogus { --x: 3; } }
            "#,
            |_, _| None,
        );
        let selectors: Vec<&str> = sheet
            .rules
            .iter()
            .map(|rule| rule.selectors[0].as_str())
            .collect();
        assert_eq!(selectors, [".has", ".nth"]);
    }

    #[test]
    fn a_supports_selector_validates_every_pseudo_name() {
        // ReCore's matcher knows these, and `selector()` asks about the
        // matcher rather than about the browser's full selector syntax.
        assert!(supports_value("selector(:has(.x))"));
        assert!(supports_value("selector(:is(.a, .b):hover)"));
        assert!(supports_value("selector(:nth-child(2n + 1))"));
        assert!(supports_value("selector(::selection)"));
        assert!(supports_value("selector(::-webkit-scrollbar-thumb:hover)"));
        assert!(supports_value("selector([data-x=\"a:b\"])"));
        // A name the list does not hold makes the whole selector unsupported.
        assert!(!supports_value("selector(:bogus-pseudo)"));
        assert!(!supports_value("selector(:not(:bogus-pseudo))"));
        assert!(!supports_value("selector(::bogus-element)"));
        assert!(!supports_value("selector()"));
    }

    #[test]
    fn supports_unwraps_a_grouped_condition() {
        assert!(supports_value(
            "((display: grid) and (not (display: bogus)))"
        ));
        assert!(supports_value("((display: grid) or (display: bogus))"));
        assert!(!supports_value("((display: bogus) and (color: red))"));
        assert!(supports_value("(not (display: bogus))"));
        assert!(supports_value("((display: grid))"));
    }

    #[test]
    fn a_media_condition_without_a_term_does_not_apply() {
        for query in [
            "screen and",
            "only screen and",
            "not screen and",
            "screen and ",
            "(min-width: 1000px) and",
            "(min-width: 1000px) or",
            "(min-width: 1000px) and ",
            "screen and (min-width: 1000px) and",
            "screen and (min-width: 1000px) or",
            "and (min-width: 1000px)",
            "()",
            "( )",
            "(min-width:)",
            "(min-width: )",
        ] {
            assert!(!media_applies(query), "'{query}' applies");
        }
        // A feature's value is not an operator even when spelled like one.
        assert!(media_applies("(bogus-feature: and)"));
        assert!(media_applies("screen and (min-width: 1000px)"));
        assert!(media_applies("only screen and (min-width: 1000px)"));
        assert!(!media_applies("not screen and (min-width: 1000px)"));
    }

    #[test]
    fn an_import_after_a_rule_is_ignored() {
        let mut read = Vec::new();
        let sheet = parse(
            ":root { --x: 1; } @import \"late.css\"; :root { --y: 2; }",
            |_, path| {
                read.push(path.to_owned());
                Some((":root { --late: 3; }".to_owned(), PathBuf::from(path)))
            },
        );
        assert!(read.is_empty(), "{read:?}");
        assert!(sheet.rules.iter().all(|rule| {
            rule.declarations
                .iter()
                .all(|declaration| declaration.name != "--late")
        }));
        assert!(
            sheet
                .warnings
                .iter()
                .any(|warning| warning.contains("an @import after a rule is ignored")),
            "{:?}",
            sheet.warnings
        );

        // An import at the start of a sheet, and one at the start of the sheet
        // it imports, are both in the preamble and are read.
        let mut read = Vec::new();
        let sheet = parse("@import \"first.css\"; :root { --x: 1; }", |_, path| {
            read.push(path.to_owned());
            let text = if path == "first.css" {
                "@import \"nested.css\"; :root { --first: 1; }"
            } else {
                ":root { --nested: 1; }"
            };
            Some((text.to_owned(), PathBuf::from(path)))
        });
        assert_eq!(read, ["first.css", "nested.css"]);
        assert_eq!(sheet.rules.len(), 3);
        assert!(
            !sheet
                .warnings
                .iter()
                .any(|warning| warning.contains("ignored"))
        );
    }

    #[test]
    fn a_function_left_open_is_closed_at_the_end_of_the_file() {
        let sheet = parse(
            "[class*=\"sidebar_\"]{background:#ff0000}\
             [class*=\"chat_\"]{background:rgb(0,255,0",
            |_, _| None,
        );
        assert_eq!(sheet.rules.len(), 2);
        assert_eq!(sheet.rules[0].declarations[0].value, "#ff0000");
        assert_eq!(sheet.rules[1].declarations[0].name, "background");
        assert_eq!(sheet.rules[1].declarations[0].value, "rgb(0,255,0)");
    }

    #[test]
    fn a_function_left_open_does_not_eat_the_previous_declaration() {
        let sheet = parse(
            "[class*=\"sidebar_\"]{color:#00ff00;background:rgb(255,0,0}",
            |_, _| None,
        );
        assert_eq!(sheet.rules.len(), 1);
        let declarations: Vec<(&str, &str)> = sheet.rules[0]
            .declarations
            .iter()
            .map(|declaration| (declaration.name.as_str(), declaration.value.as_str()))
            .collect();
        assert_eq!(
            declarations,
            [("color", "#00ff00"), ("background", "rgb(255,0,0)")]
        );
    }

    #[test]
    fn unsupported_effects_take_their_fallback() {
        let sheet = parse(
            r#"
            @supports (backdrop-filter: blur(8px)) {
                :root { --frosted: #fff; }
            }
            @supports not (backdrop-filter: blur(8px)) {
                :root { --opaque: #000; }
            }
            "#,
            |_, _| None,
        );
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].declarations[0].name, "--opaque");
    }

    #[test]
    fn import_conditions_that_do_not_apply_are_skipped() {
        let mut seen = Vec::new();
        parse(
            r#"
            @import "print.css" print;
            @import "screen.css" screen and (min-width: 10px);
            @import "fallback.css" supports(not (color: color-mix(in srgb, red, blue)));
            "#,
            |_, path| {
                seen.push(path.to_owned());
                Some((String::new(), PathBuf::from(path)))
            },
        );
        assert_eq!(seen, vec!["screen.css"]);
    }

    #[test]
    fn nested_rules_are_flattened_with_their_parent() {
        let sheet = parse(
            r#"
            .a {
                color: red;
                content: "{";
                &:hover { color: blue; }
                &.b { color: green; }
                @media (min-width: 1px) {
                    .c & { color: gold; }
                }
            }
            "#,
            |_, _| None,
        );
        let selectors: Vec<&str> = sheet
            .rules
            .iter()
            .map(|rule| rule.selectors[0].as_str())
            .collect();
        assert_eq!(selectors, [".a", ".a:hover", ".a.b", ".c .a"]);
        assert_eq!(sheet.rules[0].declarations.len(), 2);
        assert_eq!(sheet.rules[1].declarations[0].value, "blue");
        assert_eq!(sheet.rules[3].declarations[0].value, "gold");
    }

    #[test]
    fn an_import_can_name_a_layer_and_a_media_query() {
        let sheet = parse(
            "@import \"theme.css\" layer(theme) screen and (min-width: 600px);",
            |_, path| {
                Some((
                    ":root { --background-primary: #123456; }".to_owned(),
                    PathBuf::from(path),
                ))
            },
        );
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(
            sheet.layers[sheet.rules[0].layer.unwrap() as usize],
            "theme"
        );
        assert_eq!(sheet.rules[0].color_scheme, None);
    }

    #[test]
    fn property_rules_keep_their_syntax_and_initial_value() {
        let sheet = parse(
            r#"
            @property --brand {
                syntax: "<color>";
                inherits: false;
                initial-value: #123456;
            }
            "#,
            |_, _| None,
        );
        let spec = &sheet.properties["--brand"];
        assert_eq!(spec.syntax.as_deref(), Some("<color>"));
        assert_eq!(spec.initial.as_deref(), Some("#123456"));
    }

    #[test]
    fn comments_keep_the_default_they_name() {
        let sheet = parse(
            ":root { --main-color: #2780e6; /* main accent [default: #2780e6] */ }",
            |_, _| None,
        );
        assert_eq!(
            sheet.notes["--main-color"].default_value.as_deref(),
            Some("#2780e6")
        );
    }

    #[test]
    fn header_comments_are_metadata() {
        let meta = Meta::read(
            "/**\n * @name ReCord\n * @author leriart\n * @description Terminal\n */\n:root{}",
        );
        assert_eq!(meta.name.as_deref(), Some("ReCord"));
        assert_eq!(meta.author.as_deref(), Some("leriart"));
        assert_eq!(meta.description.as_deref(), Some("Terminal"));
    }

    #[test]
    fn comments_are_stripped_but_strings_are_not() {
        let (text, open) = strip_comments("a{content:\"/*x*/\";/*y*/color:red}");
        assert!(text.contains("\"/*x*/\""));
        assert!(!text.contains("/*y*/"));
        assert!(!open);
    }

    #[test]
    fn absolute_and_viewport_units_become_pixels() {
        assert_eq!(normalize_units("9pt").as_ref(), "12px");
        assert_eq!(normalize_units("1in").as_ref(), "96px");
        assert_eq!(normalize_units("2Q").as_ref(), "1.8898px");
        assert_eq!(normalize_units("1vmin").as_ref(), "7.2px");
        assert_eq!(normalize_units("1dvh").as_ref(), "7.2px");
        assert_eq!(
            normalize_units("calc(1pt + 1px)").as_ref(),
            "calc(1.3333px + 1px)"
        );
    }

    #[test]
    fn strings_and_urls_keep_their_numbers() {
        let quoted = normalize_units("\"9pt\"");
        assert!(matches!(quoted, Cow::Borrowed(_)));
        assert_eq!(quoted.as_ref(), "\"9pt\"");
        let url = normalize_units("url(9pt.png)");
        assert!(matches!(url, Cow::Borrowed(_)));
        assert_eq!(url.as_ref(), "url(9pt.png)");
        assert_eq!(
            normalize_units("url(\"9pt.png\") 9pt").as_ref(),
            "url(\"9pt.png\") 12px"
        );
        assert_eq!(
            normalize_units("1rem 0.5em 50% 9").as_ref(),
            "1rem 0.5em 50% 9"
        );
    }

    #[test]
    fn normalize_units_borrows_text_without_a_rewritten_unit() {
        let text = "color: #ff0000; margin: 4px 2rem";
        assert!(matches!(normalize_units(text), Cow::Borrowed(_)));
        // A duration and a percentage are not lengths.
        assert_eq!(
            normalize_units("transition-duration: 0.1s").as_ref(),
            "transition-duration: 0.1s"
        );
    }
}
