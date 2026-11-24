//! Appearance: the themes as small windows drawn in their own colours (a
//! click shows one at once and keeps it), New theme and each palette theme's
//! pencil opening that editor, whose changes show at once (Save keeps them
//! among the person's own themes, Cancel puts the theme from before back),
//! New CSS theme and each CSS tile's pencil opening the whole `.theme.css`
//! file, and the profile switches. See docs/themes.md.

use std::{
    collections::BTreeSet,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Space, button, column, container, row, stack, text},
};
use recore_core::{
    application::Action,
    theme::{self as palette, Custom},
};
use recore_css::{OptionKind, ThemeOption};

use super::pages::{Pref, ZOOM_SIZES};
use super::{Drop, Event as SettingsEvent, Page, Settings};
use crate::{
    app::Message,
    controls::{Kind, action, lights, placed, slider},
    ctx::{Cx, Focus, Pref as Shown, Request},
    field,
    fonts::{BOLD, MEDIUM},
    form::{self, switch},
    pictures::Icon,
    themes::color,
    widgets::{Hsv, hsv_rgb, rgb_hsv},
};

/// A theme's tile: at least this wide, this tall, this far apart.
const TILE: f32 = 150.0;
const TILE_HEIGHT: f32 = 124.0;
const TILE_GAP: f32 = 12.0;
/// The editor's colours: two columns from this width, each row this tall.
const TWO_COLUMNS: f32 = 520.0;
const COLOUR_GAP: f32 = 24.0;
/// The name field, and the CSS source's field.
const NAME: &str = "theme-name";
const CSS_NAME: &str = "css-theme-name";
const CSS_SOURCE: &str = "css-theme-source";
/// The lines of a CSS theme's source shown at once.
const CSS_LINES: u16 = 20;

/// The theme editor while it's open: the theme as edited, each colour's
/// text and whether it's wrong, its name, and what it started from.
pub struct Editor {
    /// The custom theme edited, by id; none for a new one.
    pub editing: Option<String>,
    pub base: &'static str,
    pub theme: palette::Theme,
    pub name: String,
    /// Each colour's field, in `Theme::NAMES`' order: its name as
    /// `layout.json` has it, its text and whether it's a colour.
    pub fields: Vec<(String, String, bool)>,
    /// The theme a new one started as, among `palette::starts`.
    pub start: usize,
    /// Delete was pressed once; the next press deletes.
    pub confirming: bool,
}

impl Editor {
    fn new(
        editing: Option<String>,
        base: &'static str,
        theme: palette::Theme,
        name: String,
    ) -> Self {
        let mut editor = Self {
            editing,
            base,
            theme,
            name,
            fields: Vec::new(),
            start: 0,
            confirming: false,
        };
        editor.fill();
        editor
    }

    /// Every field's text from the theme's colours.
    fn fill(&mut self) {
        self.fields = palette::Theme::NAMES
            .iter()
            .enumerate()
            .filter_map(|(index, name)| {
                Some((
                    palette::token(name),
                    palette::hex(self.theme.colour(index)?),
                    false,
                ))
            })
            .collect();
    }

    /// Takes a colour's text as typed: the theme takes it if it's a colour.
    /// Returns whether the theme changed.
    fn edit(&mut self, index: usize, text: String) -> bool {
        let parsed = palette::parse(&text);
        let Some(field) = self.fields.get_mut(index) else {
            return false;
        };
        (field.1, field.2) = (text, parsed.is_none());
        match (parsed, self.theme.colour_mut(index)) {
            (Some(colour), Some(slot)) if *slot != colour => {
                *slot = colour;
                true
            }
            _ => false,
        }
    }

    fn valid(&self) -> bool {
        self.fields.iter().all(|(_, _, wrong)| !wrong)
    }
}

/// A CSS theme's source while it's edited. The whole file is kept, so the
/// person can change colours, fonts, background pictures, gradients and
/// textures exactly as the original CSS does.
pub struct CssEditor {
    /// The theme edited, by id; none for a new file.
    pub editing: Option<String>,
    pub name: String,
    pub draft: field::Draft,
    /// The file edited; none for a new theme, which is named as saved.
    pub path: Option<PathBuf>,
    pub error: Option<String>,
    /// Delete was pressed once; the next press deletes the file.
    pub confirming_delete: bool,
}

impl CssEditor {
    fn new(editing: Option<String>, name: String, path: Option<PathBuf>, source: &str) -> Self {
        let draft = field::Draft::default();
        draft.replace(source, source.len());
        Self {
            editing,
            name,
            draft,
            path,
            error: None,
            confirming_delete: false,
        }
    }

    fn source(&self) -> String {
        self.draft.text()
    }

    /// Adds a declaration block at the end, where it wins over the theme's
    /// own values, and keeps the caret after it.
    fn append(&mut self, css: &str) {
        let mut source = self.source();
        if !source.ends_with('\n') {
            source.push('\n');
        }
        source.push('\n');
        source.push_str(css.trim());
        source.push('\n');
        self.draft.replace(&source, source.len());
        self.error = None;
        self.confirming_delete = false;
    }
}

/// A texture the CSS editor can add to the file: ReCord's own surfaces, which
/// any CSS theme can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Texture {
    Scanlines,
    Grain,
    Vignette,
}

#[derive(Clone, Debug)]
pub enum Event {
    Choose(String),
    New,
    NewCss,
    Edit(String),
    EditCss(String),
    CssName(String),
    CssSource(field::Event),
    CssSourceText(String),
    CssBackground,
    CssTexture(Texture),
    SaveCss,
    CancelCss,
    Start(usize),
    Colour(usize, String),
    Dark(bool),
    Name(String),
    Save,
    Cancel,
    Delete,
    Profile(Cosmetic, bool),
    Reload,
    OpenThemes,
    Accent(String),
    Preset(&'static str),
    ResetAccent,
    Effects(bool),
    LowPower(bool),
    Scanlines(u8),
    Grain(u8),
    Vignette(u8),
    Beam(bool),
    Flicker(bool),
    Frame(bool),
    AvatarFilter(bool),
    Background,
    ClearBackground,
    BackgroundDim(u8),
    PanelOpacity(u8),
    BackgroundFilter(crate::background::Filter),
    BackgroundBrightness(u8),
    BackgroundAnimate(bool),
    Font(&'static str),
    Motion(crate::themes::Motion),
    /// A colour row's picker opened or closed, by its target and its HSV.
    PickColour(String, Hsv),
    Hsv(Hsv),
    /// A CSS option while it's being typed, and when it's applied.
    CssDraft(String, String),
    CssApply(String, String),
    /// The picture button beside a CSS image option.
    CssImage(String),
    CssReset,
    /// Reset one custom property, or every changed property in a group.
    CssResetOne(String),
    CssResetGroup(String),
    /// Show every custom property, not only the readable ones.
    CssAll(bool),
    /// Show only recommended options, or only changed ones.
    CssRecommended(bool),
    CssChangedOnly(bool),
    /// Filter the chosen CSS theme's options by what they say.
    CssSearch(String),
    /// Open or close one group of the chosen CSS theme's options.
    CssGroup(String),
    /// Open every group of options, or close them all.
    CssGroups(bool),
    /// A theme URL typed and imported.
    ThemeUrl(String),
    ThemeImport,
    /// A local `.theme.css` file copied into ReCore's folder.
    ThemeImportFile,
    /// The changed options saved as a small CSS file.
    CssExport,
    /// The whole CSS theme written to a file the person picks.
    CssExportSource,
    /// A copy of the edited theme, opened for editing.
    CssDuplicate,
    /// Delete the edited CSS theme; the first press asks to confirm.
    CssDelete,
}

/// What an Appearance tile is: one ReCore comes with, one the person made,
/// or a Discord CSS theme in a folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tile {
    BuiltIn,
    Custom,
    Css,
}

/// Appearance's profile switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cosmetic {
    Effects,
    Decorations,
    Nameplates,
    Moving,
    NameStyles,
    ServerTags,
    Frames,
}

fn theme_message(event: Event) -> Message {
    Message::Settings(SettingsEvent::Theme(event))
}

impl Settings {
    pub(super) fn theme_event(&mut self, event: Event, cx: &mut Cx<'_>) {
        match event {
            Event::Choose(id) => {
                self.css_text.clear();
                self.css_search.clear();
                self.css_open.clear();
                choose(cx, &id);
            }
            Event::New => self.new_theme(cx),
            Event::NewCss => self.new_css_theme(cx),
            Event::EditCss(id) => self.edit_css_theme(cx, &id),
            Event::CssName(name) => {
                if let Some(editor) = self.css_editor.as_mut() {
                    editor.name = name;
                }
            }
            Event::CssSource(event) => {
                if let Some(editor) = self.css_editor.as_mut() {
                    editor.error = None;
                }
                if matches!(event, field::Event::Escape) {
                    self.cancel_css_theme(cx);
                }
            }
            Event::CssSourceText(text) => {
                if let Some(editor) = self.css_editor.as_mut() {
                    editor.error = None;
                    editor.confirming_delete = false;
                    editor.draft.replace(&text, text.len());
                }
            }
            Event::CssBackground => self.css_background(),
            Event::CssTexture(texture) => self.css_texture(texture),
            Event::SaveCss => self.save_css_theme(cx),
            Event::CancelCss => self.cancel_css_theme(cx),
            Event::Edit(id) => {
                if let Some(custom) = cx.themes.custom.iter().find(|custom| custom.id == id) {
                    let editor = Editor::new(
                        Some(custom.id.clone()),
                        custom.base,
                        custom.theme,
                        custom.name.clone(),
                    );
                    self.open_editor(editor, cx);
                    return;
                }
                // A built-in theme is edited as a copy of itself, saved among
                // the person's own.
                let Some(built_in) = palette::find(&id) else {
                    return;
                };
                let editor =
                    Editor::new(None, built_in.id, built_in.theme, copy_name(built_in.name));
                self.open_editor(editor, cx);
            }
            Event::Start(index) => {
                let starts: Vec<_> = palette::starts(&cx.themes.custom)
                    .map(|start| (start.base, start.theme, copy_name(start.name)))
                    .collect();
                let Some(editor) = self.editor.as_mut() else {
                    return;
                };
                let Some((base, theme, name)) = starts.get(index).cloned() else {
                    return;
                };
                // It takes the new start's name too, unless the person
                // named it.
                if starts
                    .get(editor.start)
                    .is_some_and(|before| before.2 == editor.name)
                {
                    editor.name = name;
                }
                editor.base = base;
                editor.theme = theme;
                editor.start = index;
                editor.fill();
                self.open = None;
                self.show_edited(cx);
            }
            Event::Colour(index, text) => {
                let changed = self
                    .editor
                    .as_mut()
                    .is_some_and(|editor| editor.edit(index, text));
                if changed {
                    self.show_edited(cx);
                }
            }
            Event::Dark(dark) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.theme.dark = dark;
                }
                self.show_edited(cx);
            }
            Event::Name(name) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.name = name;
                }
            }
            Event::Save => self.save_theme(cx),
            Event::Cancel => self.cancel_theme(cx),
            Event::Delete => self.delete_theme(cx),
            Event::Profile(cosmetic, on) => self::cosmetic(cx, cosmetic, on),
            Event::Reload => {
                self.css_text.clear();
                self.css_search.clear();
                self.css_open.clear();
                crate::themes::clear_remote_cache();
                cx.themes.reload();
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::OpenThemes => crate::themes::open_folder(cx.themes.themes_folder()),
            Event::ThemeUrl(url) => self.theme_url = url,
            Event::CssExport => {
                let Some(css) = cx.themes.chosen_css() else {
                    return;
                };
                let id = cx.themes.chosen_id().to_owned();
                let Some(changed) = cx.themes.overrides.get(&id).cloned() else {
                    return;
                };
                let name = css.name.clone();
                let file_name = format!(
                    "{}-overrides.css",
                    name.chars()
                        .map(|character| if character.is_ascii_alphanumeric() {
                            character
                        } else {
                            '-'
                        })
                        .collect::<String>()
                        .trim_matches('-')
                );
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSS", &["css"])
                    .set_file_name(file_name)
                    .save_file()
                else {
                    return;
                };
                let mut text = format!(
                    "/* ReCore changes to {name}. Load this after the theme. */
:root {{
"
                );
                for (option, value) in changed {
                    if !option.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                    }) {
                        continue;
                    }
                    let value = value.replace(['\n', '\r'], " ").trim().to_owned();
                    let _ = writeln!(text, "  {option}: {value};");
                }
                text.push_str(
                    "}
",
                );
                self.theme_url_note = Some(match std::fs::write(&path, text) {
                    Ok(()) => format!("Saved the changes to {}.", path.display()),
                    Err(error) => format!("Could not save the changes: {error}"),
                });
            }
            Event::ThemeImport => {
                let url = self.theme_url.trim().to_owned();
                if url.is_empty() {
                    self.theme_url_note = Some("Paste a theme URL first.".into());
                } else {
                    self.theme_url_note = Some("Downloading the theme...".into());
                    cx.request(Request::ImportTheme(url));
                }
            }
            Event::ThemeImportFile => self.import_css_files(cx),
            Event::CssExportSource => self.export_css_source(),
            Event::CssDuplicate => self.duplicate_css_theme(cx),
            Event::CssDelete => self.delete_css_theme(cx),
            Event::Accent(text) => {
                self.accent = Some(text.clone());
                if let Some(accent) = palette::parse(&text) {
                    let root = cx.client.root().to_path_buf();
                    cx.themes.set_accent(&root, Some(accent));
                    cx.request(Request::Changed(Shown::Theme));
                }
            }
            Event::Preset(name) => {
                let (hex, theme) = preset(name);
                let accent = palette::parse(hex);
                let root = cx.client.root().to_path_buf();
                cx.themes.set_accent(&root, accent);
                self.accent = Some(hex.to_owned());
                if cx.themes.chosen_id() != theme {
                    cx.themes.choose(&root, theme);
                }
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::ResetAccent => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_accent(&root, None);
                self.accent = Some(palette::hex(palette::RECORE_DARK.accent));
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::Effects(on) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_effects(&root, on);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::LowPower(on) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_low_power(&root, on);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::Scanlines(percent) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_scanlines(&root, percent);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::Grain(percent) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_grain(&root, percent);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::Vignette(percent) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_vignette(&root, percent);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::Beam(on) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_beam(&root, on);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::Flicker(on) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_flicker(&root, on);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::Frame(on) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_frame(&root, on);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::AvatarFilter(on) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_avatar_filter(&root, on);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::Background => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "Picture",
                        &[
                            "png", "jpg", "jpeg", "gif", "webp", "bmp", "mp4", "webm", "mov", "mkv",
                        ],
                    )
                    .pick_file()
                {
                    let root = cx.client.root().to_path_buf();
                    cx.themes.set_background(&root, Some(path));
                    cx.request(Request::Changed(Shown::Theme));
                }
            }
            Event::ClearBackground => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_background(&root, None);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::BackgroundDim(percent) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_background_dim(&root, percent);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::PanelOpacity(percent) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_panel_opacity(&root, percent);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::BackgroundFilter(filter) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_background_filter(&root, filter);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::BackgroundBrightness(percent) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_background_brightness(&root, percent);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::BackgroundAnimate(on) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_background_animate(&root, on);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::Font(name) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_font(&root, name);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::Motion(motion) => {
                let root = cx.client.root().to_path_buf();
                cx.themes.set_motion(&root, motion);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::PickColour(id, hsv) => {
                if self.color_open.as_deref() == Some(id.as_str()) {
                    self.color_open = None;
                } else {
                    self.color_open = Some(id);
                    self.color_hsv = hsv;
                }
            }
            Event::Hsv(hsv) => {
                self.color_hsv = hsv;
                let Some(target) = self.color_open.clone() else {
                    return;
                };
                let hex = format!("#{:06x}", hsv_rgb(hsv));
                let value = if let Some(name) = target.strip_prefix("css:") {
                    let previous = self
                        .css_text
                        .get(name)
                        .map(String::as_str)
                        .or_else(|| {
                            cx.themes
                                .chosen_css()
                                .and_then(|css| {
                                    css.options.iter().find(|option| option.name == name)
                                })
                                .map(|option| option.value.as_str())
                        })
                        .unwrap_or("");
                    preserve_alpha(&hex, previous)
                } else if let Some(index) = target
                    .strip_prefix("theme:")
                    .and_then(|index| index.parse::<usize>().ok())
                {
                    let previous = self
                        .editor
                        .as_ref()
                        .and_then(|editor| editor.fields.get(index))
                        .map(|field| field.1.as_str())
                        .unwrap_or("");
                    preserve_alpha(&hex, previous)
                } else {
                    hex
                };
                if target == "accent" {
                    if let Some(accent) = palette::parse(&value) {
                        let root = cx.client.root().to_path_buf();
                        cx.themes.set_accent(&root, Some(accent));
                        self.accent = Some(value);
                        cx.request(Request::Changed(Shown::Theme));
                    }
                } else if let Some(name) = target.strip_prefix("css:") {
                    self.css_text.insert(name.to_owned(), value.clone());
                    self.css_apply(cx, name, &value);
                } else if let Some(index) = target
                    .strip_prefix("theme:")
                    .and_then(|index| index.parse().ok())
                {
                    let changed = self
                        .editor
                        .as_mut()
                        .is_some_and(|editor| editor.edit(index, value));
                    if changed {
                        self.show_edited(cx);
                    }
                }
            }
            Event::CssDraft(name, value) => {
                self.css_text.insert(name.clone(), value.clone());
                // Colours show as they become valid; other values wait for
                // Enter or the slider's release, so a half-typed font or
                // path doesn't reload the theme on every key.
                let is_color = cx
                    .themes
                    .chosen_css()
                    .and_then(|css| css.options.iter().find(|option| option.name == name))
                    .is_some_and(|option| matches!(option.kind, OptionKind::Color { .. }));
                if is_color && recore_css::color::parse(&value, !cx.theme.dark).is_some() {
                    self.css_apply(cx, &name, &value);
                }
            }
            Event::CssApply(name, value) => self.css_apply(cx, &name, &value),
            Event::CssImage(name) => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "Picture",
                        &[
                            "png", "jpg", "jpeg", "gif", "webp", "bmp", "mp4", "webm", "mov", "mkv",
                        ],
                    )
                    .pick_file()
                {
                    let value = path.display().to_string();
                    self.css_text.insert(name.clone(), value.clone());
                    self.css_apply(cx, &name, &value);
                }
            }
            Event::CssReset => {
                let id = cx.themes.chosen_id().to_owned();
                let names: Vec<String> = cx
                    .themes
                    .chosen_css()
                    .map(|css| {
                        css.options
                            .iter()
                            .map(|option| option.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                self.css_text.retain(|name, _| !names.contains(name));
                let root = cx.client.root().to_path_buf();
                cx.themes.clear_options(&root, &id);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::CssAll(on) => self.show_all_css = on,
            Event::CssRecommended(on) => self.css_recommended_only = on,
            Event::CssChangedOnly(on) => self.css_changed_only = on,
            Event::CssResetOne(name) => {
                let id = cx.themes.chosen_id().to_owned();
                let root = cx.client.root().to_path_buf();
                self.css_text.remove(&name);
                cx.themes.clear_option(&root, &id, &name);
                cx.request(Request::Changed(Shown::Theme));
            }
            Event::CssResetGroup(group) => {
                let id = cx.themes.chosen_id().to_owned();
                let names: BTreeSet<String> = cx
                    .themes
                    .chosen_css()
                    .map(|css| {
                        css.options
                            .iter()
                            .filter(|option| option.group == group)
                            .map(|option| option.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                if !names.is_empty() {
                    self.css_text.retain(|name, _| !names.contains(name));
                    let root = cx.client.root().to_path_buf();
                    cx.themes.clear_named(&root, &id, &names);
                    cx.request(Request::Changed(Shown::Theme));
                }
            }
            Event::CssSearch(query) => self.css_search = query,
            Event::CssGroup(group) => {
                if !self.css_open.remove(&group) {
                    self.css_open.insert(group);
                }
            }
            Event::CssGroups(open) => {
                self.css_open.clear();
                if open && let Some(css) = cx.themes.chosen_css() {
                    self.css_open
                        .extend(css.options.iter().map(|option| option.group.clone()));
                }
            }
        }
    }

    /// Applies one of a CSS theme's options and keeps it for that theme.
    fn css_apply(&mut self, cx: &mut Cx<'_>, name: &str, value: &str) {
        let Some(option) = cx
            .themes
            .chosen_css()
            .and_then(|css| css.options.iter().find(|option| option.name == name))
            .cloned()
        else {
            return;
        };
        if matches!(option.kind, OptionKind::Color { .. })
            && recore_css::color::parse(value, !cx.theme.dark).is_none()
        {
            return;
        }
        self.css_text.insert(name.to_owned(), value.to_owned());
        let applied = css_option_value(&option, value);
        let id = cx.themes.chosen_id().to_owned();
        let root = cx.client.root().to_path_buf();
        cx.themes.set_option(&root, &id, name, applied);
        cx.request(Request::Changed(Shown::Theme));
    }

    /// Opens the editor on a copy of the theme shown, which can start from
    /// any other.
    fn new_theme(&mut self, cx: &mut Cx<'_>) {
        let current = &cx.theme.id;
        let starts: Vec<_> = palette::starts(&cx.themes.custom)
            .map(|start| {
                (
                    start.id.to_owned(),
                    start.base,
                    start.theme,
                    copy_name(start.name),
                )
            })
            .collect();
        let start = starts
            .iter()
            .position(|(id, ..)| id == current)
            .unwrap_or(0);
        let (_, base, theme, name) = starts[start].clone();
        let mut editor = Editor::new(None, base, theme, name);
        editor.start = start;
        self.open_editor(editor, cx);
    }

    /// Opens a new CSS theme on a commented starter file: colours, panel
    /// shape, type, textures and a place for a background picture.
    fn new_css_theme(&mut self, cx: &mut Cx<'_>) {
        self.open = None;
        self.theme_url_note = None;
        self.css_editor = Some(CssEditor::new(None, "My theme".into(), None, CSS_TEMPLATE));
        cx.request(Request::Focus(Focus::Id(CSS_SOURCE)));
    }

    /// Opens a CSS theme's own file, as written, for editing. Everything in
    /// the file is kept, including its comments, imports, pictures and
    /// effects.
    fn edit_css_theme(&mut self, cx: &mut Cx<'_>, id: &str) {
        let Some(css) = cx.themes.css.iter().find(|css| css.id == id) else {
            return;
        };
        let Some(path) = css.path.clone() else {
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(source) => {
                self.open = None;
                self.theme_url_note = None;
                self.css_editor = Some(CssEditor::new(
                    Some(css.id.clone()),
                    css.name.clone(),
                    Some(path),
                    &source,
                ));
                cx.request(Request::Focus(Focus::Id(CSS_SOURCE)));
            }
            Err(error) => {
                self.theme_url_note = Some(format!("Could not read {}: {error}", path.display()));
            }
        }
    }

    /// Copies local `.theme.css` files into ReCore's own folder and shows
    /// the last one. The picker accepts several files, so a theme pack can
    /// be brought in at once.
    fn import_css_files(&mut self, cx: &mut Cx<'_>) {
        let Some(paths) = rfd::FileDialog::new()
            .add_filter("Discord CSS theme", &["css"])
            .pick_files()
        else {
            return;
        };
        let folder = cx.themes.themes_folder().to_path_buf();
        if let Err(error) = std::fs::create_dir_all(&folder) {
            self.theme_url_note = Some(format!("Could not make {}: {error}", folder.display()));
            return;
        }
        let mut last = None;
        let mut failed = 0;
        for path in paths {
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("imported");
            let target = unique_css_path(&folder, name);
            match std::fs::read_to_string(&path)
                .map_err(|error| error.to_string())
                .and_then(|source| {
                    std::fs::write(&target, source)
                        .map_err(|error| error.to_string())
                        .map(|()| target.clone())
                }) {
                Ok(target) => last = Some(target),
                Err(error) => {
                    failed += 1;
                    self.theme_url_note =
                        Some(format!("Could not import {}: {error}", path.display()));
                }
            }
        }
        cx.themes.reload();
        if let Some(target) = last.as_ref()
            && let Some(id) = css_id_at(&cx.themes, target)
        {
            choose(cx, &id);
        }
        if failed == 0 {
            self.theme_url_note = Some(if last.is_some() {
                "Imported into ReCore's themes folder.".into()
            } else {
                "No theme was chosen.".into()
            });
        }
    }

    /// Writes the whole CSS source to a file the person picks, so a theme
    /// can be shared as one `.theme.css` file.
    fn export_css_source(&mut self) {
        let Some(editor) = self.css_editor.as_ref() else {
            return;
        };
        let name = editor.name.clone();
        let source = editor.source();
        let file_name = format!("{}.theme.css", safe_name(&name, "theme"));
        let Some(path) = rfd::FileDialog::new()
            .add_filter("CSS", &["css"])
            .set_file_name(file_name)
            .save_file()
        else {
            return;
        };
        self.theme_url_note = Some(match std::fs::write(&path, source) {
            Ok(()) => format!("Saved {name} to {}.", path.display()),
            Err(error) => format!("Could not save the theme: {error}"),
        });
    }

    /// Saves a copy of the edited theme under a new name and opens the copy,
    /// leaving the original file untouched.
    fn duplicate_css_theme(&mut self, cx: &mut Cx<'_>) {
        let Some(editor) = self.css_editor.take() else {
            return;
        };
        let folder = cx.themes.themes_folder().to_path_buf();
        let name = copy_name(&editor.name);
        let path = unique_css_path(&folder, &name);
        let source = renamed_meta(&editor.source(), &name);
        if let Some(parent) = path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            let mut editor = editor;
            editor.error = Some(format!("Could not make {}: {error}", parent.display()));
            self.css_editor = Some(editor);
            return;
        }
        if let Err(error) = std::fs::write(&path, source) {
            let mut editor = editor;
            editor.error = Some(format!("Could not save {}: {error}", path.display()));
            self.css_editor = Some(editor);
            return;
        }
        cx.themes.reload();
        match css_id_at(&cx.themes, &path) {
            Some(id) => self.edit_css_theme(cx, &id),
            None => {
                let mut editor = editor;
                editor.error = Some(format!(
                    "Saved {}; Reload CSS will show what could not be read.",
                    path.display()
                ));
                self.css_editor = Some(editor);
            }
        }
    }

    /// Deletes the edited theme's file after a second press.
    fn delete_css_theme(&mut self, cx: &mut Cx<'_>) {
        let Some(editor) = self.css_editor.as_mut() else {
            return;
        };
        if !editor.confirming_delete {
            editor.confirming_delete = true;
            return;
        }
        let Some(editor) = self.css_editor.take() else {
            return;
        };
        let Some(id) = editor.editing.clone() else {
            self.css_editor = Some(editor);
            return;
        };
        let root = cx.client.root().to_path_buf();
        let shown = editor
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| editor.name.clone());
        match cx.themes.delete_css(&root, &id) {
            Ok(()) => {
                self.theme_url_note = Some(format!("Deleted {shown}."));
                cx.request(Request::Changed(Shown::Theme));
            }
            Err(error) => {
                let mut editor = editor;
                editor.error = Some(error);
                self.css_editor = Some(editor);
            }
        }
    }

    /// Adds a background picture the person picks to the CSS, with its dim
    /// and panel opacity so it shows through rather than only changing a
    /// variable the panels hide.
    fn css_background(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Picture",
                &[
                    "png", "jpg", "jpeg", "gif", "webp", "bmp", "mp4", "webm", "mov", "mkv",
                ],
            )
            .pick_file()
        else {
            return;
        };
        let Some(editor) = self.css_editor.as_mut() else {
            return;
        };
        let url = css_url(&path);
        editor.append(&format!(
            ":root {{
  --rl-background-image: url(\"{url}\");
  --rl-background-dim: 0.35;
  --rl-panel-opacity: 0.82;
}}"
        ));
    }

    /// Adds one of ReCord's textures to the CSS. Appending a later `:root`
    /// wins over the theme's own value without rewriting its source.
    fn css_texture(&mut self, texture: Texture) {
        let Some(editor) = self.css_editor.as_mut() else {
            return;
        };
        let (name, value) = match texture {
            Texture::Scanlines => ("--rl-scanline-opacity", "0.08"),
            Texture::Grain => ("--rl-grain-opacity", "0.03"),
            Texture::Vignette => ("--rl-vignette-opacity", "0.18"),
        };
        editor.append(&format!(":root {{\n  {name}: {value};\n}}"));
    }

    /// Writes the CSS and shows it once the reader has it. A file that cannot
    /// be parsed stays open with the reader's reason, so nothing is lost.
    fn save_css_theme(&mut self, cx: &mut Cx<'_>) {
        let Some(mut editor) = self.css_editor.take() else {
            return;
        };
        let folder = cx.themes.themes_folder().to_path_buf();
        let path = match &editor.path {
            Some(path) => path.clone(),
            None => unique_css_path(&folder, &editor.name),
        };
        if let Some(parent) = path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            editor.error = Some(format!("Could not make {}: {error}", parent.display()));
            self.css_editor = Some(editor);
            return;
        }
        let source = editor.source();
        if let Err(error) = recore_css::load_str(&source, path.parent(), &Default::default()) {
            editor.error = Some(error);
            self.css_editor = Some(editor);
            return;
        }
        if let Err(error) = std::fs::write(&path, source) {
            editor.error = Some(format!("Could not save {}: {error}", path.display()));
            self.css_editor = Some(editor);
            return;
        }
        cx.themes.reload();
        let target = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let id = cx.themes.css.iter().find_map(|css| {
            let css_path = css.path.as_ref()?;
            let css_path = std::fs::canonicalize(css_path).unwrap_or_else(|_| css_path.clone());
            (css_path == target).then(|| css.id.clone())
        });
        self.preview = None;
        self.theme_url_note = None;
        match id {
            Some(id) => choose(cx, &id),
            None => {
                self.theme_url_note = Some(format!(
                    "Saved {}; Reload CSS will show what could not be read.",
                    path.display()
                ));
                cx.request(Request::Changed(Shown::Theme));
            }
        }
    }

    /// Closes the source editor without writing anything.
    pub(super) fn cancel_css_theme(&mut self, cx: &mut Cx<'_>) {
        if self.css_editor.take().is_some() {
            cx.request(Request::Changed(Shown::Theme));
        }
    }

    fn open_editor(&mut self, editor: Editor, cx: &mut Cx<'_>) {
        self.editor = Some(editor);
        self.show_edited(cx);
    }

    /// The window in the theme as edited, without keeping it.
    fn show_edited(&mut self, cx: &mut Cx<'_>) {
        let Some(editor) = &self.editor else {
            return;
        };
        let id = editor.editing.as_deref().unwrap_or("editing");
        self.preview = Some(crate::themes::theme(id, &editor.theme));
        cx.request(Request::Changed(Shown::Theme));
    }

    /// Closes the editor: the window shows the theme chosen again.
    fn close_editor(&mut self, cx: &mut Cx<'_>) -> Option<Editor> {
        let editor = self.editor.take()?;
        if self.preview.take().is_some() {
            cx.request(Request::Changed(Shown::Theme));
        }
        Some(editor)
    }

    /// Keeps the theme as edited among the person's own, and chooses it.
    fn save_theme(&mut self, cx: &mut Cx<'_>) {
        if !self.editor.as_ref().is_some_and(Editor::valid) {
            return;
        }
        let Some(editor) = self.close_editor(cx) else {
            return;
        };
        let name = match editor.name.trim() {
            "" => "My theme".to_owned(),
            name => name.to_owned(),
        };
        let custom = &mut cx.themes.custom;
        let saved = editor
            .editing
            .as_ref()
            .and_then(|id| custom.iter_mut().find(|custom| custom.id == *id));
        let id = match saved {
            Some(saved) => {
                saved.name = name;
                saved.theme = editor.theme;
                saved.id.clone()
            }
            None => {
                let id = palette::new_id(custom);
                custom.push(Custom {
                    id: id.clone(),
                    name,
                    base: editor.base,
                    theme: editor.theme,
                });
                id
            }
        };
        cx.keep("custom_themes", &cx.themes.custom);
        choose(cx, &id);
    }

    /// Closes the editor and shows the theme from before it opened.
    pub(super) fn cancel_theme(&mut self, cx: &mut Cx<'_>) {
        self.close_editor(cx);
    }

    /// Deletes the theme edited; if it was the chosen one, Yap shows.
    fn delete_theme(&mut self, cx: &mut Cx<'_>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if !editor.confirming {
            editor.confirming = true;
            return;
        }
        let Some(editor) = self.close_editor(cx) else {
            return;
        };
        if let Some(id) = editor.editing {
            cx.themes.custom.retain(|custom| custom.id != id);
            cx.keep("custom_themes", &cx.themes.custom);
        }
        let (id, _) = palette::chosen(cx.themes.chosen.as_deref(), &cx.themes.custom);
        let id = id.to_owned();
        choose(cx, &id);
    }
}

/// Chooses the theme `id` and keeps the choice, if there's such a theme:
/// the window shows it.
fn choose(cx: &mut Cx<'_>, id: &str) {
    let Some(theme) = cx.themes.find(id) else {
        return;
    };
    cx.themes.choose(cx.client.root(), &theme.id);
    cx.request(Request::Changed(Shown::Theme));
}

/// A profile switch: kept, and the core told what may now show.
fn cosmetic(cx: &mut Cx<'_>, cosmetic: Cosmetic, on: bool) {
    let prefs = &mut cx.layout.prefs;
    let (slot, key) = match cosmetic {
        Cosmetic::Effects => (&mut prefs.profile_effects, "profile_effects"),
        Cosmetic::Decorations => (&mut prefs.decorations, "decorations"),
        Cosmetic::Nameplates => (&mut prefs.nameplates, "nameplates"),
        Cosmetic::Moving => (&mut prefs.moving_cosmetics, "moving_cosmetics"),
        Cosmetic::NameStyles => (&mut prefs.name_styles, "name_styles"),
        Cosmetic::ServerTags => (&mut prefs.server_tags, "server_tags"),
        Cosmetic::Frames => (&mut prefs.profile_frames, "profile_frames"),
    };
    *slot = Some(on);
    cx.keep(key, on);
    if cosmetic == Cosmetic::Nameplates {
        cx.dispatch(Action::HideNameplates(!on));
    } else {
        let cosmetics = cx.layout.prefs.cosmetics();
        cx.dispatch(Action::SetCosmetics(cosmetics));
    }
    cx.request(Request::Changed(Shown::Roster));
}

impl<'a> Page<'a> {
    pub(super) fn appearance_page(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        if let Some(editor) = &settings.editor {
            return self.editor_view(editor, settings.open == Some(Drop::Start));
        }
        if let Some(editor) = &settings.css_editor {
            return self.css_editor_view(editor);
        }
        let tiles = self.theme_tiles();
        let prefs = &self.cx.layout.prefs;
        let cosmetics = prefs.cosmetics();
        let accent_text = settings.accent.as_deref().unwrap_or("#e94221");
        let accent_wrong = palette::parse(accent_text).is_none();
        let profile = |label, description, on: bool, which| {
            switch(label, on, Some(theme_message(Event::Profile(which, !on))))
                .description(description)
                .view(self.cx.look)
        };
        form::page([
            form::title(theme, "Appearance"),
            form::group([
                form::beside(
                    form::heading(theme, "THEME"),
                    row![
                        action(
                            theme,
                            "Reload CSS",
                            Kind::Plain,
                            Some(theme_message(Event::Reload))
                        )
                        .small(),
                        action(
                            theme,
                            "Themes folder",
                            Kind::Plain,
                            Some(theme_message(Event::OpenThemes))
                        )
                        .small(),
                        action(
                            theme,
                            "New theme",
                            Kind::Plain,
                            Some(theme_message(Event::New))
                        )
                        .small()
                        .icon(self.cx.look.icon(Icon::Plus, 16, crate::theme::rgb(theme.text))),
                        action(
                            theme,
                            "New CSS theme",
                            Kind::Plain,
                            Some(theme_message(Event::NewCss))
                        )
                        .small()
                        .icon(self.cx.look.icon(Icon::Plus, 16, crate::theme::rgb(theme.text))),
                    ]
                    .spacing(8),
                ),
                form::note(
                    theme,
                    css_note(self.cx.themes),
                ),
                row![
                    container(
                        form::field(
                            theme,
                            "theme-url",
                            "Theme URL",
                            &self.s.theme_url,
                        )
                        .on_input(|value| theme_message(Event::ThemeUrl(value)))
                        .on_submit(theme_message(Event::ThemeImport)),
                    )
                    .width(Length::Fill),
                    action(
                        theme,
                        "Import",
                        Kind::Plain,
                        Some(theme_message(Event::ThemeImport)),
                    )
                    .small(),
                    action(
                        theme,
                        "Import file",
                        Kind::Plain,
                        Some(theme_message(Event::ThemeImportFile)),
                    )
                    .small(),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
                self.s.theme_url_note.as_deref().map_or_else(
                    || Space::new().into(),
                    |note| form::note(theme, note),
                ),
                tiles,
            ]),
            form::group([
                form::heading(theme, "RECORE ACCENT"),
                form::note(
                    theme,
                    "ReCore and ReCore Paper derive every accent shade from this colour. Other themes keep their own.",
                ),
                {
                    let accent_colour = crate::themes::color(
                        self.cx.themes.accent.unwrap_or(palette::RECORE_DARK.accent),
                    );
                    let accent_hsv = rgb_hsv(crate::theme::rgb(accent_colour));
                    form::colour_row_with_picker(
                        theme,
                        "Accent",
                        accent_colour,
                        accent_text,
                        accent_wrong,
                        self.s.color_open.as_deref() == Some("accent"),
                        theme_message(Event::PickColour("accent".into(), accent_hsv)),
                        move |text| theme_message(Event::Accent(text)),
                        move |hsv| theme_message(Event::Hsv(hsv)),
                    )
                },
                row![
                    action(theme, "Amber", Kind::Plain, Some(theme_message(Event::Preset("amber")))).small(),
                    action(theme, "Phosphor", Kind::Plain, Some(theme_message(Event::Preset("phosphor")))).small(),
                    action(theme, "Ice", Kind::Plain, Some(theme_message(Event::Preset("ice")))).small(),
                    action(theme, "Paper", Kind::Plain, Some(theme_message(Event::Preset("paper")))).small(),
                    action(theme, "Terminal", Kind::Plain, Some(theme_message(Event::Preset("terminal")))).small(),
                    action(theme, "Reset", Kind::Plain, Some(theme_message(Event::ResetAccent))).small(),
                ]
                .spacing(8)
                .into(),
                switch(
                    "CRT layer",
                    self.cx.themes.effects,
                    Some(theme_message(Event::Effects(!self.cx.themes.effects))),
                )
                .description(
                    "Scanlines, film grain, the vignette and the corner brackets over the window.",
                )
                .view(self.cx.look),
                switch(
                    "Low power",
                    self.cx.themes.low_power,
                    Some(theme_message(Event::LowPower(!self.cx.themes.low_power))),
                )
                .description("Drops the grain and the constant repaints; the scanlines and vignette stay.")
                .view(self.cx.look),
            ]),
            form::group([
                form::heading(theme, "CRT DETAILS"),
                form::note(
                    theme,
                    "ReCord's own knobs. They apply to ReCore and ReCore Paper; a CSS theme keeps the values its file names.",
                ),
                switch(
                    "Beam",
                    self.cx.themes.recore.beam,
                    Some(theme_message(Event::Beam(!self.cx.themes.recore.beam))),
                )
                .description("The slow band travelling down the screen.")
                .view(self.cx.look),
                switch(
                    "Flicker",
                    self.cx.themes.recore.flicker,
                    Some(theme_message(Event::Flicker(!self.cx.themes.recore.flicker))),
                )
                .description("A quiet, irregular brightness change.")
                .view(self.cx.look),
                switch(
                    "Corner frame",
                    self.cx.themes.recore.frame > 0.0,
                    Some(theme_message(Event::Frame(self.cx.themes.recore.frame <= 0.0))),
                )
                .description("The four corner brackets around the window.")
                .view(self.cx.look),
                switch(
                    "Avatar scanlines",
                    self.cx.themes.recore.avatar_filter,
                    Some(theme_message(Event::AvatarFilter(
                        !self.cx.themes.recore.avatar_filter,
                    ))),
                )
                .description("ReCord's warm grade and fine lines over every picture.")
                .view(self.cx.look),
                slider(
                    "Scanlines",
                    (self.cx.themes.recore.scanlines * 100.0).round() as u32,
                    100,
                    |value| theme_message(Event::Scanlines(value as u8)),
                )
                .step(1)
                .view(self.cx.look.theme),
                slider(
                    "Film grain",
                    (self.cx.themes.recore.grain * 100.0).round() as u32,
                    100,
                    |value| theme_message(Event::Grain(value as u8)),
                )
                .step(1)
                .view(self.cx.look.theme),
                slider(
                    "Vignette",
                    (self.cx.themes.recore.vignette * 100.0).round() as u32,
                    100,
                    |value| theme_message(Event::Vignette(value as u8)),
                )
                .step(1)
                .view(self.cx.look.theme),
            ]),
            form::group([
                form::heading(theme, "INTERFACE SIZE"),
                form::dropdown(
                    ZOOM_SIZES
                        .iter()
                        .map(|(_, name)| (*name).to_owned())
                        .collect(),
                    Some(
                        ZOOM_SIZES
                            .iter()
                            .position(|(level, _)| (level - self.cx.zoom).abs() < 0.005)
                            .unwrap_or(4),
                    ),
                    self.s.open == Some(Drop::Zoom),
                    Message::Settings(SettingsEvent::Drop(Drop::Zoom)),
                    |index| Message::Settings(SettingsEvent::Pref(Pref::Zoom(index))),
                )
                .view(self.cx.look),
                form::note(
                    theme,
                    "Text, spacing and pictures all grow together, as Ctrl with + and - or the wheel does.",
                ),
            ]),
            form::group([
                form::heading(theme, "WINDOW"),
                form::note(
                    theme,
                    "The picture behind the window, its dim and the panel opacity over it, and the type every theme can use. CSS themes bring their own colours and effects.",
                ),
                row![
                    action(
                        theme,
                        "Choose background",
                        Kind::Plain,
                        Some(theme_message(Event::Background)),
                    )
                    .small(),
                    action(
                        theme,
                        "Clear",
                        Kind::Plain,
                        Some(theme_message(Event::ClearBackground)),
                    )
                    .small(),
                ]
                .spacing(8)
                .into(),
                form::note(
                    theme,
                    self.cx
                        .themes
                        .recore
                        .background
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "No background picture.".into()),
                ),
                slider(
                    "Background dim",
                    (self.cx.themes.recore.background_dim * 100.0).round() as u32,
                    100,
                    |value| theme_message(Event::BackgroundDim(value as u8)),
                )
                .step(1)
                .view(self.cx.look.theme),
                slider(
                    "Panel opacity",
                    (self.cx.themes.recore.panel_opacity * 100.0).round() as u32,
                    100,
                    |value| theme_message(Event::PanelOpacity(value as u8)),
                )
                .step(1)
                .view(self.cx.look.theme),
                form::heading(theme, "BACKGROUND GRADE"),
                row![
                    action(theme, "None", Kind::Plain, Some(theme_message(Event::BackgroundFilter(crate::background::Filter::None)))).small(),
                    action(theme, "Warm", Kind::Plain, Some(theme_message(Event::BackgroundFilter(crate::background::Filter::Warm)))).small(),
                    action(theme, "Cool", Kind::Plain, Some(theme_message(Event::BackgroundFilter(crate::background::Filter::Cool)))).small(),
                    action(theme, "Mono", Kind::Plain, Some(theme_message(Event::BackgroundFilter(crate::background::Filter::Mono)))).small(),
                    action(theme, "Sepia", Kind::Plain, Some(theme_message(Event::BackgroundFilter(crate::background::Filter::Sepia)))).small(),
                    action(theme, "Scanlines", Kind::Plain, Some(theme_message(Event::BackgroundFilter(crate::background::Filter::Scanlines)))).small(),
                ]
                .spacing(8)
                .into(),
                slider(
                    "Background brightness",
                    (self.cx.themes.recore.background_brightness * 100.0).round() as u32,
                    200,
                    |value| theme_message(Event::BackgroundBrightness(value as u8)),
                )
                .step(5)
                .view(self.cx.look.theme),
                switch(
                    "Animate background",
                    self.cx.themes.recore.background_animate,
                    Some(theme_message(Event::BackgroundAnimate(
                        !self.cx.themes.recore.background_animate,
                    ))),
                )
                .description("Pause GIFs and videos when the window is not focused, or keep them playing.")
                .view(self.cx.look),
                form::heading(theme, "TYPE"),
                row![
                    action(theme, "Pixel", Kind::Plain, Some(theme_message(Event::Font("pixel")))).small(),
                    action(theme, "Pixel mono", Kind::Plain, Some(theme_message(Event::Font("mono")))).small(),
                    action(theme, "System", Kind::Plain, Some(theme_message(Event::Font("system")))).small(),
                    action(theme, "Default", Kind::Plain, Some(theme_message(Event::Font("discord")))).small(),
                ]
                .spacing(8)
                .into(),
                form::heading(theme, "MOTION"),
                row![
                    action(theme, "Full", Kind::Plain, Some(theme_message(Event::Motion(crate::themes::Motion::Full)))).small(),
                    action(theme, "Subtle", Kind::Plain, Some(theme_message(Event::Motion(crate::themes::Motion::Subtle)))).small(),
                    action(theme, "Off", Kind::Plain, Some(theme_message(Event::Motion(crate::themes::Motion::Off)))).small(),
                ]
                .spacing(8)
                .into(),
                form::note(
                    theme,
                    match self.cx.themes.motion() {
                        crate::themes::Motion::Full => "Everything: the boot signal, the stepped popups, the pulsing badges and the live presence blink.",
                        crate::themes::Motion::Subtle => "Menus and dialogs only; the boot signal stays, the loops hold still.",
                        crate::themes::Motion::Off => "No motion loops; the boot signal still finishes.",
                    },
                ),
            ]),
            self.css_options_view(),
            self.title_bar_switch(),
            column![
                form::heading(theme, "PROFILES"),
                profile(
                    "Play profile effects",
                    "The animations some people put over their profile cards.",
                    cosmetics.effects,
                    Cosmetic::Effects,
                ),
                profile(
                    "Show avatar decorations",
                    "The pictures around people's avatars.",
                    cosmetics.decorations,
                    Cosmetic::Decorations,
                ),
                profile(
                    "Show nameplates",
                    "The pictures Discord members equip behind their rows in the member list and on your account card.",
                    prefs.nameplates.unwrap_or(true),
                    Cosmetic::Nameplates,
                ),
                profile(
                    "Animate decorations and nameplates",
                    "They play when pointed at, as in Discord. Off, they hold still.",
                    cosmetics.moving,
                    Cosmetic::Moving,
                ),
                profile(
                    "Show name styles",
                    "The fonts and colours people give their display names.",
                    cosmetics.name_styles,
                    Cosmetic::NameStyles,
                ),
                profile(
                    "Show server tags",
                    "The tag and its badge beside people's names.",
                    cosmetics.server_tags,
                    Cosmetic::ServerTags,
                ),
                profile(
                    "Show profile frames",
                    "The frames some people put around their profile cards.",
                    cosmetics.frames,
                    Cosmetic::Frames,
                ),
            ]
            .spacing(12)
            .into(),
        ])
    }

    /// The themes as tiles, as many 150 px or wider across as fit.
    fn theme_tiles(self) -> Element<'a, Message> {
        iced::widget::responsive(move |size| {
            let columns = ((size.width + TILE_GAP) / (TILE + TILE_GAP))
                .floor()
                .max(1.0) as usize;
            let width = (size.width - (columns - 1) as f32 * TILE_GAP) / columns as f32;
            let cards: Vec<(String, String, Tile, palette::Theme)> = palette::BUILT_IN
                .iter()
                .map(|built_in| {
                    (
                        built_in.id.to_owned(),
                        built_in.name.to_owned(),
                        Tile::BuiltIn,
                        built_in.theme,
                    )
                })
                .chain(self.cx.themes.custom.iter().map(|custom| {
                    (
                        custom.id.clone(),
                        custom.name.clone(),
                        Tile::Custom,
                        custom.theme,
                    )
                }))
                .chain(self.cx.themes.css.iter().map(|css| {
                    (
                        css.id.clone(),
                        css.name.clone(),
                        Tile::Css,
                        crate::themes::css_palette(css),
                    )
                }))
                .collect();
            let mut rows = column![].spacing(TILE_GAP);
            for chunk in cards.chunks(columns) {
                let mut line = row![].spacing(TILE_GAP);
                for (id, name, kind, palette) in chunk {
                    line = line.push(self.theme_tile(id, name, *kind, palette, width));
                }
                rows = rows.push(line);
            }
            rows.into()
        })
        .height(Length::Shrink)
        .into()
    }

    /// A theme to choose: a small window in its colours, its name under it,
    /// an accent ring and a tick on the one shown, and a pencil on the
    /// person's own.
    fn theme_tile(
        self,
        id: &str,
        name: &str,
        kind: Tile,
        palette: &palette::Theme,
        width: f32,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let current = self.cx.themes.chosen_id() == id;
        let preview = preview(palette, width - 16.0);
        let mut label = row![
            text(name.to_owned())
                .size(14)
                .font(if current { BOLD() } else { MEDIUM() })
                .color(if current { theme.text } else { theme.secondary })
                .wrapping(text::Wrapping::None)
                .width(Length::Fill)
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .height(24);
        if kind == Tile::Css {
            label = label.push(
                text("CSS")
                    .size(10)
                    .font(MEDIUM())
                    .color(theme.faint)
                    .wrapping(text::Wrapping::None),
            );
        }
        if current {
            label = label.push(self.cx.look.glyph(Icon::Check, 16, theme.accent));
        }
        let (accent, faint, line, hover) = (theme.accent, theme.faint, theme.line, theme.hover);
        let face = button(
            column![preview, container(label).padding([0, 2])]
                .spacing(6)
                .padding(8),
        )
        .padding(0)
        .width(width)
        .height(TILE_HEIGHT)
        .on_press(theme_message(Event::Choose(id.to_owned())))
        .style(move |_, status| {
            let lit = lights(status);
            button::Style {
                background: lit.then(|| hover.into()),
                border: Border {
                    color: if current {
                        accent
                    } else if lit {
                        faint
                    } else {
                        line
                    },
                    width: if current { 2.0 } else { 1.0 },
                    radius: 10.0.into(),
                },
                ..Default::default()
            }
        });
        let edit = if kind == Tile::Css {
            Event::EditCss(id.to_owned())
        } else {
            Event::Edit(id.to_owned())
        };
        let pencil = self
            .cx
            .look
            .icon_button(Icon::Pencil, "Edit", theme_message(edit))
            .size(24.0, 14);
        stack![face, placed(pencil, width - 8.0 - 24.0, 8.0 + 78.0 + 6.0)].into()
    }

    /// The custom properties a CSS theme exposes, grouped, with a control
    /// for each. ReCore themes have their own accent and effects above.
    fn css_options_view(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let Some(css) = self.cx.themes.chosen_css() else {
            return Space::new().into();
        };
        let show_all = self.s.show_all_css;
        let recommended_only = self.s.css_recommended_only;
        let changed_only = self.s.css_changed_only;
        let changed_names: BTreeSet<String> = self
            .cx
            .themes
            .overrides
            .get(self.cx.themes.chosen_id())
            .map(|changed| changed.keys().cloned().collect())
            .unwrap_or_default();
        let query = self.s.css_search.trim().to_ascii_lowercase();
        let matching: Vec<&ThemeOption> = css
            .options
            .iter()
            .filter(|option| {
                (show_all || option.group != "Other")
                    && (!recommended_only || option.recommended)
                    && (!changed_only || changed_names.contains(&option.name))
                    && (query.is_empty()
                        || option.label.to_ascii_lowercase().contains(&query)
                        || option.name.to_ascii_lowercase().contains(&query)
                        || option.description.as_deref().is_some_and(|description| {
                            description.to_ascii_lowercase().contains(&query)
                        }))
            })
            .collect();
        let shown = matching.len();
        let mut groups: Vec<(&str, Vec<&ThemeOption>, bool)> = Vec::new();
        for option in matching {
            let changed = changed_names.contains(&option.name);
            match groups.last_mut() {
                Some((group, options, group_changed)) if *group == option.group => {
                    options.push(option);
                    *group_changed |= changed;
                }
                _ => groups.push((option.group.as_str(), vec![option], changed)),
            }
        }
        let mut parts = vec![form::group([
            form::heading(theme, "THEME OPTIONS"),
            form::note(
                theme,
                if query.is_empty() && !recommended_only && !changed_only && !show_all {
                    format!(
                        "{} custom propert{} this theme exposes, labelled from its names and comments. Open a group to edit it; changes are kept for this theme.",
                        css.options.len(),
                        if css.options.len() == 1 { "y" } else { "ies" }
                    )
                } else {
                    format!(
                        "{} of {} custom propert{} visible. Open a group to edit it; changes are kept for this theme.",
                        shown,
                        css.options.len(),
                        if css.options.len() == 1 { "y" } else { "ies" }
                    )
                },
            ),
            container(
                form::field(
                    theme,
                    "css-options-search",
                    "Filter options",
                    &self.s.css_search,
                )
                .on_input(|value| theme_message(Event::CssSearch(value))),
            )
            .width(Length::Fill)
            .into(),
            row![
                action(
                    theme,
                    "Open all",
                    Kind::Plain,
                    Some(theme_message(Event::CssGroups(true))),
                )
                .small(),
                action(
                    theme,
                    "Close all",
                    Kind::Plain,
                    Some(theme_message(Event::CssGroups(false))),
                )
                .small(),
            ]
            .spacing(8)
            .into(),
            switch(
                "Show every variable",
                show_all,
                Some(theme_message(Event::CssAll(!show_all))),
            )
            .description("Also lists the properties ReCore can't tell a use for, for a theme you're building.")
            .view(self.cx.look),
            switch(
                "Only recommended",
                recommended_only,
                Some(theme_message(Event::CssRecommended(!recommended_only))),
            )
            .description("Hide unannotated variables and internal aliases.")
            .view(self.cx.look),
            switch(
                "Only changed",
                changed_only,
                Some(theme_message(Event::CssChangedOnly(!changed_only))),
            )
            .description("Show only the options changed from this theme's own values.")
            .view(self.cx.look),
            if !changed_names.is_empty() {
                row![
                    action(
                        theme,
                        "Export changes",
                        Kind::Plain,
                        Some(theme_message(Event::CssExport)),
                    )
                    .small(),
                    action(
                        theme,
                        "Reset these options",
                        Kind::Plain,
                        Some(theme_message(Event::CssReset)),
                    )
                    .small(),
                ]
                .spacing(8)
                .into()
            } else {
                Space::new().into()
            },
        ])];
        if groups.is_empty() {
            parts.push(form::note(
                theme,
                if query.is_empty() {
                    "This theme exposes no custom properties Appearance can edit."
                } else {
                    "No option matches the filter. Clear it to see them all."
                },
            ));
        }
        let open_all = !query.is_empty();
        for (group, options, group_changed) in groups {
            let open = open_all || self.s.css_open.contains(group);
            let heading = format!("{} ({})", group.to_uppercase(), options.len());
            let toggle = action(
                theme,
                if open { "Hide" } else { "Show" },
                Kind::Plain,
                Some(theme_message(Event::CssGroup(group.to_owned()))),
            )
            .small();
            let mut header = row![form::heading(theme, heading), toggle]
                .spacing(8)
                .align_y(Alignment::Center);
            if group_changed {
                header = header.push(
                    action(
                        theme,
                        "Reset group",
                        Kind::Plain,
                        Some(theme_message(Event::CssResetGroup(group.to_owned()))),
                    )
                    .small(),
                );
            }
            if open {
                let mut rows = column![].spacing(12);
                for option in options {
                    rows = rows.push(self.css_control(option));
                }
                parts.push(form::group([header.into(), rows.into()]));
            } else {
                parts.push(form::group([header.into()]));
            }
        }
        column(parts).spacing(24).into()
    }

    /// One control for one CSS option.
    fn css_control(self, option: &'a ThemeOption) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let draft = self.s.css_text.get(&option.name).map(String::as_str);
        let shown = draft.unwrap_or_else(|| css_value_text(option));
        let description = option.description.as_deref();
        let changed = self
            .cx
            .themes
            .overrides
            .get(self.cx.themes.chosen_id())
            .is_some_and(|changed| changed.contains_key(&option.name));
        let label = option.label.as_str();
        let name = option.name.clone();
        let control: Element<'a, Message> = match &option.kind {
            OptionKind::Color { color } => {
                let colour =
                    Color::from_rgba8(color.r, color.g, color.b, f32::from(color.a) / 255.0);
                let id = format!("css:{name}");
                let hsv = rgb_hsv(crate::theme::rgb(colour));
                let field_name = name.clone();
                form::colour_row_with_picker(
                    theme,
                    label,
                    colour,
                    shown,
                    recore_css::color::parse(shown, !self.cx.look.theme.dark).is_none(),
                    self.s.color_open.as_deref() == Some(id.as_str()),
                    theme_message(Event::PickColour(id, hsv)),
                    move |value| theme_message(Event::CssDraft(field_name.clone(), value)),
                    move |hsv| theme_message(Event::Hsv(hsv)),
                )
            }
            OptionKind::Percent { value, percent } => slider(
                label,
                css_percent(shown)
                    .unwrap_or(*value)
                    .round()
                    .clamp(0.0, 100.0) as u32,
                100,
                {
                    let percent = *percent;
                    let name = name.clone();
                    move |value| {
                        theme_message(Event::CssDraft(
                            name.clone(),
                            css_percent_text(value, percent),
                        ))
                    }
                },
            )
            .on_release(theme_message(Event::CssApply(
                name.clone(),
                shown.to_owned(),
            )))
            .view(self.cx.look.theme),
            OptionKind::Range {
                value,
                min,
                max,
                step,
                unit,
            } => {
                let max = *max;
                let min = *min;
                let unit = unit.clone();
                let shown_value = shown
                    .strip_suffix(unit.as_str())
                    .and_then(|number| number.trim().parse::<i32>().ok())
                    .unwrap_or(*value);
                let slider_unit = unit.clone();
                let slider_name = name.clone();
                let apply_name = name.clone();
                slider(
                    label,
                    (shown_value - min).clamp(0, max - min) as u32,
                    (max - min).max(1) as u32,
                    move |offset| {
                        theme_message(Event::CssDraft(
                            slider_name.clone(),
                            format!("{}{}", i64::from(offset) + i64::from(min), slider_unit),
                        ))
                    },
                )
                .step((*step).max(1) as u32)
                .mark(false)
                .shown(shown.to_owned())
                .on_release(theme_message(Event::CssApply(apply_name, shown.to_owned())))
                .view(self.cx.look.theme)
            }
            OptionKind::Toggle {
                on,
                on_value,
                off_value,
            } => switch(
                label,
                *on,
                Some(theme_message(Event::CssApply(
                    name.clone(),
                    if *on {
                        off_value.clone()
                    } else {
                        on_value.clone()
                    },
                ))),
            )
            .description(description.unwrap_or(""))
            .view(self.cx.look),
            OptionKind::Choice { choices } => {
                let mut words = column![
                    text(label)
                        .size(14)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None)
                ]
                .spacing(6);
                if let Some(description) = description {
                    words = words.push(form::note(theme, description));
                }
                let mut buttons = column![].spacing(4);
                for chunk in choices.chunks(4) {
                    let mut line = row![].spacing(6).align_y(Alignment::Center);
                    for (choice, value) in chunk {
                        let chosen = value == &option.value;
                        line = line.push(
                            action(
                                theme,
                                choice.clone(),
                                if chosen { Kind::Primary } else { Kind::Plain },
                                Some(theme_message(Event::CssApply(name.clone(), value.clone()))),
                            )
                            .small(),
                        );
                    }
                    buttons = buttons.push(line);
                }
                words.push(buttons).into()
            }
            OptionKind::Image { embedded, .. } => {
                let value = if *embedded { "" } else { shown };
                let submit = value.to_owned();
                let choose = name.clone();
                let clear = name.clone();
                let mut parts = vec![
                    container(
                        form::field(theme, option.name.clone(), label, value)
                            .on_input({
                                let name = name.clone();
                                move |value| theme_message(Event::CssDraft(name.clone(), value))
                            })
                            .on_submit(theme_message(Event::CssApply(name.clone(), submit))),
                    )
                    .width(Length::Fill)
                    .into(),
                    action(
                        theme,
                        "Choose…",
                        Kind::Plain,
                        Some(theme_message(Event::CssImage(choose))),
                    )
                    .small()
                    .into(),
                    action(
                        theme,
                        "Clear",
                        Kind::Plain,
                        Some(theme_message(Event::CssApply(clear, String::new()))),
                    )
                    .small()
                    .into(),
                ];
                if *embedded {
                    parts.insert(
                        0,
                        form::note(theme, "The theme carries this picture inside its file."),
                    );
                }
                let mut lines =
                    column![row(parts).spacing(8).align_y(Alignment::Center)].spacing(6);
                if let Some(description) = description {
                    lines = lines.push(form::note(theme, description));
                }
                lines.into()
            }
            OptionKind::Font | OptionKind::Text | OptionKind::Number { .. } => {
                let submit = shown.to_owned();
                container(
                    form::field(theme, option.name.clone(), label, shown)
                        .on_input({
                            let name = name.clone();
                            move |value| theme_message(Event::CssDraft(name.clone(), value))
                        })
                        .on_submit(theme_message(Event::CssApply(name.clone(), submit))),
                )
                .width(Length::Fill)
                .into()
            }
        };
        let mut footer = column![].spacing(4);
        let mut has_footer = false;
        if let Some(description) = description
            && !matches!(
                option.kind,
                OptionKind::Toggle { .. } | OptionKind::Choice { .. } | OptionKind::Image { .. }
            )
        {
            footer = footer.push(form::note(theme, description));
            has_footer = true;
        }
        if let Some(default) = &option.default_value {
            footer = footer.push(form::note(theme, format!("Default: {default}")));
            has_footer = true;
        }
        if let Some(only_in) = option.only_in {
            footer = footer.push(form::note(
                theme,
                if only_in {
                    "Only applies to the dark palette."
                } else {
                    "Only applies to the light palette."
                },
            ));
            has_footer = true;
        }
        if changed {
            footer = footer.push(
                action(
                    theme,
                    "Reset this option",
                    Kind::Plain,
                    Some(theme_message(Event::CssResetOne(name.clone()))),
                )
                .small(),
            );
            has_footer = true;
        }
        if has_footer {
            column![control, footer].spacing(4).into()
        } else {
            control
        }
    }

    /// A CSS theme's own source, whole: colours, images, gradients, fonts and
    /// textures as the file writes them, with buttons that add the kinds of
    /// asset a native theme can paint.
    fn css_editor_view(self, editor: &'a CssEditor) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let name: Element<'a, Message> = if editor.editing.is_some() {
            text(editor.name.clone())
                .size(14)
                .color(theme.secondary)
                .width(Length::Fill)
                .into()
        } else {
            container(
                form::field(theme, CSS_NAME, "File name", &editor.name)
                    .on_input(|name| theme_message(Event::CssName(name)))
                    .on_submit(theme_message(Event::SaveCss)),
            )
            .width(Length::Fill)
            .into()
        };
        let folder = self.cx.themes.themes_folder().display().to_string();
        let path = editor.path.as_ref().map_or_else(
            || format!("Will be saved in {folder}."),
            |path| format!("Editing {}.", path.display()),
        );
        let mut heading = row![
            form::heading(
                theme,
                if editor.editing.is_some() {
                    "EDIT CSS THEME"
                } else {
                    "NEW CSS THEME"
                },
            ),
            container(Space::new()).width(Length::Fill),
            action(
                theme,
                "Save CSS",
                Kind::Primary,
                Some(theme_message(Event::SaveCss))
            ),
            action(
                theme,
                "Back to themes",
                Kind::Plain,
                Some(theme_message(Event::CancelCss)),
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if editor.editing.is_some() {
            heading = heading
                .push(action(
                    theme,
                    "Duplicate",
                    Kind::Plain,
                    Some(theme_message(Event::CssDuplicate)),
                ))
                .push(action(
                    theme,
                    "Open folder",
                    Kind::Plain,
                    Some(theme_message(Event::OpenThemes)),
                ));
        }
        let mut parts = vec![
            form::title(theme, "Appearance"),
            form::group([
                heading.into(),
                row![name].spacing(8).align_y(Alignment::Center).into(),
                form::note(theme, path),
            ]),
        ];
        if let Some(error) = &editor.error {
            parts.push(text(error.as_str()).size(13).color(theme.danger).into());
        }
        if let Some(note) = self.s.theme_url_note.as_deref() {
            parts.push(form::note(theme, note));
        }
        parts.push(form::group([
            form::heading(theme, "CSS SOURCE"),
            form::note(
                theme,
                "Write standard CSS. ReCore reads colours, fonts, shapes, gradients, background pictures and ReCord's --rl-* textures; save to see the theme at once.",
            ),
            form::code_area(
                theme,
                &editor.draft,
                CSS_SOURCE,
                "/* A ReCore CSS theme */",
                CSS_LINES,
                |event| theme_message(Event::CssSource(event)),
            ),
            row![
                action(
                    theme,
                    "Choose background...",
                    Kind::Plain,
                    Some(theme_message(Event::CssBackground)),
                )
                .small(),
                action(
                    theme,
                    "Scanlines",
                    Kind::Plain,
                    Some(theme_message(Event::CssTexture(Texture::Scanlines))),
                )
                .small(),
                action(
                    theme,
                    "Grain",
                    Kind::Plain,
                    Some(theme_message(Event::CssTexture(Texture::Grain))),
                )
                .small(),
                action(
                    theme,
                    "Vignette",
                    Kind::Plain,
                    Some(theme_message(Event::CssTexture(Texture::Vignette))),
                )
                .small(),
                action(
                    theme,
                    "Export...",
                    Kind::Plain,
                    Some(theme_message(Event::CssExportSource)),
                )
                .small(),
            ]
            .spacing(8)
            .into(),
            form::note(
                theme,
                "Choose background adds --rl-background-image with a dim and panel opacity; the texture buttons add --rl-scanline-opacity, --rl-grain-opacity or --rl-vignette-opacity.",
            ),
        ]));
        if editor.editing.is_some() {
            parts.push(form::buttons([action(
                theme,
                if editor.confirming_delete {
                    "Delete this file for good"
                } else {
                    "Delete CSS theme"
                },
                Kind::Danger,
                Some(theme_message(Event::CssDelete)),
            )
            .small()
            .icon(
                self.cx
                    .look
                    .icon(Icon::Trash, 16, crate::theme::rgb(theme.on_danger)),
            )
            .into()]));
        }
        form::page(parts)
    }

    /// The editor: the name with Save and Cancel, what a new theme starts
    /// from, Dark, each colour's field, and Delete for a saved one.
    fn editor_view(self, editor: &'a Editor, starts_open: bool) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut parts = vec![
            form::title(theme, "Appearance"),
            form::group([
                form::heading(
                    theme,
                    if editor.editing.is_some() {
                        "EDIT THEME"
                    } else {
                        "NEW THEME"
                    },
                ),
                row![
                    form::field(theme, NAME, "Name", &editor.name)
                        .on_input(|name| theme_message(Event::Name(name)))
                        .on_submit(theme_message(Event::Save)),
                    action(
                        theme,
                        "Save",
                        Kind::Primary,
                        editor.valid().then(|| theme_message(Event::Save))
                    ),
                    action(
                        theme,
                        "Cancel",
                        Kind::Plain,
                        Some(theme_message(Event::Cancel))
                    )
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ]),
        ];
        if editor.editing.is_none() {
            let names: Vec<String> = palette::starts(&self.cx.themes.custom)
                .map(|start| start.name.to_owned())
                .collect();
            parts.push(form::group([
                form::heading(theme, "START FROM"),
                form::dropdown(
                    names,
                    Some(editor.start),
                    starts_open,
                    Message::Settings(SettingsEvent::Drop(Drop::Start)),
                    |index| theme_message(Event::Start(index)),
                )
                .view(self.cx.look),
            ]));
        }
        parts.push(
            switch(
                "Dark",
                editor.theme.dark,
                Some(theme_message(Event::Dark(!editor.theme.dark))),
            )
            .description("Drop-down lists and scroll bars draw dark to match.")
            .view(self.cx.look),
        );
        let color_open = self.s.color_open.clone();
        let colours = iced::widget::responsive(move |size| {
            let columns = if size.width >= TWO_COLUMNS { 2 } else { 1 };
            let per = editor.fields.len().div_ceil(columns);
            let mut line = row![].spacing(COLOUR_GAP);
            for column_index in 0..columns {
                let mut cells = column![];
                for index in column_index * per..((column_index + 1) * per).min(editor.fields.len())
                {
                    let (name, hex, wrong) = &editor.fields[index];
                    let shown = editor.theme.colour(index).map_or(Color::TRANSPARENT, color);
                    let id = format!("theme:{index}");
                    let hsv = rgb_hsv(crate::theme::rgb(shown));
                    cells = cells.push(form::colour_row_with_picker(
                        theme,
                        name,
                        shown,
                        hex,
                        *wrong,
                        color_open.as_deref() == Some(id.as_str()),
                        theme_message(Event::PickColour(id, hsv)),
                        move |text| theme_message(Event::Colour(index, text)),
                        move |hsv| theme_message(Event::Hsv(hsv)),
                    ));
                }
                line = line.push(container(cells).width(Length::FillPortion(1)));
            }
            line.into()
        })
        .height(Length::Shrink);
        parts.push(form::group([
            form::heading(theme, "COLOURS"),
            form::note(
                theme,
                "Each as #rgb, #rrggbb or #rrggbbaa. The window shows a change as you type it.",
            ),
            colours.into(),
        ]));
        if editor.editing.is_some() {
            parts.push(form::buttons([action(
                theme,
                if editor.confirming {
                    "Delete it for good"
                } else {
                    "Delete theme"
                },
                Kind::Danger,
                Some(theme_message(Event::Delete)),
            )
            .small()
            .icon(
                self.cx
                    .look
                    .icon(Icon::Trash, 16, crate::theme::rgb(theme.on_danger)),
            )
            .into()]));
        }
        form::page(parts)
    }
}

/// The starter file a new CSS theme opens on: ReCore's own colours, Discord's
/// palette names, square panels and ReCord's textures, with the properties a
/// person is most likely to change called out in comments.
const CSS_TEMPLATE: &str = r#"/**
 * @name My theme
 * @author You
 * @description Made in ReCore.
 */

:root {
  /* Discord's grounds */
  --background-primary: #080907;
  --background-secondary: #0e100c;
  --background-secondary-alt: #0e100c;
  --background-tertiary: #080907;
  --background-base-lowest: #080907;
  --background-base-lower: #0e100c;
  --background-base-low: #0e100c;
  --background-base: #080907;
  --background-base-high: #131511;

  /* Ink and signals */
  --text-normal: #e4cdb1;
  --text-muted: #a58979;
  --text-link: #f28c6d;
  --interactive-normal: #a58979;
  --interactive-hover: #e4cdb1;
  --interactive-active: #e4cdb1;
  --brand-experiment: #e94221;
  --red-400: #ff4626;
  --green-360: #79b59a;
  --yellow-300: #dda052;
  --border-subtle: #2c241d;

  /* Panels and type */
  --app-frame-background: #050605;
  --gap: 0px;
  --roundness: 0px;
  --font-primary: sans-serif;

  /* With a background picture, fade each section on its own. */
  --rl-rail-opacity: 1;
  --rl-sidebar-opacity: 1;
  --rl-chat-opacity: 1;
  --rl-members-opacity: 1;
  --rl-composer-opacity: 1;

  /* Textures: scanlines, grain and the vignette */
  --rl-scanline-opacity: 0.08;
  --rl-grain-opacity: 0.03;
  --rl-vignette-opacity: 0.18;
}

body {
  background: var(--background-base);
}
"#;

/// The id of the CSS theme read from `path`, if it is still there.
fn css_id_at(themes: &crate::themes::Themes, path: &Path) -> Option<String> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    themes.css.iter().find_map(|css| {
        let css_path = css.path.as_ref()?;
        let css_path = std::fs::canonicalize(css_path).unwrap_or_else(|_| css_path.clone());
        (css_path == target).then(|| css.id.clone())
    })
}

/// `name` as a file name, safe on every platform.
fn safe_name(name: &str, fallback: &str) -> String {
    let mut base: String = name
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    while base.contains("--") {
        base = base.replace("--", "-");
    }
    let base = base.trim_matches('-');
    if base.is_empty() {
        fallback.to_owned()
    } else {
        base.to_owned()
    }
}

/// The source with its `@name` header renamed, or one added when the file
/// has no header yet.
fn renamed_meta(source: &str, name: &str) -> String {
    let mut renamed = String::with_capacity(source.len() + name.len());
    let mut done = false;
    for line in source.split_inclusive('\n') {
        if !done && let Some(at) = line.find("@name") {
            renamed.push_str(&line[..at]);
            renamed.push_str("@name ");
            renamed.push_str(name);
            if line.ends_with("\r\n") {
                renamed.push_str("\r\n");
            } else if line.ends_with('\n') {
                renamed.push('\n');
            }
            done = true;
            continue;
        }
        renamed.push_str(line);
    }
    if done {
        renamed
    } else {
        format!("/**\n * @name {name}\n */\n{source}")
    }
}

/// A path the CSS can name, with the separators CSS URLs use.
fn css_url(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], "")
}

/// A file name for a new CSS theme, safe on every platform, with a number if
/// that name is already there.
fn unique_css_path(folder: &Path, name: &str) -> PathBuf {
    let mut base: String = name
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    while base.contains("--") {
        base = base.replace("--", "-");
    }
    let base = base.trim_matches('-');
    let base = if base.is_empty() { "my-theme" } else { base };
    let mut path = folder.join(format!("{base}.theme.css"));
    let mut number = 2;
    while path.exists() {
        path = folder.join(format!("{base}-{number}.theme.css"));
        number += 1;
    }
    path
}

/// What a theme copied from another is called until it's named.
fn copy_name(name: &str) -> String {
    format!("{name} copy")
}

/// The text a CSS option's field shows.
fn css_value_text(option: &ThemeOption) -> &str {
    match &option.kind {
        OptionKind::Image { url, embedded } => {
            if *embedded {
                ""
            } else {
                url.as_deref().unwrap_or("")
            }
        }
        _ => option.value.as_str(),
    }
}

/// Keeps the alpha of the colour already in a field when the HSV picker
/// changes its hue, so `#rrggbbaa` does not become opaque.
fn preserve_alpha(hex: &str, previous: &str) -> String {
    let alpha = recore_css::color::parse(previous, true).map(|color| color.a);
    match alpha {
        Some(alpha) if alpha != 255 => format!("{hex}{alpha:02x}"),
        _ => hex.to_owned(),
    }
}

/// `25%` or `0.25` as the 0..100 a slider shows.
fn css_percent(text: &str) -> Option<f32> {
    let text = text.trim();
    if let Some(percent) = text.strip_suffix('%') {
        return percent.trim().parse::<f32>().ok();
    }
    text.parse::<f32>()
        .ok()
        .map(|value| if value > 1.0 { value } else { value * 100.0 })
}

/// A slider's 0..100 as the theme writes it: `25%` or `0.25`.
fn css_percent_text(value: u32, percent: bool) -> String {
    if percent {
        format!("{value}%")
    } else {
        format!("{:.2}", value as f32 / 100.0)
    }
}

/// The CSS value an option is set to from the value its field or slider
/// shows.
fn css_option_value(option: &ThemeOption, value: &str) -> String {
    let value = value.trim();
    match &option.kind {
        OptionKind::Image { .. } => {
            if value.is_empty() || value.eq_ignore_ascii_case("none") {
                "none".into()
            } else if value.starts_with("url(") {
                value.to_owned()
            } else {
                format!("url(\"{}\")", value.replace('\\', "/").replace('"', "\\\""))
            }
        }
        OptionKind::Number { unit, .. } if !unit.is_empty() && value.parse::<f32>().is_ok() => {
            format!("{value}{unit}")
        }
        _ => value.to_owned(),
    }
}

/// ReCord's one-click palettes: the accent and the ReCore theme they pair
/// with. `terminal` uses ReCord's custom colours in the theme; ReCore keeps
/// the accent and the dark palette here.
fn preset(name: &str) -> (&'static str, &'static str) {
    match name {
        "phosphor" => ("#3ddc97", "recore"),
        "ice" => ("#4fb8c9", "recore"),
        "paper" => ("#b8401f", "recore-light"),
        "terminal" => ("#3ddc97", "recore"),
        _ => ("#e94221", "recore"),
    }
}

/// What Appearance says of the CSS themes it read, and where to put more.
fn css_note(themes: &crate::themes::Themes) -> String {
    let folder = themes.themes_folder().display();
    let mut note = if themes.css.is_empty() {
        format!(
            "No CSS themes yet. Put a Vencord, Vesktop or Discord .theme.css file in {folder}, then Reload CSS."
        )
    } else {
        format!(
            "{} CSS theme{} read from {folder} and the theme folders of Vencord, Vesktop and their kin. A CSS theme gives its colours, accent, font, radius and background; Discord's web-only rules have no nodes in ReCore and are not painted.",
            themes.css.len(),
            if themes.css.len() == 1 { "" } else { "s" }
        )
    };
    if !themes.problems.is_empty() {
        let _ = write!(
            note,
            " {} file{} could not be read.",
            themes.problems.len(),
            if themes.problems.len() == 1 { "" } else { "s" }
        );
        for problem in themes.problems.iter().take(2) {
            let _ = write!(note, " {}: {}.", problem.path.display(), problem.message);
        }
    }
    if let Some(css) = themes.chosen_css() {
        for warning in css.warnings.iter().take(3) {
            note.push(' ');
            note.push_str(warning);
            if !warning.ends_with('.') {
                note.push('.');
            }
        }
        if css.warnings.len() > 3 {
            let _ = write!(note, " And {} more warnings.", css.warnings.len() - 3);
        }
    }
    note
}

/// A theme's small window, 78 px tall: the rail with three servers, the
/// sidebar's channels, two messages and the composer, in its colours.
fn preview<'a>(palette: &palette::Theme, width: f32) -> Element<'a, Message> {
    let c = |colour| color(colour);
    let (rail, sidebar, chat, raised, accent, ink, muted) = (
        c(palette.rail),
        c(palette.sidebar),
        c(palette.chat),
        c(palette.raised),
        c(palette.accent),
        c(palette.text),
        c(palette.muted),
    );
    let blob = |w: f32, h: f32, colour: Color, radius: f32| -> Element<'a, Message> {
        container(Space::new())
            .width(w)
            .height(h)
            .style(move |_| crate::style::rounded(colour, radius))
            .into()
    };
    let mut layers = stack![blob(width, 78.0, chat, 6.0)];
    layers = layers.push(blob(16.0, 78.0, rail, 0.0));
    for index in 0..3 {
        let (colour, radius) = if index == 0 {
            (accent, 3.0)
        } else {
            (raised, 5.0)
        };
        layers = layers.push(placed(
            blob(10.0, 10.0, colour, radius),
            3.0,
            6.0 + index as f32 * 13.0,
        ));
    }
    layers = layers.push(placed(blob(38.0, 78.0, sidebar, 0.0), 16.0, 0.0));
    for index in 0..4 {
        let w = if index == 1 {
            24.0
        } else {
            18.0 + (index % 2) as f32 * 4.0
        };
        let colour = if index == 1 { ink } else { muted };
        layers = layers.push(placed(
            blob(w, 4.0, colour, 2.0),
            22.0,
            8.0 + index as f32 * 11.0,
        ));
    }
    let room = width - 66.0;
    for index in 0..2 {
        let y = 8.0 + index as f32 * 22.0;
        let first = index == 0;
        layers = layers
            .push(placed(
                blob(10.0, 10.0, if first { accent } else { muted }, 5.0),
                60.0,
                y,
            ))
            .push(placed(
                blob(if first { 22.0 } else { 16.0 }, 4.0, ink, 2.0),
                74.0,
                y + 1.0,
            ))
            .push(placed(
                blob(
                    (room - 14.0).min(if first { 52.0 } else { 40.0 }),
                    3.0,
                    muted,
                    1.5,
                ),
                74.0,
                y + 8.0,
            ));
    }
    layers = layers
        .push(placed(blob(room, 12.0, raised, 4.0), 60.0, 78.0 - 18.0))
        .push(placed(
            blob(6.0, 6.0, accent, 3.0),
            60.0 + room - 10.0,
            78.0 - 18.0,
        ));
    container(layers.width(width).height(78))
        .clip(true)
        .style(|_| iced::widget::container::Style {
            border: crate::style::round(6),
            ..Default::default()
        })
        .into()
}

impl<'a> Page<'a> {
    /// Appearance's "Use the system title bar", where ReCore can draw its
    /// own: the desktop's way until it's chosen.
    pub(super) fn title_bar_switch(self) -> Element<'a, Message> {
        if !crate::window_bar::offered() {
            return Space::new().into();
        }
        let system = !self.cx.own_title_bar;
        switch(
            "Use the system title bar",
            system,
            Some(Message::Settings(SettingsEvent::Pref(
                super::pages::Pref::SystemTitleBar(!system),
            ))),
        )
        .description("Your desktop draws the window's title bar and buttons, as on its other windows. Off, ReCore draws its own.")
        .view(self.cx.look)
    }
}

#[cfg(test)]
mod tests {
    use super::{CssEditor, preserve_alpha, renamed_meta, safe_name, unique_css_path};

    #[test]
    fn a_duplicate_renames_its_header_and_keeps_the_rest() {
        let source = "/**\n * @name One\n * @author Someone\n */\n:root { --x: 1; }";
        let renamed = renamed_meta(source, "One copy");
        assert!(renamed.contains("@name One copy"), "{renamed}");
        assert!(renamed.contains("@author Someone"), "{renamed}");
        assert!(renamed.contains("--x: 1"), "{renamed}");
        assert_eq!(safe_name("../My theme", "theme"), "My-theme");
    }

    #[test]
    fn the_hsv_picker_keeps_an_existing_alpha() {
        assert_eq!(preserve_alpha("#ff0000", "#ff000080"), "#ff000080");
        assert_eq!(preserve_alpha("#ff0000", "#ff0000"), "#ff0000");
        assert_eq!(preserve_alpha("#ff0000", ""), "#ff0000");
    }

    #[test]
    fn a_new_css_theme_gets_a_safe_unique_file() {
        let folder = tempfile::tempdir().unwrap();
        let first = unique_css_path(folder.path(), "../My theme");
        assert_eq!(
            first.file_name().and_then(|name| name.to_str()),
            Some("My-theme.theme.css")
        );
        std::fs::write(&first, "").unwrap();
        let second = unique_css_path(folder.path(), "My theme");
        assert_eq!(
            second.file_name().and_then(|name| name.to_str()),
            Some("My-theme-2.theme.css")
        );
    }

    #[test]
    fn a_texture_is_added_after_the_source_so_it_wins() {
        let mut editor = CssEditor::new(None, "A".into(), None, ":root { --gap: 2px; }");
        editor.append(":root { --rl-grain-opacity: 0.03; }");
        let source = editor.source();
        let gap = source.find("--gap").unwrap();
        let grain = source.find("--rl-grain-opacity").unwrap();
        assert!(gap < grain, "the later declaration wins: {source}");
    }
}
