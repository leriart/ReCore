//! The options a theme exposes: the custom properties the window can change
//! and the controls Appearance offers for them. The names are the theme's own,
//! so a Vencord theme's `--background-primary` or ReCord's
//! `--rl-scanline-opacity` both become a control with a readable label.

use crate::{
    Rgba,
    css::{MetaVar, Note, PropertySpec, Vars},
};

/// A control Appearance can show for one of a theme's custom properties.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeOption {
    /// The custom property, such as `--rl-accent`.
    pub name: String,
    /// A readable name for the control, taken from the comment or the property.
    pub label: String,
    /// The explanation the theme writes beside the property, when there is one.
    pub description: Option<String>,
    /// The heading it belongs under.
    pub group: String,
    /// The control that edits it and the value as the theme holds it now.
    pub kind: OptionKind,
    /// The value as the theme writes it now, overrides included.
    pub value: String,
    /// A default the theme names in a comment or `@var` header, for the
    /// Reset button and the small “default” note.
    pub default_value: Option<String>,
    /// Whether ReCore found a reason to show this option by default: a
    /// theme comment, `@var` metadata, a known token, or a value it can
    /// type. Unannotated text variables are left for “Show every variable”.
    pub recommended: bool,
    /// `Some(true)` when the option exists only in the dark palette,
    /// `Some(false)` when only in the light one.
    pub only_in: Option<bool>,
}

/// What sort of value a theme option holds.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionKind {
    /// A colour, shown as a swatch with a picker and a hex field.
    Color {
        /// The resolved colour, which the swatch shows.
        color: Rgba,
    },
    /// A percentage, shown from 0 to 100.
    Percent {
        /// The percentage, 0 to 100.
        value: f32,
        /// Whether the theme writes `25%` rather than `0.25`.
        percent: bool,
    },
    /// A length, a duration or another number with a unit.
    Number {
        /// The number as the theme writes it.
        value: f32,
        /// The unit that follows it, such as `px`, `rem` or `s`.
        unit: String,
    },
    /// A switch between two values, such as `on` and `off`.
    Toggle {
        /// Whether the option is currently on.
        on: bool,
        /// The value written when the switch is on.
        on_value: String,
        /// The value written when the switch is off.
        off_value: String,
    },
    /// A whole-number slider a BetterDiscord `@var slider` declares.
    Range {
        /// The current value, between `min` and `max`.
        value: i32,
        /// The lowest value the slider allows.
        min: i32,
        /// The highest value the slider allows.
        max: i32,
        /// How far one drag step moves the value.
        step: i32,
        /// The unit shown beside the number, which may be empty.
        unit: String,
    },
    /// One of a fixed list of values, shown as a menu.
    Choice {
        /// Each choice's label and the value the theme writes for it.
        choices: Vec<(String, String)>,
    },
    /// A picture, as a `url(...)` or `none`.
    Image {
        /// The address the theme names, or `None` for no picture.
        url: Option<String>,
        /// Whether the picture is a `data:` URI the theme carries itself.
        embedded: bool,
    },
    /// A font family, shown as a text field with the known fonts offered.
    Font,
    /// Free text the reader could not type more precisely.
    Text,
}

/// Reads every custom property worth offering from the resolved variables.
#[allow(
    dead_code,
    reason = "the tests read options without a theme's comments"
)]
pub fn detect(vars: &Vars, dark: bool) -> Vec<ThemeOption> {
    detect_with_notes(
        vars,
        dark,
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
    )
}

/// [`detect`], with the comments and sections a theme writes around its
/// properties.
pub fn detect_with_notes(
    vars: &Vars,
    dark: bool,
    notes: &std::collections::BTreeMap<String, Note>,
    properties: &std::collections::BTreeMap<String, PropertySpec>,
) -> Vec<ThemeOption> {
    let mut options = Vec::new();
    for (name, raw) in vars.iter() {
        if !name.starts_with("--") || name.starts_with("--rl-asset-") {
            continue;
        }
        // A data URI is a picture the theme carries, not something to type
        // into a field; offer its Clear button, but not the URI itself.
        if raw.len() > 2_000 {
            if image_name(&name.to_ascii_lowercase()) {
                let note = notes.get(name).cloned().unwrap_or_default();
                let spec = properties.get(name);
                let (label, group, description) = describe(name, &note);
                options.push(ThemeOption {
                    name: name.to_owned(),
                    label,
                    description,
                    group,
                    kind: OptionKind::Image {
                        url: None,
                        embedded: true,
                    },
                    value: String::new(),
                    default_value: spec
                        .and_then(|spec| spec.initial.clone())
                        .or_else(|| note.default_value.clone()),
                    recommended: true,
                    only_in: None,
                });
            }
            continue;
        }
        let note = notes.get(name).cloned().unwrap_or_default();
        let spec = properties.get(name);
        let Some(kind) = classify(vars, name, raw, dark, &note, spec) else {
            continue;
        };
        let (label, group, description) = describe(name, &note);
        // A colour's field and swatch show hex, not the `var()` or
        // `color-mix()` that produced it; the other value controls show the
        // value after its variables are filled in, so they can start from it.
        let value = match &kind {
            OptionKind::Color { color } => color.hex(),
            OptionKind::Percent { .. }
            | OptionKind::Number { .. }
            | OptionKind::Range { .. }
            | OptionKind::Toggle { .. }
            | OptionKind::Choice { .. } => vars
                .resolve(raw)
                .map(|value| value.into_owned())
                .unwrap_or_else(|| raw.to_owned()),
            _ => raw.to_owned(),
        };
        let alias = raw.trim_start().starts_with("var(");
        let alias_name = name.ends_with("_v") || name.ends_with("-v");
        let recommended = crate::options::record(name).is_some()
            || crate::options::known(name).is_some()
            || note.description.is_some()
            || note.group.is_some()
            || note.default_value.is_some()
            || spec.is_some()
            || (!alias && !alias_name && !matches!(kind, OptionKind::Text));
        options.push(ThemeOption {
            name: name.to_owned(),
            label,
            description,
            group,
            kind,
            value,
            default_value: spec
                .and_then(|spec| spec.initial.clone())
                .or_else(|| note.default_value.clone()),
            recommended,
            only_in: None,
        });
    }
    sort(&mut options);
    options
}

/// Adds the options only the other appearance names, keeping the chosen
/// appearance's value and kind for the names both have.
pub fn merge(
    mut options: Vec<ThemeOption>,
    other: Vec<ThemeOption>,
    other_dark: bool,
) -> Vec<ThemeOption> {
    for mut option in other {
        if !options.iter().any(|known| known.name == option.name) {
            option.only_in = Some(other_dark);
            options.push(option);
        }
    }
    sort(&mut options);
    options
}

fn sort(options: &mut [ThemeOption]) {
    options.sort_by(|a, b| {
        group_order(&a.group)
            .cmp(&group_order(&b.group))
            .then_with(|| a.group.cmp(&b.group))
            .then_with(|| a.label.cmp(&b.label))
    });
}

fn classify(
    vars: &Vars,
    name: &str,
    raw: &str,
    dark: bool,
    note: &Note,
    spec: Option<&PropertySpec>,
) -> Option<OptionKind> {
    let value = raw.trim();
    let lower = value.to_ascii_lowercase();
    let name_lower = name.to_ascii_lowercase();

    if let Some(choices) = choices(name) {
        return Some(OptionKind::Choice { choices });
    }
    if let Some(choices) = note_choices(note.description.as_deref(), value) {
        return Some(OptionKind::Choice { choices });
    }
    if name_lower.contains("flicker") || name_lower.contains("beam") {
        let on = vars
            .resolve(vars.get(name)?)?
            .trim()
            .trim_end_matches(['s', 'm'])
            .parse::<f32>()
            .is_ok_and(|value| value > 0.0);
        return Some(OptionKind::Toggle {
            on,
            on_value: if name_lower.contains("flicker") {
                "6s".into()
            } else {
                "0.5".into()
            },
            off_value: "0".into(),
        });
    }
    if matches!(
        lower.as_str(),
        "on" | "off" | "true" | "false" | "yes" | "no"
    ) {
        let (on_value, off_value) = match lower.as_str() {
            "true" | "false" => ("true".to_owned(), "false".to_owned()),
            "yes" | "no" => ("yes".to_owned(), "no".to_owned()),
            _ => ("on".to_owned(), "off".to_owned()),
        };
        return Some(OptionKind::Toggle {
            on: matches!(lower.as_str(), "on" | "true" | "yes"),
            on_value,
            off_value,
        });
    }
    if matches!(lower.as_str(), "0" | "1")
        && ["icon", "enable", "toggle", "switch", "settings"]
            .iter()
            .any(|word| name_lower.contains(word))
    {
        return Some(OptionKind::Toggle {
            on: lower == "1",
            on_value: "1".into(),
            off_value: "0".into(),
        });
    }
    if let Some(percent) = value.strip_suffix('%')
        && let Ok(percent) = percent.trim().parse::<f32>()
    {
        return Some(OptionKind::Percent {
            value: percent.clamp(0.0, 100.0),
            percent: true,
        });
    }
    if image_name(&name_lower) {
        let url = value.find("url(").and_then(|start| {
            let rest = &value[start + 4..];
            rest.find(')')
                .map(|end| rest[..end].trim().trim_matches(['"', '\'']).to_owned())
        });
        if let Some(url) = url {
            return Some(OptionKind::Image {
                embedded: url.starts_with("data:"),
                url: Some(url),
            });
        }
        if lower == "none" {
            return Some(OptionKind::Image {
                url: None,
                embedded: false,
            });
        }
    }
    if let Some(color) = vars.color(name, !dark) {
        return Some(OptionKind::Color { color });
    }
    if font_name(&name_lower) {
        return Some(OptionKind::Font);
    }
    if name_lower.contains("opacity")
        || name_lower.contains("surface")
        || name_lower.contains("dim")
        || name_lower.contains("alpha")
    {
        return percent(vars, name).map(|(value, percent)| OptionKind::Percent { value, percent });
    }
    if let Some((value, unit)) = number(vars, name) {
        return Some(OptionKind::Number { value, unit });
    }
    if let Some(spec) = spec
        && let Some(kind) = property_kind(vars, name, dark, spec)
    {
        return Some(kind);
    }
    Some(OptionKind::Text)
}

/// A kind `@property` gives a value that its CSS text did not reveal.
fn property_kind(vars: &Vars, name: &str, dark: bool, spec: &PropertySpec) -> Option<OptionKind> {
    let syntax = spec.syntax.as_deref()?.to_ascii_lowercase();
    if syntax.contains("color") {
        return vars
            .color(name, !dark)
            .map(|color| OptionKind::Color { color });
    }
    if syntax.contains("percentage") {
        return percent(vars, name).map(|(value, percent)| OptionKind::Percent { value, percent });
    }
    if syntax.contains("length") {
        return number(vars, name).map(|(value, unit)| OptionKind::Number { value, unit });
    }
    if syntax.contains("number") || syntax.contains("integer") {
        let value = vars.resolve(vars.get(name)?)?;
        return value
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| OptionKind::Number {
                value,
                unit: String::new(),
            });
    }
    None
}

fn choices(name: &str) -> Option<Vec<(String, String)>> {
    let values = match name {
        "--rl-motion" => ["full", "subtle", "off"].as_slice(),
        "--rl-background-size" | "--background-size" => ["cover", "contain", "auto"].as_slice(),
        "--background-attachment" => ["fixed", "scroll", "local"].as_slice(),
        _ => return None,
    };
    Some(
        values
            .iter()
            .map(|value| (title_case(value), (*value).to_owned()))
            .collect(),
    )
}

/// The choices a comment lists as `value: description`, or as
/// `off to disable, image to use..., color to use...`.
fn note_choices(description: Option<&str>, current: &str) -> Option<Vec<(String, String)>> {
    let description = description?;
    let mut choices = Vec::new();
    for part in description.split([',', ';']) {
        let (value, explicit) = match part.split_once(':') {
            Some((label, _)) => (label.trim(), true),
            None => (part.split_whitespace().next().unwrap_or(""), false),
        };
        let value = value.trim_end_matches(['.', ':', '-']);
        if !explicit && !option_word(value) {
            continue;
        }
        if value.is_empty()
            || value.len() > 24
            || !value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
            || matches!(
                value.to_ascii_lowercase().as_str(),
                "the"
                    | "a"
                    | "an"
                    | "to"
                    | "for"
                    | "use"
                    | "set"
                    | "this"
                    | "that"
                    | "if"
                    | "when"
                    | "default"
                    | "change"
                    | "example"
                    | "note"
                    | "more"
                    | "info"
                    | "see"
                    | "or"
                    | "and"
                    | "with"
                    | "without"
                    | "value"
                    | "values"
            )
        {
            continue;
        }
        if !choices
            .iter()
            .any(|(_, known): &(String, String)| known.eq_ignore_ascii_case(value))
        {
            choices.push((title_case(value), value.to_owned()));
        }
    }
    let plain_toggle = choices.len() == 2
        && choices
            .iter()
            .all(|(_, value)| matches!(value.as_str(), "off" | "on" | "true" | "false"));
    (choices.len() >= 2
        && !plain_toggle
        && choices
            .iter()
            .any(|(_, value)| value.eq_ignore_ascii_case(current)))
    .then_some(choices)
}

/// The words comments commonly list as a choice without a colon. Keeping
/// this list small stops prose such as `height of the chatbar (...)` from
/// becoming a choice.
fn option_word(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "off"
            | "on"
            | "true"
            | "false"
            | "yes"
            | "no"
            | "hide"
            | "show"
            | "custom"
            | "image"
            | "picture"
            | "color"
            | "colour"
            | "gradient"
            | "none"
            | "auto"
            | "left"
            | "center"
            | "right"
            | "top"
            | "bottom"
            | "titlebar"
            | "default"
            | "separated"
            | "aligned"
            | "compact"
            | "small"
            | "large"
            | "full"
            | "subtle"
            | "dark"
            | "light"
            | "system"
            | "minimal"
            | "normal"
            | "inline"
            | "block"
            | "cover"
            | "contain"
            | "fixed"
            | "scroll"
            | "local"
            | "round"
            | "square"
    )
}

fn title_case(text: &str) -> String {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_uppercase().collect::<String>() + characters.as_str()
}

/// Uses BetterDiscord's `@var` header to give an option the label and kind
/// the theme intended, matching `accentcolor_s` to `--accentcolor` as well as
/// exact names.
pub fn apply_meta(options: &mut [ThemeOption], vars: &[MetaVar]) {
    for option in options {
        let name = option.name.trim_start_matches("--");
        let Some(var) = vars.iter().find(|var| meta_name_matches(&var.name, name)) else {
            continue;
        };
        option.recommended = true;
        if option.default_value.is_none() {
            option.default_value = meta_default(&var.rest);
        }
        let label = clean_meta_label(&var.label);
        if !label.is_empty() {
            if option
                .description
                .as_deref()
                .is_some_and(|description| description.eq_ignore_ascii_case(&label))
            {
                option.description = None;
            }
            option.label = label;
        }
        match var.kind.as_str() {
            "checkbox" => {
                let current = option.value.trim().to_ascii_lowercase();
                let (on_value, off_value) = match current.as_str() {
                    "true" | "false" => ("true".to_owned(), "false".to_owned()),
                    "on" | "off" => ("on".to_owned(), "off".to_owned()),
                    "yes" | "no" => ("yes".to_owned(), "no".to_owned()),
                    _ => ("1".to_owned(), "0".to_owned()),
                };
                option.kind = OptionKind::Toggle {
                    on: matches!(current.as_str(), "1" | "true" | "on" | "yes"),
                    on_value,
                    off_value,
                };
            }
            "color" => {
                if let Some(color) = crate::color::parse(&option.value, true) {
                    option.kind = OptionKind::Color { color };
                }
            }
            "select" => {
                if let Some(choices) = meta_choices(&var.rest)
                    && choices.len() >= 2
                {
                    option.kind = OptionKind::Choice { choices };
                }
            }
            "slider" => {
                if let Some(range) = meta_range(&var.rest, &option.value) {
                    option.kind = range;
                }
            }
            "text" if font_name(&option.name.to_ascii_lowercase()) => {
                option.kind = OptionKind::Font;
            }
            _ => {}
        }
    }
}

fn meta_default(rest: &str) -> Option<String> {
    let rest = rest.trim();
    if rest.is_empty() || rest.starts_with('{') {
        return None;
    }
    let value = rest.split('{').next().unwrap_or(rest).trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn meta_name_matches(meta: &str, name: &str) -> bool {
    let meta = meta.replace('_', "-");
    let name = name.replace('_', "-");
    if meta.eq_ignore_ascii_case(&name) {
        return true;
    }
    let compact = |text: &str| {
        text.chars()
            .filter(|character| !matches!(character, '-' | '_'))
            .collect::<String>()
    };
    let compact_name = compact(&name);
    for suffix in ["-s", "-b", "-i", "-c", "-n", "-setting", "-color"] {
        if let Some(base) = meta.strip_suffix(suffix)
            && (base.eq_ignore_ascii_case(&name)
                || compact(base).eq_ignore_ascii_case(&compact_name))
        {
            return true;
        }
    }
    false
}

fn clean_meta_label(label: &str) -> String {
    let mut label = label.trim();
    let lower = label.to_ascii_lowercase();
    let mut end = label.len();
    for marker in ["[default", " - default", " default:"] {
        if let Some(index) = lower.find(marker) {
            end = end.min(index);
        }
    }
    label = &label[..end];
    label
        .trim_end_matches([':', '-', ' ', ','])
        .trim()
        .to_owned()
}

fn meta_range(rest: &str, current: &str) -> Option<OptionKind> {
    let (value, unit) = split_number(current)?;
    let mut min = 0i32;
    let mut max = 100i32;
    let mut step = 1i32;
    if let Some(open) = rest.find('{')
        && let Some(close) = rest[open..].find('}')
    {
        let body = &rest[open + 1..open + close];
        for field in body.split(',') {
            let Some((key, value)) = field.split_once(':') else {
                continue;
            };
            let value = value.trim().parse::<f32>().ok()?.round() as i32;
            match key.trim().to_ascii_lowercase().as_str() {
                "min" => min = value,
                "max" => max = value,
                "step" => step = value.max(1),
                _ => {}
            }
        }
    }
    if max <= min || value < min as f32 || value > max as f32 {
        return None;
    }
    Some(OptionKind::Range {
        value: value.round() as i32,
        min,
        max,
        step,
        unit,
    })
}

/// `12px`, `50%` or `0.5` as its number and suffix.
fn split_number(text: &str) -> Option<(f32, String)> {
    let text = text.trim();
    let end = text
        .find(|character: char| {
            !(character.is_ascii_digit() || matches!(character, '.' | '-' | '+'))
        })
        .unwrap_or(text.len());
    let number = text[..end].parse::<f32>().ok()?;
    Some((number, text[end..].trim().to_owned()))
}

fn meta_choices(rest: &str) -> Option<Vec<(String, String)>> {
    let start = rest.find('{')?;
    let end = rest.rfind('}')?;
    let inside = rest.get(start + 1..end)?;
    let mut choices = Vec::new();
    for part in crate::color::split_top(inside, ',') {
        let (label, value) = match part.split_once(':') {
            Some((label, value)) => (label, value),
            None => (part, part),
        };
        let label = meta_unquote(label.trim());
        let value = meta_unquote(value.trim());
        if value.is_empty() {
            continue;
        }
        choices.push((label, value));
    }
    (choices.len() >= 2).then_some(choices)
}

fn meta_unquote(text: &str) -> String {
    text.trim().trim_matches(['"', '\'']).trim().to_owned()
}

fn image_name(name: &str) -> bool {
    name.contains("image")
        || name.contains("background")
        || name.contains("logo")
        || name.contains("avatar")
        || name.contains("mark")
        || name.contains("picture")
        || name.contains("icon")
}

/// Whether a custom property names a font family, rather than its size,
/// weight, style or colour.
fn font_name(name: &str) -> bool {
    (name.contains("font")
        && ![
            "size",
            "weight",
            "color",
            "colour",
            "style",
            "line-height",
            "letter-spacing",
        ]
        .iter()
        .any(|word| name.contains(word)))
        || name.contains("typeface")
}

fn percent(vars: &Vars, name: &str) -> Option<(f32, bool)> {
    let value = vars.resolve(vars.get(name)?)?;
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        return Some((percent.trim().parse::<f32>().ok()?.clamp(0.0, 100.0), true));
    }
    let number = value.parse::<f32>().ok()?;
    Some((
        if number > 1.0 {
            number.clamp(0.0, 100.0)
        } else {
            (number * 100.0).clamp(0.0, 100.0)
        },
        false,
    ))
}

fn number(vars: &Vars, name: &str) -> Option<(f32, String)> {
    let value = vars.resolve(vars.get(name)?)?;
    let value = value.trim();
    for unit in ["px", "rem", "em", "ms", "s"] {
        if let Some(number) = value.strip_suffix(unit) {
            return Some((number.trim().parse::<f32>().ok()?, unit.to_owned()));
        }
    }
    None
}

/// A readable label, group and description for a custom property, from its
/// name, the comment beside it and the sections the theme writes.
fn describe(name: &str, note: &Note) -> (String, String, Option<String>) {
    if let Some((label, group, description)) = record(name) {
        let description = description.map(str::to_owned).or_else(|| {
            note.description
                .clone()
                .filter(|description| useful_description(description))
        });
        return (label.to_owned(), group.to_owned(), description);
    }
    let (label, mut group, known_description) = match known(name) {
        Some((label, group, description)) => (
            label.to_owned(),
            group.to_owned(),
            description.map(str::to_owned),
        ),
        None => {
            let group = numbered_group(name)
                .map(str::to_owned)
                .unwrap_or_else(|| group_for(name).to_owned());
            (pretty(name), group, None)
        }
    };
    // A theme's own section headings are usually better than a generic
    // bucket; ReCord's `--rl-*` names keep their curated groups.
    if !name.starts_with("--rl-")
        && let Some(section) = &note.group
        && !section.is_empty()
        && !ignored_section(section)
    {
        group = canonical_group(section)
            .map(str::to_owned)
            .unwrap_or_else(|| section.clone());
    }
    let description = note
        .description
        .clone()
        .filter(|description| {
            Some(description) != note.group.as_ref() && useful_description(description)
        })
        .or(known_description);
    (label, group, description)
}

/// Whether a comment says something about the option, rather than naming a
/// broad section or the theme itself.
fn useful_description(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    !(lower.contains("design token")
        || lower.contains("discord's token")
        || lower.contains("material you palette")
        || lower == "palette"
        || lower == "tokens"
        || lower == "colors"
        || lower == "colours"
        || lower == "theme options")
}

/// Labels and explanations for Discord's tokens and the custom properties
/// the common theme families use.
fn known(name: &str) -> Option<(&'static str, &'static str, Option<&'static str>)> {
    if let Some(label) = name.strip_prefix("--font-weight-") {
        let label = match label {
            "black" => "Black weight",
            "extrabold" => "Extra-bold weight",
            "bold" => "Bold weight",
            "semibold" => "Semi-bold weight",
            "medium" => "Medium weight",
            "normal" => "Normal weight",
            "light" => "Light weight",
            "extralight" => "Extra-light weight",
            "thin" => "Thin weight",
            _ => return None,
        };
        return Some((label, "Typography", Some("How heavy this text is drawn.")));
    }
    if let Some(label) = name.strip_prefix("--scrollbar-") {
        let label = if label.contains("thumb") {
            "Scrollbar thumb"
        } else if label.contains("track") {
            "Scrollbar track"
        } else {
            "Scrollbar"
        };
        return Some((
            label,
            "Interface",
            Some("The scrollbar's thumb or its track."),
        ));
    }
    if let Some(alpha) = name.strip_prefix("--rl-a")
        && alpha.chars().all(|character| character.is_ascii_digit())
    {
        return Some((
            "Accent alpha",
            "ReCord palette",
            Some("A translucent step of the accent."),
        ));
    }
    let option = match name {
        // Backgrounds.
        "--background-primary" | "--background-base-lower" | "--chat-background" => (
            "Chat background",
            "Backgrounds",
            Some("The main chat area behind messages."),
        ),
        "--background-secondary" | "--background-base-low" | "--panel-bg" | "--user-popout-bg" => (
            "Sidebar and panels",
            "Backgrounds",
            Some("The channel sidebar, member list and panels."),
        ),
        "--background-secondary-alt"
        | "--background-base-lowest"
        | "--background-tertiary"
        | "--app-frame-background"
        | "--coal" => (
            "Rail and deep panels",
            "Backgrounds",
            Some("The server rail and the deepest surfaces."),
        ),
        "--background-floating"
        | "--background-surface-high"
        | "--card-background-default"
        | "--background-nested-floating"
        | "--bg-floating" => (
            "Menus and cards",
            "Backgrounds",
            Some("Floating menus, popouts, cards and tooltips."),
        ),
        "--background-surface-higher" | "--background-surface-highest" => (
            "Raised surfaces",
            "Backgrounds",
            Some("Surfaces above the menus and cards."),
        ),
        "--channeltextarea-background"
        | "--input-background-default"
        | "--background-code"
        | "--textbox-background"
        | "--textbox-border" => (
            "Composer and inputs",
            "Backgrounds",
            Some("The message box, fields and code blocks."),
        ),
        "--background-accent" => (
            "Accent panel",
            "Backgrounds",
            Some("The background behind accent elements."),
        ),
        "--background-modifier-hover"
        | "--interactive-background-hover"
        | "--interactive-background-active"
        | "--interactive-background-selected"
        | "--background-modifier-active"
        | "--background-modifier-selected"
        | "--hover"
        | "--active"
        | "--active-2" => (
            "Hover and selection",
            "Backgrounds",
            Some("Rows and buttons under the pointer or selected."),
        ),
        "--background-mentioned"
        | "--background-mentioned-hover"
        | "--mention"
        | "--mention-hover" => (
            "Mention highlight",
            "Backgrounds",
            Some("Messages that mention this account."),
        ),
        "--background-message-hover" | "--message-background-hover" | "--message-hover" => (
            "Message hover",
            "Backgrounds",
            Some("A message under the pointer."),
        ),
        "--tint"
        | "--background-shading"
        | "--card-shading"
        | "--popout-shading"
        | "--modal-shading"
        | "--input-shading" => (
            "Shading",
            "Backgrounds",
            Some("A translucent layer over the background."),
        ),

        // Text.
        "--text-normal" | "--text-default" | "--text-strong" | "--normal-text" => {
            ("Text", "Text", Some("Normal message and interface text."))
        }
        "--header-primary" | "--header-secondary" => (
            "Headings",
            "Text",
            Some("Channel names, headings and important text."),
        ),
        "--text-muted" | "--text-subtle" | "--muted-text" | "--textdark" | "--textdarker"
        | "--textdarkest" => (
            "Muted text",
            "Text",
            Some("Timestamps, placeholders and quieter labels."),
        ),
        "--interactive-normal"
        | "--interactive-hover"
        | "--interactive-active"
        | "--interactive-muted"
        | "--interactive-text-hover"
        | "--interactive-text-active" => (
            "Interactive text",
            "Text",
            Some("Text in buttons, menus and controls."),
        ),
        "--channels-default"
        | "--channel-normal"
        | "--channel-muted"
        | "--channel-hover"
        | "--channel-selected"
        | "--channel-selected-bg" => (
            "Channel text",
            "Text",
            Some("The names in the channel list."),
        ),
        "--channel-icon" | "--channel-unread" | "--channel-unread-hover" => (
            "Channel icons",
            "Text",
            Some("Icons and unread marks in the channel list."),
        ),
        "--text-invert" | "--control-primary-text-default" => (
            "Text on accent",
            "Accents",
            Some("The text drawn over accent buttons."),
        ),
        "--textbrightest" | "--textbrighter" | "--textbright" => (
            "Bright text",
            "Text",
            Some("The brightest steps of the text scale."),
        ),
        "--white" | "--white-500" | "--white-hsl" => (
            "White",
            "Base colours",
            Some("The theme's brightest neutral."),
        ),

        // Accents, links and mentions.
        "--brand-experiment"
        | "--brand-500"
        | "--accent"
        | "--accent-color"
        | "--accentcolor"
        | "--main-color"
        | "--primary"
        | "--control-primary-background-default"
        | "--background-brand" => (
            "Accent",
            "Accents",
            Some("The main accent, used by buttons and selected items."),
        ),
        "--brand-360"
        | "--brand-560"
        | "--brand-530"
        | "--accent-hover"
        | "--hover-color"
        | "--accentcolor2"
        | "--control-primary-background-hover" => (
            "Accent hover",
            "Accents",
            Some("The accent when hovered, or its lighter step."),
        ),
        "--text-link" | "--link" | "--linkcolor" | "--text-brand" => {
            ("Links", "Accents", Some("Links and clickable text."))
        }
        "--mentioncolor" => (
            "Mentions",
            "Accents",
            Some("The colour of names and mentions."),
        ),
        "--reply" | "--reply-hover" => (
            "Replies",
            "Accents",
            Some("Messages that reply to this account."),
        ),

        // Status.
        "--status-danger" | "--danger" | "--dangercolor" | "--danger-color" | "--dnd" => (
            "Danger",
            "Status",
            Some("Errors, destructive buttons and Do Not Disturb."),
        ),
        "--status-positive" | "--success-color" | "--successcolor" | "--online" => (
            "Online and success",
            "Status",
            Some("Online people, positive results and confirmations."),
        ),
        "--status-warning" | "--warning-color" | "--warningcolor" | "--idle" => {
            ("Warning", "Status", Some("Warnings and the idle presence."))
        }
        "--streaming" | "--streaming-color" => {
            ("Streaming", "Status", Some("The streaming presence badge."))
        }
        "--offline" | "--offline-color" | "--invisible" => {
            ("Offline", "Status", Some("Offline and invisible people."))
        }

        // Borders and lines.
        "--border-subtle" => (
            "Subtle border",
            "Borders",
            Some("The quietest panel and divider edges."),
        ),
        "--border-normal" | "--border" => (
            "Border",
            "Borders",
            Some("The normal panel and divider edge."),
        ),
        "--border-strong" => (
            "Strong border",
            "Borders",
            Some("The most visible panel edge."),
        ),
        "--border-light" => ("Light border", "Borders", Some("A translucent panel edge.")),
        "--border-hover" => (
            "Hover border",
            "Borders",
            Some("The panel edge while hovered."),
        ),
        "--button-border" => (
            "Button border",
            "Borders",
            Some("The neutral edge around buttons."),
        ),
        "--line" | "--fg-border" | "--bg-border" => (
            "Line",
            "Borders",
            Some("Dividers, outlines and quiet edges."),
        ),
        "--spine-default" => (
            "Reply spine",
            "Borders",
            Some("The line beside a replied-to message."),
        ),

        // Interface parts.
        "--control-secondary-background-default"
        | "--control-secondary-background-hover"
        | "--button-secondary-background"
        | "--button-secondary-background-hover" => (
            "Secondary buttons",
            "Interface",
            Some("The neutral buttons in dialogs and settings."),
        ),
        "--top-bar-height"
        | "--top-bar-button-position"
        | "--top-bar-title-position"
        | "--subtle-top-bar-title" => (
            "Top bar",
            "Interface",
            Some("The bar above the channel and its buttons."),
        ),
        "--custom-window-controls" | "--window-control-size" => (
            "Window controls",
            "Interface",
            Some("The custom minimise, maximise and close buttons."),
        ),
        "--chatbar-height" | "--custom-chatbar" => (
            "Chat bar",
            "Interface",
            Some("The message bar and whether it is separated from the chat."),
        ),
        "--custom-dms-icon"
        | "--dms-icon-svg-size"
        | "--dms-icon-color-before"
        | "--dms-icon-color-after"
        | "--dms-icon-svg-transition" => (
            "Direct messages button",
            "Interface",
            Some("The home and direct messages button above the channel list."),
        ),
        "--custom-dms-background" | "--dms-background-image-size" | "--dms-background-color" => (
            "Direct messages background",
            "Interface",
            Some("The background of the direct messages button."),
        ),
        "--custom-spotify-bar" => (
            "Spotify bar",
            "Interface",
            Some("The text-like progress bar under a Spotify activity."),
        ),
        "--panel-labels" | "--label-color" | "--label-font-weight" => (
            "Panel labels",
            "Interface",
            Some("The small labels on native panels."),
        ),
        "--chat-bubble-bg" | "--chat-bubble-padding" | "--chat-bubble-display" => (
            "Chat bubbles",
            "Interface",
            Some("How messages read as bubbles."),
        ),
        "--channel-pill-width" | "--channel-pill-height" => (
            "Channel pills",
            "Interface",
            Some("The unread and selected marks in the channel list."),
        ),

        // Typography.
        "--font" | "--font-primary" | "--font-display" | "--font-headline" | "--main-font"
        | "--rl-font" => (
            "Font",
            "Typography",
            Some("The family the window draws its text with."),
        ),
        "--font-code" | "--code-font" | "--rl-font-mono" => (
            "Code font",
            "Typography",
            Some("The family used for code blocks."),
        ),
        "--font-size" | "--text-size" => (
            "Text size",
            "Typography",
            Some("The base size of the window's text."),
        ),
        "--letter-spacing" | "--rl-tracking" => (
            "Letter spacing",
            "Typography",
            Some("The space between letters."),
        ),

        // Shapes and spacing.
        "--radius" | "--radius-md" | "--radius-sm" | "--radius-lg" | "--radius-xl"
        | "--radius-xxl" | "--window-roundness" | "--rl-radius" => (
            "Corner radius",
            "Shape",
            Some("How rounded panels, fields and pictures are."),
        ),
        "--gap" => (
            "Panel gap",
            "Shape",
            Some("The space between the main panels."),
        ),
        "--divider-thickness" | "--border-thickness" => (
            "Border thickness",
            "Shape",
            Some("How thick dividers and panel borders are."),
        ),
        "--toolbar-height" | "--bottom-height" | "--chat-bottom-spacing" => (
            "Sizes",
            "Shape",
            Some("The height of bars and the space below the chat."),
        ),

        // Pictures.
        "--background-image" | "--background-image-url" | "--rl-background-image" => (
            "Background picture",
            "Pictures",
            Some("A picture behind the interface."),
        ),
        "--dms-background-image-url" => (
            "DMs background picture",
            "Pictures",
            Some("The picture on the direct messages button."),
        ),
        "--dms-icon-svg-url" => (
            "DMs icon",
            "Pictures",
            Some("The icon on the direct messages button."),
        ),
        "--home-icon" => (
            "Home icon",
            "Pictures",
            Some("The icon on the home button."),
        ),
        "--user-popout-image" => (
            "Popout background",
            "Pictures",
            Some("The picture behind a user popout."),
        ),
        "--user-modal-image" => (
            "Modal background",
            "Pictures",
            Some("The picture behind a user modal."),
        ),
        "--overlay-image" => (
            "Overlay background",
            "Pictures",
            Some("The picture behind an overlay."),
        ),
        "--grain" => (
            "Grain texture",
            "Pictures",
            Some("A texture laid over the window."),
        ),
        "--background-position" => (
            "Background position",
            "Pictures",
            Some("Where the background picture sits."),
        ),
        "--background-attachment" => (
            "Background attachment",
            "Pictures",
            Some("Whether the background stays put while a page scrolls."),
        ),
        "--background-size" => (
            "Background fit",
            "Pictures",
            Some("How the background picture fills the window."),
        ),

        // Effects.
        "--animations" | "--rl-motion" => (
            "Animations",
            "Effects",
            Some("Whether the window's motion plays."),
        ),
        "--transition"
        | "--list-item-transition"
        | "--border-hover-transition"
        | "--message-highlight-duration" => (
            "Transitions",
            "Effects",
            Some("How quickly hovers and highlights move."),
        ),
        "--app-blur" | "--foreground-blur" | "--panel-blur" | "--blur-amount" => (
            "Blur",
            "Effects",
            Some("How much the surfaces behind this one are blurred."),
        ),
        "--shadow" | "--shadow-lg" => (
            "Shadows",
            "Effects",
            Some("The shadows under panels and menus."),
        ),
        "--transparency-tweaks"
        | "--remove-bg-layer"
        | "--opacity"
        | "--background-shading-percent" => (
            "Transparency",
            "Effects",
            Some("How see-through the window and its panels are."),
        ),

        // Controls from Midnight and system24.
        "--small-user-panel" => (
            "Small account panel",
            "Interface",
            Some("Draw the account panel shorter, as old Discord did."),
        ),
        "--unrounding" => (
            "Square panels",
            "Shape",
            Some("Remove the rounded corners from panels."),
        ),
        "--round-pfp" => (
            "Round avatars",
            "Pictures",
            Some("Draw profile pictures round instead of square."),
        ),
        "--remove-pfp-decor" => (
            "Avatar decorations",
            "Pictures",
            Some("Hide the decorations around profile pictures."),
        ),
        "--ascii-titles" | "--ascii-loader" => (
            "ASCII art",
            "Interface",
            Some("Use the theme's ASCII titles and loading screen."),
        ),
        "--colors" => (
            "Theme colours",
            "Interface",
            Some("Use the theme's own palette instead of Discord's."),
        ),
        "--settingsicons" => (
            "Settings icons",
            "Interface",
            Some("Use the theme's icons in User Settings."),
        ),
        _ => return None,
    };
    Some(option)
}

/// The group for the numbered colour scales themes use.
fn numbered_group(name: &str) -> Option<&'static str> {
    let (base, index) = name.rsplit_once('-')?;
    index.parse::<u32>().ok()?;
    match base {
        "--text" => Some("Text"),
        "--bg" => Some("Backgrounds"),
        "--accent" => Some("Accents"),
        "--red" | "--green" | "--blue" | "--yellow" | "--purple" | "--orange" | "--pink"
        | "--gray" | "--grey" | "--white" | "--black" => Some("Base colours"),
        _ => None,
    }
}
/// ReCord's own options, with their names and what each does.
fn record(name: &str) -> Option<(&'static str, &'static str, Option<&'static str>)> {
    let option = match name {
        "--rl-accent" | "--rl-signal" => (
            "Accent",
            "ReCore look",
            Some("The signal colour every ReCord shade derives from."),
        ),
        "--rl-font" => (
            "Typeface",
            "ReCore look",
            Some("The font family the window draws with."),
        ),
        "--rl-radius" => (
            "Corner radius",
            "ReCore look",
            Some("Square panels at 0, softer corners above."),
        ),
        "--rl-motion" => (
            "Animations",
            "ReCore look",
            Some("Full, subtle (menus and dialogs) or off."),
        ),
        "--rl-icons" => (
            "Pixel icons",
            "ReCore look",
            Some("ReCord's 16 px pixel icons instead of Discord's."),
        ),
        "--rl-scanline-opacity" => (
            "Scanlines",
            "ReCord effects",
            Some("The horizontal CRT lines over the window."),
        ),
        "--rl-grain-opacity" => (
            "Film grain",
            "ReCord effects",
            Some("The static over the window."),
        ),
        "--rl-vignette-opacity" => (
            "Vignette",
            "ReCord effects",
            Some("The darkening towards the window's edges."),
        ),
        "--rl-flicker-duration" => (
            "CRT flicker",
            "ReCord effects",
            Some("A slow brightness flicker."),
        ),
        "--rl-beam-opacity" => (
            "Rolling scan beam",
            "ReCord effects",
            Some("The pale band that sweeps down the window."),
        ),
        "--rl-frame-opacity" => (
            "Corner frame",
            "ReCord effects",
            Some("The brackets in the window's corners."),
        ),
        "--rl-background-image" => (
            "Background image",
            "Background",
            Some("A picture behind the interface."),
        ),
        "--rl-background-dim" => (
            "Background darkening",
            "Background",
            Some("How far the picture is dimmed behind the panels."),
        ),
        "--rl-surface" | "--rl-panel-opacity" => (
            "Panel opacity",
            "Background",
            Some("How see-through panels are over the picture."),
        ),
        "--rl-rail-opacity" => (
            "Server rail opacity",
            "Background",
            Some("How much of the picture shows through the server rail."),
        ),
        "--rl-sidebar-opacity" => (
            "Channel list opacity",
            "Background",
            Some("How much of the picture shows through the channel list."),
        ),
        "--rl-chat-opacity" => (
            "Chat opacity",
            "Background",
            Some("How much of the picture shows through the messages."),
        ),
        "--rl-members-opacity" => (
            "Member list opacity",
            "Background",
            Some("How much of the picture shows through the member list."),
        ),
        "--rl-composer-opacity" => (
            "Composer opacity",
            "Background",
            Some("How much of the picture shows through the message box."),
        ),
        "--rl-raised-opacity" => (
            "Popout opacity",
            "Background",
            Some("How much of the picture shows through cards and popouts."),
        ),
        "--rl-tooltip-opacity" => (
            "Tooltip opacity",
            "Background",
            Some("How much of the picture shows through tooltips."),
        ),
        "--rl-font-size" => (
            "Text size",
            "Typography",
            Some("The size of the window's text."),
        ),
        "--rl-mono-font" => (
            "Code font",
            "Typography",
            Some("The family used for code and monospaced text."),
        ),
        "--rl-background-size" => (
            "Background fit",
            "Background",
            Some("How the picture fills the window."),
        ),
        "--rl-avatar-filter" => (
            "Avatar filter",
            "Pictures",
            Some("The CSS filter over people's avatars."),
        ),
        "--rl-avatar-scanline" => (
            "Avatar scanlines",
            "Pictures",
            Some("The scanline mask over avatars."),
        ),
        "--rl-tracking" => (
            "Letter spacing",
            "Typography",
            Some("The spacing between letters."),
        ),
        "--rl-lite" => (
            "Low power",
            "ReCord effects",
            Some("Drops the grain and the constant repaints."),
        ),
        "--rl-void" => ("Void", "ReCord palette", None),
        "--rl-ink" => ("Ink", "ReCord palette", None),
        "--rl-coal" => ("Coal", "ReCord palette", None),
        "--rl-panel" => ("Panel", "ReCord palette", None),
        "--rl-raised" => ("Raised", "ReCord palette", None),
        "--rl-high" => ("High", "ReCord palette", None),
        "--rl-higher" => ("Higher", "ReCord palette", None),
        "--rl-line" => ("Line", "ReCord palette", None),
        "--rl-paper" => ("Paper", "ReCord palette", None),
        "--rl-cream" => ("Cream", "ReCord palette", None),
        "--rl-bone" => ("Bone", "ReCord palette", None),
        "--rl-smoke" => ("Smoke", "ReCord palette", None),
        "--rl-ash" => ("Ash", "ReCord palette", None),
        "--rl-faint" => ("Faint", "ReCord palette", None),
        "--rl-teal" => ("Teal", "ReCord palette", None),
        "--rl-green" => ("Green", "ReCord palette", None),
        "--rl-amber" => ("Amber", "ReCord palette", None),
        "--rl-red" => ("Red", "ReCord palette", None),
        "--rl-purple" => ("Purple", "ReCord palette", None),
        "--rl-offline" => ("Offline", "ReCord palette", None),
        "--rl-coal-base" => ("Coal base", "ReCord palette", None),
        "--rl-high-base" => ("High base", "ReCord palette", None),
        "--rl-higher-base" => ("Higher base", "ReCord palette", None),
        "--rl-ink-base" => ("Ink base", "ReCord palette", None),
        "--rl-raised-base" => ("Raised base", "ReCord palette", None),
        "--rl-night-teal" => ("Night teal", "ReCord palette", None),
        "--rl-oxide" => ("Oxide", "ReCord palette", None),
        "--rl-signal-deep" => ("Signal deep", "ReCord palette", None),
        "--rl-signal-dim" => ("Signal dim", "ReCord palette", None),
        "--rl-signal-hot" => ("Signal hot", "ReCord palette", None),
        "--rl-signal-soft" => ("Signal soft", "ReCord palette", None),
        "--rl-teal-deep" => ("Teal deep", "ReCord palette", None),
        "--rl-weight" => (
            "Weight",
            "Typography",
            Some("The weight ReCord's text is drawn at."),
        ),
        "--rl-ease" => (
            "Easing",
            "Effects",
            Some("How ReCord's transitions speed up and slow down."),
        ),
        "--rl-dither" => (
            "Dither pattern",
            "Effects",
            Some("The repeating pattern ReCord draws over its accents."),
        ),
        _ => return None,
    };
    Some(option)
}

/// A human label from a custom property's name.
fn pretty(name: &str) -> String {
    let name = name.trim_start_matches("--");
    let name = name
        .strip_prefix("rl-")
        .or_else(|| name.strip_prefix("discord-"))
        .unwrap_or(name);
    let mut words = Vec::new();
    for word in name.split(['-', '_']).filter(|word| !word.is_empty()) {
        words.extend(compound_words(word));
    }
    let Some(first) = words.first() else {
        return name.to_owned();
    };
    let mut label = word_label(first, true);
    for word in words.iter().skip(1) {
        label.push(' ');
        label.push_str(&word_label(word, false));
    }
    label
}

/// Splits the long lower-case names themes such as DiscordRecolor write
/// (`backgroundprimary`, `settingsicons`) into the words they hold.
fn compound_words(word: &str) -> Vec<String> {
    const WORDS: &[&str] = &[
        "background",
        "foreground",
        "primary",
        "secondary",
        "tertiary",
        "surface",
        "container",
        "settings",
        "icons",
        "buttons",
        "button",
        "text",
        "brightest",
        "brighter",
        "bright",
        "darkest",
        "darker",
        "dark",
        "color",
        "colour",
        "accent",
        "brand",
        "floating",
        "code",
        "channel",
        "textarea",
        "input",
        "tooltip",
        "scrollbar",
        "mention",
        "danger",
        "warning",
        "success",
        "positive",
        "negative",
        "hover",
        "active",
        "selected",
        "link",
        "avatar",
        "header",
        "footer",
        "panel",
        "sidebar",
        "chat",
        "members",
        "composer",
        "title",
        "window",
        "controls",
        "control",
        "server",
        "guild",
        "message",
        "bubble",
        "pill",
        "border",
        "line",
        "radius",
        "round",
        "opacity",
        "shadow",
        "blur",
        "image",
        "picture",
        "logo",
        "icon",
        "font",
        "size",
        "weight",
        "height",
        "width",
        "spacing",
        "padding",
        "margin",
        "gap",
        "transition",
        "animation",
        "duration",
        "content",
        "version",
        "brightness",
        "shading",
        "tint",
        "gradient",
        "overlay",
        "popout",
        "modal",
        "spotify",
        "ascii",
        "labels",
        "panel",
    ];
    let mut pieces = Vec::new();
    let mut rest = word;
    while !rest.is_empty() {
        let lower = rest.to_ascii_lowercase();
        let found = WORDS
            .iter()
            .filter(|known| lower.starts_with(**known))
            .max_by_key(|known| known.len());
        let Some(known) = found else {
            pieces.push(rest.to_owned());
            break;
        };
        pieces.push((*known).to_owned());
        rest = &rest[known.len()..];
    }
    pieces
}

fn word_label(word: &str, first: bool) -> String {
    let lower = word.to_ascii_lowercase();
    let label = match lower.as_str() {
        "bg" => "background",
        "fg" => "foreground",
        "ui" => "UI",
        "url" => "URL",
        "svg" => "SVG",
        "pfp" => "avatar",
        "dms" => "DMs",
        "dm" => "DM",
        "id" => "ID",
        "api" => "API",
        "hdr" => "header",
        "tb" => "title bar",
        "img" => "image",
        "num" => "number",
        "ascii" => "ASCII",
        other => other,
    }
    .to_owned();
    if first
        && !matches!(
            lower.as_str(),
            "ui" | "url" | "svg" | "dms" | "dm" | "id" | "api"
        )
    {
        let mut characters = label.chars();
        let mut out = String::new();
        if let Some(first) = characters.next() {
            out.extend(first.to_uppercase());
            out.push_str(characters.as_str());
        }
        out
    } else {
        label
    }
}

/// Headings that say nothing about the options below them.
fn ignored_section(section: &str) -> bool {
    let lower = section.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "color options" | "colour options" | "colors" | "colours" | "other options"
    ) || lower.contains("tokens")
        || lower.contains("palette")
        || lower.starts_with("settings")
}

/// The stable group a theme's section heading names, so `ACCENT COLORS` and
/// `accent colors` share one group across themes.
fn canonical_group(section: &str) -> Option<&'static str> {
    let lower = section.to_ascii_lowercase();
    if lower.contains("light theme")
        || lower.contains("dark theme")
        || lower.contains("ash theme")
        || lower.contains("midnight theme")
    {
        return None;
    }
    if lower.contains("base color") || lower.contains("base colour") {
        Some("Base colours")
    } else if lower.contains("accent") || lower.contains("brand") {
        Some("Accents")
    } else if lower.contains("status") || lower.contains("online") || lower.contains("presence") {
        Some("Status")
    } else if lower.contains("channel") {
        Some("Channels")
    } else if lower.contains("text") {
        Some("Text")
    } else if lower.contains("font") || lower.contains("typeface") {
        Some("Typography")
    } else if lower.contains("background")
        || lower.contains("shading")
        || lower.contains("popout")
        || lower.contains("modal")
    {
        Some("Backgrounds")
    } else if lower.contains("border") || lower.contains("line") {
        Some("Borders")
    } else if lower.contains("size")
        || lower.contains("spacing")
        || lower.contains("shape")
        || lower.contains("round")
    {
        Some("Shape")
    } else if lower.contains("image") || lower.contains("picture") || lower.contains("icon") {
        Some("Pictures")
    } else if lower.contains("animation")
        || lower.contains("transition")
        || lower.contains("blur")
        || lower.contains("transparen")
        || lower.contains("effect")
    {
        Some("Effects")
    } else if lower.contains("top bar")
        || lower.contains("chatbar")
        || lower.contains("window")
        || lower.contains("button")
        || lower.contains("dms")
        || lower.contains("label")
        || lower.contains("access")
        || lower.contains("styling")
        || lower.contains("interface")
    {
        Some("Interface")
    } else {
        None
    }
}

fn group_for(name: &str) -> &'static str {
    let name = name.to_ascii_lowercase();
    let picture = (name.contains("image")
        || name.contains("picture")
        || name.contains("logo")
        || name.contains("avatar")
        || name.contains("icon"))
        && !name.contains("color")
        && !name.contains("colour");
    if font_name(&name)
        || name.contains("letter-spacing")
        || name.contains("tracking")
        || name.contains("font-weight")
        || name == "rl-weight"
    {
        "Typography"
    } else if picture {
        "Pictures"
    } else if name.contains("radius")
        || name.contains("roundness")
        || name.contains("rounding")
        || name.ends_with("-round")
        || name.ends_with("_round")
        || name.contains("-round-")
        || name.contains("gap")
        || name.contains("thickness")
        || name.contains("height")
        || name.contains("width")
        || name.contains("size")
        || name.contains("spacing")
        || name.contains("padding")
        || name.contains("margin")
        || name.contains("bottom")
    {
        "Shape"
    } else if name.contains("opacity")
        || name.contains("blur")
        || name.contains("shadow")
        || name.contains("animation")
        || name.contains("motion")
        || name.contains("transition")
        || name.contains("duration")
        || name.contains("scanline")
        || name.contains("grain")
        || name.contains("vignette")
        || name.contains("flicker")
        || name.contains("beam")
        || name.contains("filter")
        || name.contains("brightness")
        || name.contains("glow")
        || name.contains("gradient")
    {
        "Effects"
    } else if name.contains("border")
        || name.contains("line")
        || name.contains("divider")
        || name.contains("outline")
        || name.contains("spine")
    {
        "Borders"
    } else if name.contains("background")
        || name.contains("surface")
        || name.contains("panel")
        || name.contains("bg-")
        || name.contains("chat")
        || name.contains("sidebar")
        || name.contains("rail")
        || name.contains("card")
        || name.contains("input")
        || name.contains("composer")
        || name.contains("field")
        || name.contains("shading")
        || name.contains("tint")
        || name.contains("modal")
        || name.contains("popout")
    {
        "Backgrounds"
    } else if name.contains("accent")
        || name.contains("brand")
        || name.contains("main-color")
        || name.contains("link")
        || name.contains("mention")
        || name.contains("highlight")
        || name.contains("primary")
    {
        "Accents"
    } else if name.contains("status")
        || name.contains("online")
        || name.contains("idle")
        || name.contains("dnd")
        || name.contains("streaming")
        || name.contains("offline")
        || name.contains("success")
        || name.contains("danger")
        || name.contains("warning")
        || name.contains("positive")
        || name.contains("negative")
    {
        "Status"
    } else if name.contains("text")
        || name.contains("fg")
        || name.contains("foreground")
        || name.contains("header")
        || name.contains("muted")
        || name.contains("channel-normal")
    {
        "Text"
    } else if name.contains("button")
        || name.contains("toggle")
        || name.contains("switch")
        || name.contains("pill")
        || name.contains("control")
        || name.contains("window")
        || name.contains("top-bar")
        || name.contains("chatbar")
        || name.contains("toolbar")
        || name.contains("label")
        || name.contains("spotify")
        || name.contains("ascii")
        || name.contains("scrollbar")
    {
        "Interface"
    } else if name.starts_with("--ambxst-") {
        ambxst_group(&name)
    } else if name.starts_with("--ctp-")
        || name.starts_with("--tn-")
        || name.starts_with("--gb-")
        || name.starts_with("--sol-")
        || name.starts_with("--nord")
    {
        palette_group(&name)
    } else {
        "Other"
    }
}

/// A bundled theme's own palette names. Accent-like colours go under
/// Accents; grounds and inks go under Base colours.
fn palette_group(name: &str) -> &'static str {
    if name.contains("accent")
        || name.contains("orange")
        || name.contains("yellow")
        || name.contains("green")
        || name.contains("aqua")
        || name.contains("cyan")
        || name.contains("blue")
        || name.contains("purple")
        || name.contains("magenta")
        || name.contains("violet")
        || name.contains("red")
        || name.contains("mauve")
        || name.contains("pink")
    {
        "Accents"
    } else {
        "Base colours"
    }
}

/// The Ambxst shell's own settings, grouped by the part they dress.
fn ambxst_group(name: &str) -> &'static str {
    if name.contains("opacity")
        || name.contains("transparency")
        || name.contains("blur")
        || name.contains("shadow")
        || name.contains("gradient")
        || name.contains("elevation")
    {
        "Effects"
    } else if name.contains("primary")
        || name.contains("secondary")
        || name.contains("tertiary")
        || name.contains("error")
        || name.contains("surface")
        || name.contains("outline")
        || name.contains("scrim")
        || name.contains("inverse")
        || name.contains("overlay")
        || name.contains("border-color")
    {
        "Base colours"
    } else if name.contains("font")
        || name.contains("size")
        || name.contains("weight")
        || name.contains("tracking")
        || name.contains("height")
    {
        "Typography"
    } else {
        "Interface"
    }
}

fn group_order(group: &str) -> u8 {
    let lower = group.to_ascii_lowercase();
    match group {
        "ReCore look" => 0,
        "ReCord effects" => 1,
        "Background" => 2,
        "ReCord palette" => 3,
        "Backgrounds" => 4,
        "Text" => 5,
        "Channels" => 6,
        "Accents" => 7,
        "Status" => 8,
        "Borders" => 9,
        "Interface" => 10,
        "Base colours" => 11,
        "Typography" => 12,
        "Shape" => 13,
        "Pictures" => 14,
        "Effects" => 15,
        "Other" => 16,
        _ if lower.contains("background") => 4,
        _ if lower.contains("text") || lower.contains("font") => 5,
        _ if lower.contains("channel") => 6,
        _ if lower.contains("accent") || lower.contains("color") => 7,
        _ if lower.contains("status") => 8,
        _ if lower.contains("border") => 9,
        _ if lower.contains("interface") => 10,
        _ if lower.contains("type") => 12,
        _ if lower.contains("size") || lower.contains("shape") => 13,
        _ if lower.contains("picture") || lower.contains("image") => 14,
        _ if lower.contains("effect") || lower.contains("animation") => 15,
        _ => 17,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::{Declaration, Vars};

    fn vars(entries: &[(&str, &str)]) -> Vars {
        let mut vars = Vars::default();
        for (name, value) in entries {
            vars.insert(&Declaration {
                name: (*name).into(),
                value: (*value).into(),
                important: false,
            });
        }
        vars
    }

    #[test]
    fn record_options_become_controls() {
        let vars = vars(&[
            ("--rl-accent", "#e94221"),
            ("--rl-scanline-opacity", "0.25"),
            ("--rl-motion", "full"),
            ("--rl-icons", "on"),
            ("--rl-background-image", "url(\"bg.png\")"),
            ("--rl-font", "\"Pixel Operator\", monospace"),
        ]);
        let options = detect(&vars, true);
        let find = |name: &str| options.iter().find(|option| option.name == name).unwrap();
        assert!(matches!(find("--rl-accent").kind, OptionKind::Color { .. }));
        assert!(matches!(
            find("--rl-scanline-opacity").kind,
            OptionKind::Percent {
                value: 25.0,
                percent: false
            }
        ));
        assert!(matches!(
            find("--rl-motion").kind,
            OptionKind::Choice { .. }
        ));
        assert!(matches!(
            find("--rl-icons").kind,
            OptionKind::Toggle { on: true, .. }
        ));
        assert!(matches!(
            find("--rl-background-image").kind,
            OptionKind::Image { .. }
        ));
        assert!(matches!(find("--rl-font").kind, OptionKind::Font));
    }

    #[test]
    fn a_numeric_icon_switch_is_a_toggle() {
        let vars = vars(&[("--settingsicons", "1"), ("--z-index", "1")]);
        let options = detect(&vars, true);
        let find = |name: &str| options.iter().find(|option| option.name == name).unwrap();
        assert!(matches!(
            find("--settingsicons").kind,
            OptionKind::Toggle { on: true, .. }
        ));
        assert!(matches!(find("--z-index").kind, OptionKind::Text));
    }

    #[test]
    fn a_large_embedded_picture_is_still_offered_for_clearing() {
        let data = format!("url(\"data:image/png;base64,{}\")", "A".repeat(3000));
        let vars = vars(&[("--background-image", data.as_str())]);
        let options = detect(&vars, true);
        let image = options
            .iter()
            .find(|option| option.name == "--background-image")
            .expect("the picture is offered");
        assert!(matches!(
            image.kind,
            OptionKind::Image {
                embedded: true,
                url: None
            }
        ));
        assert!(image.value.is_empty());
    }

    #[test]
    fn property_rules_type_and_default_an_option() {
        let css = r#"
        @property --brand {
            syntax: "<color>";
            inherits: false;
            initial-value: #123456;
        }
        :root { --brand: #abcdef; }
        "#;
        let theme =
            crate::load_str(css, None, &crate::Options::with_mode(crate::Mode::Dark)).unwrap();
        let brand = theme
            .options
            .iter()
            .find(|option| option.name == "--brand")
            .unwrap();
        assert!(matches!(brand.kind, OptionKind::Color { .. }));
        assert_eq!(brand.default_value.as_deref(), Some("#123456"));
        assert!(brand.recommended);
    }

    #[test]
    fn an_option_only_in_the_other_appearance_says_so() {
        let css = r#"
        :root { --shared: #111111; }
        .theme-light { --light-only: #eeeeee; }
        "#;
        let theme =
            crate::load_str(css, None, &crate::Options::with_mode(crate::Mode::Dark)).unwrap();
        let light = theme
            .options
            .iter()
            .find(|option| option.name == "--light-only")
            .unwrap();
        assert_eq!(light.only_in, Some(false));
        let shared = theme
            .options
            .iter()
            .find(|option| option.name == "--shared")
            .unwrap();
        assert_eq!(shared.only_in, None);
    }

    #[test]
    fn numbered_scales_past_255_are_still_recognised() {
        let css = ":root { --green-360: #79b59a; --red-400: #ff7a61; }";
        let theme =
            crate::load_str(css, None, &crate::Options::with_mode(crate::Mode::Dark)).unwrap();
        let green = theme
            .options
            .iter()
            .find(|option| option.name == "--green-360")
            .unwrap();
        assert_eq!(green.group, "Base colours");
        assert_eq!(green.label, "Green 360");
    }

    #[test]
    fn comments_and_sections_name_and_group_options() {
        let css = r#"
        :root {
            /* text colors */
            --text-main: #ffffff; /* normal text */
            /* background and dark colors */
            --bg-main: #111111; /* main background color */
            /* accent colors */
            --accent-one: #ff0000; /* buttons and links */
        }
        "#;
        let theme =
            crate::load_str(css, None, &crate::Options::with_mode(crate::Mode::Dark)).unwrap();
        let find = |name: &str| {
            theme
                .options
                .iter()
                .find(|option| option.name == name)
                .unwrap()
        };
        assert_eq!(find("--text-main").group, "Text");
        assert_eq!(
            find("--text-main").description.as_deref(),
            Some("normal text")
        );
        assert_eq!(find("--bg-main").group, "Backgrounds");
        assert_eq!(
            find("--bg-main").description.as_deref(),
            Some("main background color")
        );
        assert_eq!(find("--accent-one").group, "Accents");
    }

    #[test]
    fn a_comment_can_turn_a_value_into_choices() {
        let css = r#"
        :root {
            --top-bar-position: titlebar; /* off: default, hide: no title, titlebar: move it up */
        }
        "#;
        let theme =
            crate::load_str(css, None, &crate::Options::with_mode(crate::Mode::Dark)).unwrap();
        let option = theme
            .options
            .iter()
            .find(|option| option.name == "--top-bar-position")
            .unwrap();
        assert!(matches!(
            &option.kind,
            OptionKind::Choice { choices }
                if choices.iter().any(|(_, value)| value == "titlebar")
                    && choices.iter().any(|(_, value)| value == "hide")
        ));
    }

    #[test]
    fn a_comment_can_list_choices_without_colons() {
        let css = r#"
        :root {
            --custom-dms-background: off; /* off to disable, image to use a picture, color to use a gradient */
            --chatbar-height: 47px; /* height of the chatbar (56px by default, 47px to align with the user panel) */
        }
        "#;
        let theme =
            crate::load_str(css, None, &crate::Options::with_mode(crate::Mode::Dark)).unwrap();
        let background = theme
            .options
            .iter()
            .find(|option| option.name == "--custom-dms-background")
            .unwrap();
        assert!(matches!(
            &background.kind,
            OptionKind::Choice { choices }
                if choices.len() == 3
                    && choices.iter().any(|(_, value)| value == "image")
                    && choices.iter().any(|(_, value)| value == "color")
        ));
        let height = theme
            .options
            .iter()
            .find(|option| option.name == "--chatbar-height")
            .unwrap();
        assert!(!matches!(height.kind, OptionKind::Choice { .. }));
    }

    #[test]
    fn betterdiscord_vars_label_and_type_options() {
        let css = r#"
        /**
         * @name DiscordRecolor
         * @var checkbox settingsicons_s "User Settings Icons" 1
         * @var text backgroundprimary_s "Background 1: [default] = 54, 57, 63" 30, 30, 30
         * @var select corner "Corners" round {Round: round, Square: square}
         * @var slider gap "Gap" 8 {min: 0, max: 20, step: 2}
         */
        :root {
            --settingsicons: 1;
            --backgroundprimary: 30, 30, 30;
            --corner: round;
            --gap: 8;
        }
        "#;
        let theme =
            crate::load_str(css, None, &crate::Options::with_mode(crate::Mode::Dark)).unwrap();
        let settings = theme
            .options
            .iter()
            .find(|option| option.name == "--settingsicons")
            .unwrap();
        assert_eq!(settings.label, "User Settings Icons");
        assert!(matches!(settings.kind, OptionKind::Toggle { on: true, .. }));
        let background = theme
            .options
            .iter()
            .find(|option| option.name == "--backgroundprimary")
            .unwrap();
        assert_eq!(background.label, "Background 1");
        assert_eq!(background.group, "Backgrounds");
        assert!(matches!(background.kind, OptionKind::Color { .. }));
        let corner = theme
            .options
            .iter()
            .find(|option| option.name == "--corner")
            .unwrap();
        assert_eq!(corner.label, "Corners");
        assert!(matches!(
            &corner.kind,
            OptionKind::Choice { choices }
                if choices.iter().any(|(label, value)| label == "Square" && value == "square")
        ));
        let gap = theme
            .options
            .iter()
            .find(|option| option.name == "--gap")
            .unwrap();
        assert_eq!(gap.label, "Gap");
        assert!(matches!(
            gap.kind,
            OptionKind::Range {
                value: 8,
                min: 0,
                max: 20,
                step: 2,
                ..
            }
        ));
    }

    #[test]
    fn font_size_is_a_length_and_font_colour_is_a_colour() {
        let vars = vars(&[
            ("--font-primary", "\"gg sans\", sans-serif"),
            ("--font-size", "16px"),
            ("--font-weight", "600"),
            ("--font-color", "#f1dedd"),
        ]);
        let options = detect(&vars, true);
        let kind = |name: &str| {
            options
                .iter()
                .find(|option| option.name == name)
                .map(|option| &option.kind)
        };
        assert!(matches!(kind("--font-primary"), Some(OptionKind::Font)));
        assert!(matches!(
            kind("--font-size"),
            Some(OptionKind::Number { value: 16.0, unit }) if unit == "px"
        ));
        assert!(matches!(kind("--font-weight"), Some(OptionKind::Text)));
        assert!(matches!(
            kind("--font-color"),
            Some(OptionKind::Color { .. })
        ));
    }

    #[test]
    fn generic_discord_tokens_are_readable() {
        let vars = vars(&[
            ("--background-primary", "#313338"),
            ("--text-normal", "#dbdee1"),
            ("--radius-md", "8px"),
            ("--font-primary", "\"gg sans\", sans-serif"),
        ]);
        let options = detect(&vars, true);
        assert!(options.iter().any(|option| option.group == "Backgrounds"));
        assert!(options.iter().any(|option| option.group == "Text"));
        assert!(options.iter().any(|option| option.group == "Shape"));
        assert!(options.iter().any(|option| option.group == "Typography"));
        assert_eq!(
            options
                .iter()
                .find(|option| option.name == "--background-primary")
                .unwrap()
                .label,
            "Chat background"
        );
    }
}
