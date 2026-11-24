//! The themes the window shows: ReCore's own (ReCord's palette, dark or
//! paper), the person's custom ones in `layout.json`, and the Discord CSS
//! themes Vencord, Vesktop and their kin keep. The choice is `layout.json`'s
//! `theme`: a built-in id, a `custom:` one or a `css:` one. See
//! docs/themes.md.
//!
//! The chosen theme is applied before the window's first frame, and again to
//! a window made anew. With none kept, the window shows ReCore. An id ReCore
//! doesn't know, or a deleted theme's, shows ReCore too. The window-control
//! command `theme ID` chooses and keeps one as Appearance does; Appearance
//! itself is Settings' (`settings/appearance.rs`).

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use iced::Color;
use recore_core::theme::{self as palette, Colour, Custom};
use recore_css::{NativeStyle, ThemeFile};

use crate::{
    background::{self, Filter, Grade},
    fonts,
    layout::{self, Layout},
    theme::Theme,
};

/// The theme shown when `layout.json` names none: ReCord's terminal palette.
const DEFAULT: &str = "recore";

/// The themes this data folder offers and the one it chose.
/// ReCore's own look, the knobs ReCord's plugin exposes: its CRT levels,
/// its window picture, how opaque the panels over it are, and its type.
/// A CSS theme keeps its own values instead.
#[derive(Clone, Debug)]
pub struct RecoreOptions {
    pub scanlines: f32,
    pub grain: f32,
    pub vignette: f32,
    pub frame: f32,
    pub beam: bool,
    pub flicker: bool,
    pub avatar_filter: bool,
    pub background: Option<PathBuf>,
    pub background_dim: f32,
    pub panel_opacity: f32,
    pub font: String,
    /// Whether the person chose `font`, rather than ReCore's start.
    pub font_chosen: bool,
    pub motion: Motion,
    /// The background's filter, brightness (0.25–2) and whether a moving
    /// background plays.
    pub background_filter: Filter,
    pub background_brightness: f32,
    pub background_animate: bool,
}

/// ReCord's three motion levels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Motion {
    #[default]
    Full,
    Subtle,
    Off,
}

impl Default for RecoreOptions {
    fn default() -> Self {
        Self {
            scanlines: 0.25,
            grain: 0.18,
            // ReCord's vignette darkens the window's edges, and its top and
            // bottom most of all; ReCore starts without it.
            vignette: 0.0,
            frame: 0.6,
            beam: true,
            flicker: false,
            avatar_filter: true,
            background: None,
            background_dim: 0.7,
            panel_opacity: 0.75,
            font: "pixel".into(),
            font_chosen: false,
            motion: Motion::Full,
            background_filter: Filter::None,
            background_brightness: 1.0,
            background_animate: true,
        }
    }
}

impl RecoreOptions {
    fn read(layout: &Layout) -> Self {
        let percent = |value: Option<u8>, fallback: f32| {
            value
                .map(|value| f32::from(value.min(100)) / 100.0)
                .unwrap_or(fallback)
        };
        Self {
            scanlines: percent(layout.recore_scanlines, 0.25),
            grain: percent(layout.recore_grain, 0.18),
            vignette: percent(layout.recore_vignette, 0.0),
            frame: if layout.recore_frame.unwrap_or(true) {
                0.6
            } else {
                0.0
            },
            beam: layout.recore_beam.unwrap_or(true),
            flicker: layout.recore_flicker.unwrap_or(false),
            avatar_filter: layout.recore_avatar_filter.unwrap_or(true),
            background: layout.recore_background.as_deref().map(PathBuf::from),
            background_dim: percent(layout.recore_background_dim, 0.7),
            panel_opacity: percent(layout.recore_panel_opacity, 0.75),
            font: layout.recore_font.clone().unwrap_or_else(|| "pixel".into()),
            font_chosen: layout.recore_font.is_some(),
            motion: match layout.recore_motion.as_deref() {
                Some("subtle") => Motion::Subtle,
                Some("off") => Motion::Off,
                _ => Motion::Full,
            },
            background_filter: Filter::from_name(
                layout.recore_background_filter.as_deref().unwrap_or("none"),
            ),
            background_brightness: layout
                .recore_background_brightness
                .map(|value| f32::from(value.clamp(25, 200)) / 100.0)
                .unwrap_or(1.0),
            background_animate: layout.recore_background_animate.unwrap_or(true),
        }
    }
}

pub struct Themes {
    pub custom: Vec<Custom>,
    /// Discord CSS themes read from the folders below.
    pub css: Vec<ThemeFile>,
    /// The theme files that could not be read, for Appearance to list.
    pub problems: Vec<recore_css::Problem>,
    /// Where ReCore looked for CSS themes.
    pub folders: Vec<PathBuf>,
    /// The data folder, for keeping a choice that followed a renamed file.
    root: PathBuf,
    pub chosen: Option<String>,
    /// ReCore's accent, over its theme's own signal orange.
    pub accent: Option<Colour>,
    /// Whether the CRT layer is drawn, and whether low power takes its grain.
    pub effects: bool,
    pub low_power: bool,
    /// ReCore's own options, when ReCore or ReCore Paper is chosen.
    pub recore: RecoreOptions,
    /// The values the person changed in CSS themes, by theme id and custom
    /// property, kept in `layout.json`.
    pub overrides: BTreeMap<String, BTreeMap<String, String>>,
    /// The chosen background picture's first frame, and how far it is
    /// dimmed. A GIF or video plays from `background_picture` through the
    /// window's animation players.
    pub background: Option<iced::widget::image::Handle>,
    pub background_picture: Option<std::sync::Arc<recore_core::chat::Picture>>,
    /// The chosen CSS page's paint list, drawn once at the reference size.
    pub paint: Option<iced::widget::image::Handle>,
    pub background_dim: f32,
    /// The theme files' fingerprint when they were last read.
    fingerprint: u64,
}

impl Themes {
    /// The themes `layout` keeps, and the CSS themes in `root`'s themes
    /// folder and the clients' own.
    pub fn read(root: &Path, layout: &Layout) -> Self {
        set_remote_cache_dir(root);
        let mut custom = layout.custom_themes.clone();
        palette::tidy(&mut custom);
        let folders = theme_folders(root);
        let accent = layout.recore_accent.as_deref().and_then(palette::parse);
        let effects = layout.recore_effects.unwrap_or(true);
        let low_power = layout.recore_low_power.unwrap_or(false);
        let recore = RecoreOptions::read(layout);
        let overrides = layout.css_options.clone();
        let mut themes = Self {
            custom,
            css: Vec::new(),
            problems: Vec::new(),
            folders,
            root: root.to_path_buf(),
            chosen: layout.theme.clone(),
            accent,
            effects,
            low_power,
            recore,
            overrides,
            background: None,
            background_picture: None,
            paint: None,
            background_dim: 0.0,
            fingerprint: 0,
        };
        themes.reload_local();
        themes
    }

    /// Reads the CSS themes again, keeping the chosen one if it's still
    /// there. Reload CSS uses this and follows every theme's remote imports.
    pub fn reload(&mut self) {
        self.reload_with(true);
    }

    /// Reads the local theme files only, then follows the chosen theme's
    /// remote imports. This is what starting the window does: a folder full
    /// of remote themes no longer waits for every base to download before
    /// the first frame. Choosing a theme fills its imports in.
    pub fn reload_local(&mut self) {
        self.reload_with(false);
    }

    fn reload_with(&mut self, remote_all: bool) {
        // A theme's id comes from its name; editing that name must not drop
        // the choice or the options kept for the same file.
        let previous = self
            .chosen_css()
            .and_then(|css| css.path.as_deref())
            .map(canonical);
        let scan = if remote_all {
            let mut fetched: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
            recore_css::scan_with_imports(&self.folders, &mut |url| {
                fetched
                    .entry(url.to_owned())
                    .or_insert_with(|| fetch_remote(url))
                    .clone()
            })
        } else {
            recore_css::scan(&self.folders)
        };
        self.css = scan.themes;
        self.problems = scan.problems;
        self.fingerprint = recore_css::theme_files_fingerprint(&self.folders);
        if let Some(previous) = previous
            && let Some(new_id) = self
                .css
                .iter()
                .find(|css| {
                    css.path
                        .as_deref()
                        .is_some_and(|path| canonical(path) == previous)
                })
                .map(|css| css.id.clone())
            && self.chosen.as_deref() != Some(new_id.as_str())
        {
            let old_id = self.chosen.replace(new_id.clone());
            layout::store(&self.root, "theme", &new_id);
            if let Some(old_id) = old_id
                && old_id != new_id
                && let Some(changed) = self.overrides.remove(&old_id)
            {
                self.overrides.insert(new_id, changed);
                layout::store(&self.root, "css_options", &self.overrides);
            }
        }
        self.apply_overrides();
        if !remote_all {
            let chosen = self.chosen_id().to_owned();
            if let Some(css) = self.chosen_css() {
                // A local theme without changed options is complete from the
                // scan; only one naming remote files, which the local scan
                // leaves for the chosen theme to fetch, is read again.
                let overridden = self
                    .overrides
                    .get(&chosen)
                    .is_some_and(|changed| !changed.is_empty());
                if css.remote && !overridden {
                    self.reload_one(&chosen);
                    return;
                }
            }
            self.load_paint();
            self.apply_font(&chosen);
            self.load_background();
            return;
        }
        let chosen = self.chosen_id().to_owned();
        self.load_paint();
        self.apply_font(&chosen);
        self.load_background();
    }

    /// Whether a theme file was added, removed or saved since the last read.
    pub fn files_changed(&self) -> bool {
        recore_css::theme_files_fingerprint(&self.folders) != self.fingerprint
    }

    /// Re-reads each CSS theme that has options the person changed, so its
    /// palette and effects show them.
    fn apply_overrides(&mut self) {
        if self.overrides.is_empty() {
            return;
        }
        for theme in &mut self.css {
            let Some(changed) = self.overrides.get(&theme.id) else {
                continue;
            };
            if changed.is_empty() {
                continue;
            }
            let Some(path) = theme.path.clone() else {
                continue;
            };
            let options = recore_css::Options {
                overrides: changed
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect(),
                ..recore_css::Options::default()
            };
            if let Ok(reloaded) =
                recore_css::load_with_imports(path, &options, &mut |url| fetch_remote(url))
            {
                let id = theme.id.clone();
                *theme = reloaded;
                theme.id = id;
            }
        }
    }

    /// Whether avatars are square and framed, as ReCord draws them: ReCore's
    /// own themes, and a CSS theme whose radius is square.
    pub fn square_avatars(&self) -> bool {
        if matches!(self.chosen_id(), "recore" | "recore-light") {
            return self.recore.avatar_filter;
        }
        self.chosen_css()
            .is_some_and(|css| css.look.radius.unwrap_or(8.0) < 2.0)
    }

    /// Whether avatars get ReCord's warm grade and scanlines: ReCore's own
    /// themes, and a CSS theme that names an avatar filter or scanline.
    pub fn avatar_filter(&self) -> bool {
        if self.low_power {
            return false;
        }
        if matches!(self.chosen_id(), "recore" | "recore-light") {
            return true;
        }
        self.chosen_css().is_some_and(|css| {
            css.look.effects.avatar_scanline > 0.0
                || css
                    .look
                    .effects
                    .avatar_filter
                    .as_deref()
                    .is_some_and(|filter| {
                        let filter = filter.trim();
                        !filter.is_empty() && !filter.eq_ignore_ascii_case("none")
                    })
        })
    }

    /// The chosen theme if it is a CSS one, for Appearance's options.
    pub fn chosen_css(&self) -> Option<&ThemeFile> {
        self.css.iter().find(|css| css.id == self.chosen_id())
    }

    /// Keeps `value` for `name` in the CSS theme `id`, and shows it.
    pub fn set_option(&mut self, root: &Path, id: &str, name: &str, value: String) {
        let changed = self.overrides.entry(id.to_owned()).or_default();
        changed.insert(name.to_owned(), value);
        layout::store(root, "css_options", &self.overrides);
        self.reload_one(id);
    }

    /// Forgets one changed option, restoring the theme's own value.
    pub fn clear_option(&mut self, root: &Path, id: &str, name: &str) {
        let mut names = std::collections::BTreeSet::new();
        names.insert(name.to_owned());
        self.clear_named(root, id, &names);
    }

    /// Forgets the changed options in `names`, restoring the theme's values.
    pub fn clear_named(
        &mut self,
        root: &Path,
        id: &str,
        names: &std::collections::BTreeSet<String>,
    ) {
        if let Some(changed) = self.overrides.get_mut(id) {
            changed.retain(|name, _| !names.contains(name));
            if changed.is_empty() {
                self.overrides.remove(id);
            }
            layout::store(root, "css_options", &self.overrides);
            self.reload_one(id);
        }
    }

    /// Forgets every changed option of the CSS theme `id`.
    pub fn clear_options(&mut self, root: &Path, id: &str) {
        self.overrides.remove(id);
        layout::store(root, "css_options", &self.overrides);
        self.reload_one(id);
        let chosen = self.chosen_id().to_owned();
        self.apply_font(&chosen);
    }

    /// Deletes a CSS theme's file, forgets the options kept for it, and
    /// shows ReCore when it was the chosen one.
    pub fn delete_css(&mut self, root: &Path, id: &str) -> Result<(), String> {
        let index = self
            .css
            .iter()
            .position(|css| css.id == id)
            .ok_or_else(|| "the theme is no longer there".to_owned())?;
        let path = self.css[index]
            .path
            .clone()
            .ok_or_else(|| "the theme has no file to delete".to_owned())?;
        fs::remove_file(&path)
            .map_err(|error| format!("could not delete {}: {error}", path.display()))?;
        self.overrides.remove(id);
        layout::store(root, "css_options", &self.overrides);
        if self.chosen.as_deref() == Some(id) || self.chosen_id() == id {
            self.chosen = Some(DEFAULT.to_owned());
            layout::store(root, "theme", DEFAULT);
        }
        self.reload_local();
        Ok(())
    }

    /// Re-reads one CSS theme with its changed options.
    fn reload_one(&mut self, id: &str) {
        let Some(index) = self.css.iter().position(|css| css.id == id) else {
            return;
        };
        let Some(path) = self.css[index].path.clone() else {
            return;
        };
        let paint = self.chosen_id() == id && self.css[index].look.background.is_none();
        let options = recore_css::Options {
            overrides: self
                .overrides
                .get(id)
                .map(|changed| {
                    changed
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            paint,
            ..recore_css::Options::default()
        };
        if let Ok(reloaded) =
            recore_css::load_with_imports(path, &options, &mut |url| fetch_remote(url))
        {
            let id = self.css[index].id.clone();
            self.css[index] = reloaded;
            self.css[index].id = id;
        }
        let chosen = self.chosen_id().to_owned();
        self.apply_font(&chosen);
        self.load_background();
    }

    /// The theme to show.
    pub fn shown(&self) -> Theme {
        self.find(self.chosen_id()).unwrap_or_else(|| {
            let built_in = palette::find(DEFAULT).expect("ReCore is a built-in theme");
            theme(built_in.id, &built_in.theme)
        })
    }

    /// The chosen theme's id, as Appearance marks it.
    pub fn chosen_id(&self) -> &str {
        if let Some(id) = self.chosen.as_deref()
            && self.find(id).is_some()
        {
            return id;
        }
        DEFAULT
    }

    /// Lowers the panels over ReCore's own background picture, as ReCord's
    /// surface opacity does. Text and lines keep their full strength.
    fn paint_recore(&self, shown: &mut Theme) {
        if self.background.is_none() || self.recore.panel_opacity >= 1.0 {
            return;
        }
        let opacity = self.recore.panel_opacity.clamp(0.0, 1.0);
        for colour in [
            &mut shown.rail,
            &mut shown.sidebar,
            &mut shown.panel,
            &mut shown.chat,
            &mut shown.stage,
            &mut shown.raised,
            &mut shown.field,
            &mut shown.sunken,
            &mut shown.hover,
            &mut shown.active,
            &mut shown.row_hover,
            &mut shown.button_default,
            &mut shown.button_default_hover,
        ] {
            colour.a *= opacity;
        }
    }

    /// A CSS theme's surface opacity, when it keeps a background picture
    /// behind the panels. Both Ambxst and ReCord name this in their files.
    fn paint_css(&self, css: &ThemeFile, shown: &mut Theme) {
        if css.look.background.is_none() && css.paint.is_none() {
            return;
        }
        let opacity = css.look.panel_opacity.unwrap_or(1.0).clamp(0.0, 1.0);
        if opacity >= 1.0 {
            return;
        }
        for colour in [
            &mut shown.rail,
            &mut shown.sidebar,
            &mut shown.panel,
            &mut shown.chat,
            &mut shown.stage,
            &mut shown.raised,
            &mut shown.field,
            &mut shown.sunken,
            &mut shown.hover,
            &mut shown.active,
            &mut shown.row_hover,
            &mut shown.button_default,
            &mut shown.button_default_hover,
        ] {
            colour.a *= opacity;
        }
    }

    /// `id`'s theme, if it's one ReCore has, the person made, or a CSS theme
    /// in one of the folders.
    pub fn find(&self, id: &str) -> Option<Theme> {
        if let Some(css) = self.css.iter().find(|css| css.id == id) {
            let mut shown = from_css(css);
            self.paint_css(css, &mut shown);
            let mut shown = shown.worked_out();
            apply_kit_native_after(&mut shown, &css.native);
            return Some(shown);
        }
        palette::lookup(id, &self.custom).map(|(id, found)| {
            let found = match (id, self.accent) {
                ("recore" | "recore-light", Some(accent)) => found.accented(accent),
                _ => found,
            };
            let mut shown = theme(id, &found);
            shown.square = matches!(id, "recore" | "recore-light");
            self.paint_recore(&mut shown);
            shown.worked_out()
        })
    }

    /// Keeps `accent` as ReCore's own accent, or clears it with `None`.
    pub fn set_accent(&mut self, root: &Path, accent: Option<Colour>) {
        self.accent = accent;
        layout::store(root, "recore_accent", accent.map(palette::hex));
    }

    /// Turns the CRT layer on or off, and keeps it.
    pub fn set_effects(&mut self, root: &Path, on: bool) {
        self.effects = on;
        layout::store(root, "recore_effects", on);
    }

    /// Turns low power on or off, and keeps it.
    pub fn set_low_power(&mut self, root: &Path, on: bool) {
        self.low_power = on;
        layout::store(root, "recore_low_power", on);
    }

    /// ReCore's CRT knobs, kept in `layout.json`.
    pub fn set_scanlines(&mut self, root: &Path, percent: u8) {
        self.recore.scanlines = f32::from(percent.min(100)) / 100.0;
        layout::store(root, "recore_scanlines", percent.min(100));
    }

    pub fn set_grain(&mut self, root: &Path, percent: u8) {
        self.recore.grain = f32::from(percent.min(100)) / 100.0;
        layout::store(root, "recore_grain", percent.min(100));
    }

    pub fn set_vignette(&mut self, root: &Path, percent: u8) {
        self.recore.vignette = f32::from(percent.min(100)) / 100.0;
        layout::store(root, "recore_vignette", percent.min(100));
    }

    pub fn set_beam(&mut self, root: &Path, on: bool) {
        self.recore.beam = on;
        layout::store(root, "recore_beam", on);
    }

    pub fn set_flicker(&mut self, root: &Path, on: bool) {
        self.recore.flicker = on;
        layout::store(root, "recore_flicker", on);
    }

    pub fn set_frame(&mut self, root: &Path, on: bool) {
        self.recore.frame = if on { 0.6 } else { 0.0 };
        layout::store(root, "recore_frame", on);
    }

    pub fn set_avatar_filter(&mut self, root: &Path, on: bool) {
        self.recore.avatar_filter = on;
        layout::store(root, "recore_avatar_filter", on);
    }

    pub fn set_background(&mut self, root: &Path, path: Option<PathBuf>) {
        let kept = path.map(|path| path.display().to_string());
        self.recore.background = kept.as_deref().map(PathBuf::from);
        layout::store(root, "recore_background", kept);
        self.load_background();
    }

    pub fn set_background_dim(&mut self, root: &Path, percent: u8) {
        self.recore.background_dim = f32::from(percent.min(100)) / 100.0;
        layout::store(root, "recore_background_dim", percent.min(100));
        self.refresh_background_handle();
    }

    pub fn set_panel_opacity(&mut self, root: &Path, percent: u8) {
        self.recore.panel_opacity = f32::from(percent.min(100)) / 100.0;
        layout::store(root, "recore_panel_opacity", percent.min(100));
    }

    pub fn set_font(&mut self, root: &Path, name: &str) {
        self.recore.font = name.to_owned();
        self.recore.font_chosen = true;
        layout::store(root, "recore_font", name.to_owned());
        let chosen = self.chosen_id().to_owned();
        self.apply_font(&chosen);
    }

    pub fn set_motion(&mut self, root: &Path, motion: Motion) {
        self.recore.motion = motion;
        let name = match motion {
            Motion::Full => "full",
            Motion::Subtle => "subtle",
            Motion::Off => "off",
        };
        layout::store(root, "recore_motion", name);
    }

    /// The chosen theme's motion level.
    pub fn motion(&self) -> Motion {
        if matches!(self.chosen_id(), "recore" | "recore-light") {
            return self.recore.motion;
        }
        match self.chosen_css().map(|css| css.look.motion) {
            Some(recore_css::Motion::Off) => Motion::Off,
            Some(recore_css::Motion::Subtle) => Motion::Subtle,
            _ => Motion::Full,
        }
    }

    /// How long the chosen CSS theme's popups take and how many steps they
    /// move in, when a rule on a menu, popout or dialog names one. ReCore's
    /// own motion is 190 ms in four steps.
    pub fn motion_pace(&self) -> Option<(Duration, u32)> {
        let css = self.chosen_css()?;
        let native = &css.native;
        if native.motion_ms.is_none() && native.motion_steps.is_none() {
            return None;
        }
        Some((
            Duration::from_millis(u64::from(native.motion_ms.unwrap_or(190))),
            native.motion_steps.unwrap_or(4),
        ))
    }

    /// The CRT layer the window draws over itself: ReCord's effect levels for
    /// a ReCore theme, the CSS theme's own `--rl-*` levels otherwise. `None`
    /// when the layer is off or the theme asks for no effects.
    pub fn shade(&self, theme: &Theme) -> Option<recore_iced_kit::effects::Shade> {
        if !self.effects {
            return None;
        }
        // A CSS theme brings its own levels; every other theme takes the
        // window's, so each has the same controls.
        let effects = if let Some(css) = self.css.iter().find(|css| css.id == self.chosen_id()) {
            css.look.effects.clone()
        } else {
            recore_css::Effects {
                scanlines: self.recore.scanlines,
                grain: self.recore.grain,
                vignette: self.recore.vignette,
                flicker: self.recore.flicker,
                beam: self.recore.beam,
                frame: self.recore.frame,
                avatar_filter: None,
                avatar_scanline: if self.recore.avatar_filter { 0.14 } else { 0.0 },
            }
        };
        let grain = if self.low_power { 0.0 } else { effects.grain };
        let moving = self.motion() != Motion::Off;
        let shade = recore_iced_kit::effects::Shade {
            scanlines: effects.scanlines,
            grain,
            vignette: effects.vignette,
            frame: effects.frame,
            beam: if self.low_power || !effects.beam || !moving {
                0.0
            } else {
                0.5
            },
            flicker: if self.low_power || !effects.flicker || !moving {
                0.0
            } else {
                1.0
            },
            line: theme.line,
            accent: theme.accent,
        };
        shade.visible().then_some(shade)
    }

    /// Keeps `id` as the chosen theme in `layout.json`, leaving the rest of
    /// what's kept there as it is, and gives the window its font.
    pub fn choose(&mut self, root: &Path, id: &str) {
        self.chosen = Some(id.to_owned());
        layout::store(root, "theme", id);
        if self.css.iter().any(|css| css.id == id) {
            self.reload_one(id);
        } else {
            self.load_paint();
            self.apply_font(id);
            self.load_background();
        }
    }

    /// Reads the chosen theme's page paint list if the local scan left it
    /// out. The scan never paints, so a folder full of themes stays cheap.
    fn load_paint(&mut self) {
        let id = self.chosen_id().to_owned();
        let Some(index) = self.css.iter().position(|css| css.id == id) else {
            return;
        };
        if self.css[index].paint.is_some() || self.css[index].look.background.is_some() {
            return;
        }
        let Some(path) = self.css[index].path.clone() else {
            return;
        };
        let paint = true;
        let options = recore_css::Options {
            overrides: self
                .overrides
                .get(&id)
                .map(|changed| {
                    changed
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            paint,
            ..recore_css::Options::default()
        };
        if let Ok(reloaded) =
            recore_css::load_with_imports(path, &options, &mut |url| fetch_remote(url))
        {
            self.css[index] = reloaded;
            self.css[index].id = id;
        }
    }

    /// Decodes the chosen CSS theme's background picture, if it names one
    /// ReCore can read, and keeps how far it is dimmed.
    fn load_background(&mut self) {
        let paint = self.chosen_css().and_then(|css| {
            if css.look.background.is_some() {
                return None;
            }
            css.paint
                .as_ref()
                .map(|paint| crate::css_paint::handle(paint, 960, 540))
        });
        self.paint = paint;
        if self.chosen_css().is_none() {
            self.set_background_picture(
                self.recore
                    .background
                    .as_deref()
                    .and_then(|path| load_picture_file(path)),
            );
            self.background_dim = self.recore.background_dim;
            return;
        }
        let (dim, background) = {
            let Some(css) = self.chosen_css() else {
                self.background = None;
                self.background_dim = 0.0;
                return;
            };
            let dim = css.look.background_dim.unwrap_or(0.0);
            let background = match &css.look.background {
                Some(recore_css::Image::Data(bytes)) => {
                    recore_core::chat::decode_picture(bytes, BACKGROUND_WIDTH, BACKGROUND_HEIGHT)
                }
                Some(recore_css::Image::File(path)) => load_picture_file(path),
                Some(recore_css::Image::Url(url)) => {
                    remote_picture(url, BACKGROUND_WIDTH, BACKGROUND_HEIGHT)
                }
                Some(recore_css::Image::Gradient(gradient)) => Some(gradient_picture(
                    gradient,
                    BACKGROUND_WIDTH,
                    BACKGROUND_HEIGHT,
                )),
                None => None,
            };
            (dim, background)
        };
        self.background_dim = dim;
        self.set_background_picture(background);
    }

    /// Keeps a decoded background and the handle that shows its first frame
    /// until an animation's player begins.
    fn set_background_picture(&mut self, picture: Option<recore_core::chat::Picture>) {
        self.background_picture = picture.map(std::sync::Arc::new);
        self.refresh_background_handle();
    }

    /// The grade the background is drawn with. The dim maps the picture to
    /// the palette the window is showing, so the panels and the ground agree
    /// instead of a grey sheet covering the picture.
    pub fn background_grade(&self, theme: &Theme) -> Grade {
        Grade::new(
            self.recore.background_filter,
            self.recore.background_brightness,
        )
        .with_palette(background_palette(theme), self.background_dim)
    }

    /// Makes the still background from the source picture and its grade.
    fn refresh_background_handle(&mut self) {
        let theme = self.shown();
        self.background = self.background_picture.as_ref().map(|picture| {
            let mut rgba = picture.rgba.to_vec();
            background::apply(&mut rgba, picture.width, self.background_grade(&theme));
            iced::widget::image::Handle::from_rgba(picture.width, picture.height, rgba)
        });
    }

    /// Whether the chosen CSS theme has a root or body animation ReCore can
    /// paint. The caller then keeps a clock running for it.
    pub fn has_animations(&self) -> bool {
        self.chosen_css()
            .is_some_and(|theme| !theme.look.animations.is_empty())
    }

    /// The chosen CSS theme's ground animations, in the order the file
    /// declares them. The canvas owns the clock and paints them without
    /// rebuilding the window for every step.
    pub fn animations(&self) -> &[recore_css::Animation] {
        self.chosen_css()
            .map(|theme| theme.look.animations.as_slice())
            .unwrap_or(&[])
    }

    /// How the chosen CSS theme asks its background to fill the window.
    pub fn background_fit(&self) -> recore_css::BackgroundFit {
        self.chosen_css()
            .map(|theme| theme.look.background_fit)
            .unwrap_or_default()
    }

    pub fn set_background_filter(&mut self, root: &Path, filter: Filter) {
        self.recore.background_filter = filter;
        layout::store(root, "recore_background_filter", filter.name());
        self.refresh_background_handle();
    }

    pub fn set_background_brightness(&mut self, root: &Path, percent: u8) {
        self.recore.background_brightness = f32::from(percent.clamp(25, 200)) / 100.0;
        layout::store(root, "recore_background_brightness", percent.clamp(25, 200));
        self.refresh_background_handle();
    }

    pub fn set_background_animate(&mut self, root: &Path, on: bool) {
        self.recore.background_animate = on;
        layout::store(root, "recore_background_animate", on);
    }

    /// Loads the fonts a theme carries and draws with its family.
    fn apply_font(&self, id: &str) {
        if let Some(css) = self.css.iter().find(|css| css.id == id) {
            for font in &css.fonts {
                fonts::load(&font.family, font.bytes.clone());
            }
            if let Some(family) = &css.look.font_family {
                fonts::set_family(Some(family));
                return;
            }
            fonts::set_family(None);
            return;
        }
        // ReCore's own type, a theme built on it, or any theme whose font
        // the person chose here.
        let based_on_recore = self
            .custom
            .iter()
            .find(|custom| custom.id == id)
            .is_some_and(|custom| matches!(custom.base, "recore" | "recore-light"));
        if matches!(id, "recore" | "recore-light") || based_on_recore || self.recore.font_chosen {
            fonts::set_family(recore_family(&self.recore.font));
        } else {
            fonts::set_family(None);
        }
    }

    /// Where the themes folder is, making it if it isn't there.
    pub fn themes_folder(&self) -> &Path {
        &self.folders[0]
    }
}

/// Gives the window the chosen theme's font before it draws its first frame
/// (the `default_font` iced lays un-fonted text with is read then), so a CSS
/// theme's type or ReCord's pixel type covers even text that names no font.
pub fn prepare_fonts(root: &Path) {
    let layout = Layout::read(root);
    if matches!(layout.theme.as_deref(), Some("recore" | "recore-light")) {
        fonts::set_family(recore_family(
            layout.recore_font.as_deref().unwrap_or("pixel"),
        ));
        return;
    }
    let themes = Themes::read(root, &layout);
    themes.apply_font(themes.chosen_id());
}

/// How large a background is decoded: large enough for a desktop, small
/// enough that a GIF's frames and a video's RGBA buffer stay reasonable.
const BACKGROUND_WIDTH: u32 = 1280;
const BACKGROUND_HEIGHT: u32 = 720;

/// Paints a CSS gradient into a still picture at the background size. A
/// theme's animated gradients become their first frame; the CSS animation
/// layer carries the movement.
fn gradient_picture(
    gradient: &recore_css::Gradient,
    width: u32,
    height: u32,
) -> recore_core::chat::Picture {
    use recore_css::GradientKind;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    let (width_f, height_f) = (width.max(1) as f32, height.max(1) as f32);
    let stops = gradient.resolved();
    for y in 0..height {
        for x in 0..width {
            let nx = x as f32 / width_f;
            let ny = y as f32 / height_f;
            let t = match gradient.kind {
                GradientKind::Linear(degrees) => {
                    let radians = degrees.to_radians();
                    ((nx - 0.5) * radians.sin() + (ny - 0.5) * -radians.cos() + 0.5).clamp(0.0, 1.0)
                }
                GradientKind::Radial => {
                    (((nx - 0.5).powi(2) + (ny - 0.5).powi(2)).sqrt() / 0.707).clamp(0.0, 1.0)
                }
                GradientKind::Conic => {
                    let angle = (ny - 0.5).atan2(nx - 0.5) + std::f32::consts::PI;
                    (angle / std::f32::consts::TAU).fract()
                }
            };
            let colour = recore_css::Gradient::sample(&stops, t);
            rgba.extend_from_slice(&[colour.r, colour.g, colour.b, colour.a]);
        }
    }
    recore_core::chat::Picture {
        width,
        height,
        rgba: recore_core::chat::Pixels::new(width, height, rgba),
        animation: None,
    }
}

/// Loads a background picture from a file. GIF, APNG, WebP and video files
/// keep their animation, which the window's picture players step.
fn load_picture_file(path: &Path) -> Option<recore_core::chat::Picture> {
    recore_core::chat::decode_picture_at(path, BACKGROUND_WIDTH, BACKGROUND_HEIGHT)
}

/// Downloads a picture a CSS theme names on the web and decodes it like a
/// local one.
fn remote_picture(url: &str, width: u32, height: u32) -> Option<recore_core::chat::Picture> {
    let Some(bytes) = fetch_remote(url) else {
        tracing::warn!(%url, "a theme's remote background picture did not download");
        return None;
    };
    recore_core::chat::decode_picture(&bytes, width, height)
}

/// The most a theme's remote file may be: enough for a long stylesheet or a
/// font, not enough to hold the window while a stray address answers.
const REMOTE_LIMIT: usize = 8 * 1024 * 1024;

/// How long a downloaded theme file stays in memory, so Reload CSS doesn't
/// ask the web for the same base theme over and over.
const REMOTE_TTL: Duration = Duration::from_secs(600);
/// How many remote files are kept: a folder of themes that share a base.
const REMOTE_KEEP: usize = 64;

type RemoteCache = Mutex<BTreeMap<String, (std::time::Instant, Vec<u8>)>>;

fn remote_cache() -> &'static RemoteCache {
    static CACHE: std::sync::OnceLock<RemoteCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// The folder remote theme files are kept in across starts. It follows the
/// data folder, so a theme works offline after its first load.
fn remote_cache_dir() -> &'static Mutex<Option<PathBuf>> {
    static DIR: std::sync::OnceLock<Mutex<Option<PathBuf>>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| Mutex::new(None))
}

fn set_remote_cache_dir(root: &Path) {
    if let Ok(mut dir) = remote_cache_dir().lock() {
        *dir = Some(root.join("theme-cache"));
    }
}

/// After a connection fails, other downloads wait this long before trying:
/// an offline machine should not stall startup once per import.
const REMOTE_RETRY: Duration = Duration::from_secs(30);

fn offline_until() -> &'static std::sync::Mutex<Option<std::time::Instant>> {
    static UNTIL: std::sync::OnceLock<std::sync::Mutex<Option<std::time::Instant>>> =
        std::sync::OnceLock::new();
    UNTIL.get_or_init(|| std::sync::Mutex::new(None))
}

fn remote_offline() -> bool {
    offline_until()
        .lock()
        .ok()
        .and_then(|until| *until)
        .is_some_and(|until| std::time::Instant::now() < until)
}

/// Files that answered 404 or were too large, so a theme that names one does
/// not ask the web for it again on every reload.
const REMOTE_FAILED_TTL: Duration = Duration::from_secs(300);

fn remote_failed() -> &'static std::sync::Mutex<BTreeMap<String, std::time::Instant>> {
    static FAILED: std::sync::OnceLock<std::sync::Mutex<BTreeMap<String, std::time::Instant>>> =
        std::sync::OnceLock::new();
    FAILED.get_or_init(|| std::sync::Mutex::new(BTreeMap::new()))
}

fn remote_failed_recently(url: &str) -> bool {
    remote_failed()
        .lock()
        .map(|failed| {
            failed
                .get(url)
                .is_some_and(|at| at.elapsed() < REMOTE_FAILED_TTL)
        })
        .unwrap_or(false)
}

fn note_remote_failure(url: &str) {
    if let Ok(mut failed) = remote_failed().lock() {
        if failed.len() >= REMOTE_KEEP {
            failed.clear();
        }
        failed.insert(url.to_owned(), std::time::Instant::now());
    }
}

fn note_remote_success(url: &str) {
    if let Ok(mut failed) = remote_failed().lock() {
        failed.remove(url);
    }
}

/// Forgets the remote theme files downloaded so far, so Reload CSS sees a
/// changed base theme at once.
pub fn clear_remote_cache() {
    if let Ok(mut cache) = remote_cache().lock() {
        cache.clear();
    }
    if let Ok(mut until) = offline_until().lock() {
        *until = None;
    }
    if let Ok(mut failed) = remote_failed().lock() {
        failed.clear();
    }
    if let Ok(dir) = remote_cache_dir().lock()
        && let Some(dir) = dir.as_ref()
    {
        let _ = fs::remove_dir_all(dir);
    }
}

/// Downloads `url` for the CSS interpreter: the remote `@import`s, fonts and
/// pictures a theme names. It runs on a thread of its own so that the
/// blocking client never starts a second runtime inside the window's.
fn fetch_remote(url: &str) -> Option<Vec<u8>> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return None;
    }
    if let Ok(mut cache) = remote_cache().lock()
        && let Some((at, bytes)) = cache.get_mut(url)
    {
        if at.elapsed() < REMOTE_TTL {
            *at = std::time::Instant::now();
            return Some(bytes.clone());
        }
    }
    let disk = disk_cache_read(url);
    let fallback = if disk.is_some() {
        disk.clone()
    } else {
        disk_cache_read_any(url)
    };
    if let Some(bytes) = &disk
        && let Ok(mut cache) = remote_cache().lock()
    {
        cache.insert(url.to_owned(), (std::time::Instant::now(), bytes.clone()));
    }
    if remote_offline() || remote_failed_recently(url) {
        return fallback;
    }
    let url_owned = url.to_owned();
    let fetched = std::thread::Builder::new()
        .name("recore-theme-fetch".into())
        .spawn(move || {
            let _ = recore_core::init_tls();
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent(concat!("ReCore/", env!("CARGO_PKG_VERSION")))
                .build()
                .ok()?;
            let response = match client.get(&url_owned).send() {
                Ok(response) => response,
                Err(error) => {
                    tracing::warn!(url = %url_owned, error = %error, "a theme's remote file did not connect");
                    note_remote_failure(&url_owned);
                    if let Ok(mut until) = offline_until().lock() {
                        *until = Some(std::time::Instant::now() + REMOTE_RETRY);
                    }
                    return None;
                }
            };
            if !response.status().is_success() {
                tracing::warn!(url = %url_owned, status = %response.status(), "a theme's remote file did not download");
                note_remote_failure(&url_owned);
                return None;
            }
            if response
                .content_length()
                .is_some_and(|length| length > REMOTE_LIMIT as u64)
            {
                tracing::warn!(url = %url_owned, "a theme's remote file is too large");
                note_remote_failure(&url_owned);
                return None;
            }
            let bytes = response.bytes().ok()?;
            if let Ok(mut until) = offline_until().lock() {
                *until = None;
            }
            if bytes.len() > REMOTE_LIMIT {
                tracing::warn!(url = %url_owned, "a theme's remote file is too large");
                note_remote_failure(&url_owned);
                return None;
            }
            note_remote_success(&url_owned);
            Some(bytes.to_vec())
        })
        .ok()?;
    let fetched = match fetched.join().ok().flatten() {
        Some(bytes) => bytes,
        None => return fallback,
    };
    disk_cache_write(url, &fetched);
    if let Ok(mut cache) = remote_cache().lock() {
        if cache.len() >= REMOTE_KEEP
            && let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(url, _)| url.clone())
        {
            cache.remove(&oldest);
        }
        cache.insert(url.to_owned(), (std::time::Instant::now(), fetched.clone()));
    }
    Some(fetched)
}

/// Downloads a theme URL into `folder` and returns the file it wrote. The
/// CSS keeps its remote imports; they are read by the interpreter as usual.
pub fn import_theme(folder: &Path, url: &str) -> Result<PathBuf, String> {
    let bytes = fetch_remote(url).ok_or_else(|| {
        "The theme did not download. Check the address and the connection.".to_owned()
    })?;
    if bytes.is_empty() {
        return Err("The theme downloaded empty.".to_owned());
    }
    fs::create_dir_all(folder)
        .map_err(|error| format!("Could not make the themes folder: {error}"))?;
    let mut name = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_owned();
    name = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    if name.is_empty() || name == "-" {
        name = "imported.theme.css".to_owned();
    } else if !name.to_ascii_lowercase().ends_with(".css") {
        name.push_str(".theme.css");
    }
    let mut path = folder.join(name);
    let mut number = 2;
    while path.exists() {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("imported");
        path = folder.join(format!("{stem}-{number}.theme.css"));
        number += 1;
    }
    fs::write(&path, bytes).map_err(|error| format!("Could not save the theme: {error}"))?;
    Ok(path)
}

/// A stable file name for a URL, without pulling in a hash dependency.
fn remote_key(url: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in url.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn disk_cache_path(url: &str) -> Option<PathBuf> {
    let dir = remote_cache_dir().lock().ok()?.as_ref().cloned()?;
    Some(dir.join(format!("{}.bin", remote_key(url))))
}

fn disk_cache_read(url: &str) -> Option<Vec<u8>> {
    let path = disk_cache_path(url)?;
    let meta = fs::metadata(&path).ok()?;
    if meta.len() > REMOTE_LIMIT as u64 {
        let _ = fs::remove_file(&path);
        return None;
    }
    if meta
        .modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > REMOTE_TTL)
    {
        return None;
    }
    fs::read(path)
        .ok()
        .filter(|bytes| bytes.len() <= REMOTE_LIMIT)
}

/// The cached copy even after its TTL, used as an offline fallback.
fn disk_cache_read_any(url: &str) -> Option<Vec<u8>> {
    let path = disk_cache_path(url)?;
    let meta = fs::metadata(&path).ok()?;
    if meta.len() > REMOTE_LIMIT as u64 {
        return None;
    }
    fs::read(path)
        .ok()
        .filter(|bytes| bytes.len() <= REMOTE_LIMIT)
}

fn disk_cache_write(url: &str, bytes: &[u8]) {
    let Some(path) = disk_cache_path(url) else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let temporary = path.with_extension("part");
    if fs::write(&temporary, bytes).is_ok() {
        let _ = fs::rename(&temporary, &path);
    }
    // Keep the folder from growing without limit when someone cycles through
    // many remote bases.
    if let Ok(entries) = fs::read_dir(parent) {
        let mut files: Vec<_> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "bin"))
            .collect();
        if files.len() > 128 {
            files.sort_by_key(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok());
            for old in files.iter().take(files.len() - 128) {
                let _ = fs::remove_file(old);
            }
        }
    }
}

/// The family ReCore's own theme draws with.
fn recore_family(name: &str) -> Option<&'static str> {
    match name {
        "mono" => Some("Pixel Operator Mono"),
        "system" | "discord" => None,
        _ => Some("Pixel Operator"),
    }
}

/// The folders a theme may come from: this data folder's, then the desktop's
/// own client folders. A later file with the same id keeps its own id.
fn theme_folders(root: &Path) -> Vec<PathBuf> {
    let own = root.join("themes");
    let _ = std::fs::create_dir_all(&own);
    let mut folders = Vec::new();
    let mut seen = BTreeSet::new();
    // Themes the packages carry. Windows keeps them beside recore.exe, the
    // AppImage and a distribution install keep them in share/recore, and a
    // macOS bundle keeps them in Contents/Resources. The first folder is
    // the data folder's, so a person's own theme wins over a bundled one.
    let mut bundled = Vec::new();
    if let Some(binary) = std::env::current_exe()
        .ok()
        .and_then(|binary| binary.parent().map(Path::to_path_buf))
    {
        bundled.push(binary.join("themes"));
        bundled.push(binary.join("../share/recore/themes"));
        bundled.push(binary.join("../Resources/themes"));
    }
    // Running from the repository finds the bundled themes too; a released
    // binary doesn't take themes from whatever folder it was started in.
    let here = PathBuf::from("themes");
    let here = cfg!(debug_assertions)
        .then_some(here)
        .filter(|folder| folder.is_dir());
    for folder in std::iter::once(own)
        .chain(bundled)
        .chain(here)
        .chain(recore_css::theme_folders())
    {
        let canonical = std::fs::canonicalize(&folder).unwrap_or_else(|_| folder.clone());
        if seen.insert(canonical) {
            folders.push(folder);
        }
    }
    folders
}

/// Fills the kit's theme from the core's.
macro_rules! from_palette {
    ($($name:ident,)*) => {
        /// The core's `theme`, named `id`, as the window paints with it.
        pub fn theme(id: &str, theme: &palette::Theme) -> Theme {
            let mut shown = Theme::new(id, theme.dark);
            shown.bonfire = theme.bonfire();
            $(shown.$name = color(theme.$name);)*
            shown.mention = color(theme.mention());
            shown.frame = shown.rail;
            shown.worked_out()
        }
    };
}

recore_iced_kit::for_theme_colours!(from_palette);

/// A CSS theme's colours as the window paints with them.
fn from_css(file: &ThemeFile) -> Theme {
    let theme = &file.palette;
    let mut shown = Theme::new(&file.id, theme.dark);
    shown.square = file.look.radius.unwrap_or(8.0) < 2.0;
    shown.panel_gap = file.look.panel_gap.unwrap_or(0.0);
    shown.panel_radius = file.look.panel_radius.unwrap_or(0.0);
    shown.sidebar_width = file
        .page_layout
        .map(|layout| layout.sidebar)
        .filter(|width| *width > 0.0);
    shown.members_width = file
        .page_layout
        .map(|layout| layout.members)
        .filter(|width| *width > 0.0);
    macro_rules! assign {
        ($($name:ident,)*) => {
            $(shown.$name = css_color(theme.$name);)*
        };
    }
    recore_iced_kit::for_theme_colours!(assign);
    shown.mention = css_color(theme.link);
    shown.frame = file.look.frame.map(css_color).unwrap_or(shown.rail);
    apply_kit_native(&mut shown, &file.native);
    apply_kit_type(&mut shown, &file.native);
    let mut shown = shown.worked_out();
    apply_kit_native_after(&mut shown, &file.native);
    shown
}

/// A CSS theme's colours as the core keeps them, for Appearance's tiles.
pub fn css_palette(file: &ThemeFile) -> palette::Theme {
    let theme = &file.palette;
    let mut found = palette::RECORE_DARK;
    found.dark = theme.dark;
    macro_rules! assign {
        ($($name:ident,)*) => {
            $(found.$name = core_colour(theme.$name);)*
        };
    }
    recore_iced_kit::for_theme_colours!(assign);
    apply_core_native(&mut found, &file.native);
    found
}

/// A selector-derived colour on the window's palette.
fn apply_kit_native(theme: &mut Theme, native: &NativeStyle) {
    macro_rules! set {
        ($($field:ident,)*) => {
            $(if let Some(colour) = native.$field {
                theme.$field = css_color(colour);
            })*
        };
    }
    set!(
        rail, sidebar, panel, chat, stage, raised, field, text, secondary, muted, hover, row_hover,
        line, accent, tooltip,
    );
    macro_rules! fade {
        ($($field:ident, $opacity:ident,)*) => {
            $(if let Some(opacity) = native.$opacity {
                theme.$field.a *= opacity.clamp(0.0, 1.0);
            })*
        };
    }
    fade!(
        rail,
        rail_opacity,
        sidebar,
        sidebar_opacity,
        panel,
        panel_opacity,
        chat,
        chat_opacity,
        raised,
        raised_opacity,
        field,
        field_opacity,
        stage,
        stage_opacity,
        tooltip,
        tooltip_opacity,
    );
    theme.navigation_gradient = native
        .sidebar_gradient
        .as_ref()
        .or(native.rail_gradient.as_ref())
        .and_then(css_gradient);
    theme.chat_gradient = native.chat_gradient.as_ref().and_then(css_gradient);
}

/// The type a CSS theme gives the window's own headings, before its colours
/// are worked out.
fn apply_kit_type(theme: &mut Theme, native: &NativeStyle) {
    if let Some(size) = native.category_size {
        theme.category_size = size;
    }
    if let Some(value) = native.category_transform {
        theme.category_transform = transform(value);
    }
    if let Some(size) = native.member_heading_size {
        theme.member_heading_size = size;
    }
    if let Some(value) = native.member_heading_transform {
        theme.member_heading_transform = transform(value);
    }
    if let Some(size) = native.header_size {
        theme.header_size = size;
    }
    if let Some(value) = native.header_transform {
        theme.header_transform = transform(value);
    }
}

/// The values `worked_out` would otherwise compute from the palette: the
/// selection, a field's hint, the scrollbar's inks and the popups' shadows.
fn apply_kit_native_after(theme: &mut Theme, native: &NativeStyle) {
    if let Some(colour) = native.selection {
        theme.selection = css_color(colour);
    }
    if let Some(colour) = native.placeholder {
        theme.placeholder = css_color(colour);
    }
    if let Some(colour) = native.scrollbar {
        theme.scrollbar_rest = css_color(colour);
    }
    if let Some(colour) = native.scrollbar_hover {
        theme.scrollbar_pointed = css_color(colour);
    }
    if let Some(shadow) = native.raised_shadow {
        theme.raised_shadow = css_shadow(shadow);
    }
    if let Some(shadow) = native.card_shadow {
        theme.card_shadow = css_shadow(shadow);
    }
}

/// A selector-derived colour on the core's palette, for Appearance's tile.
fn apply_core_native(theme: &mut palette::Theme, native: &NativeStyle) {
    macro_rules! set {
        ($($field:ident,)*) => {
            $(if let Some(colour) = native.$field {
                theme.$field = core_colour(colour);
            })*
        };
    }
    set!(
        rail, sidebar, panel, chat, stage, raised, field, text, secondary, muted, hover, row_hover,
        line, accent, tooltip,
    );
    macro_rules! fade {
        ($($field:ident, $opacity:ident,)*) => {
            $(if let Some(opacity) = native.$opacity {
                theme.$field = faded(theme.$field, opacity);
            })*
        };
    }
    fade!(
        rail,
        rail_opacity,
        sidebar,
        sidebar_opacity,
        panel,
        panel_opacity,
        chat,
        chat_opacity,
        raised,
        raised_opacity,
        field,
        field_opacity,
        stage,
        stage_opacity,
        tooltip,
        tooltip_opacity,
    );
}

/// `colour` at `opacity` of its own alpha.
fn faded(colour: Colour, opacity: f32) -> Colour {
    let alpha = (f32::from(colour.alpha()) * opacity.clamp(0.0, 1.0)) as u8;
    Colour::from_argb((u32::from(alpha) << 24) | colour.rgb_part())
}

fn transform(transform: recore_css::TextTransform) -> crate::theme::Transform {
    match transform {
        recore_css::TextTransform::None => crate::theme::Transform::None,
        recore_css::TextTransform::Uppercase => crate::theme::Transform::Uppercase,
        recore_css::TextTransform::Lowercase => crate::theme::Transform::Lowercase,
        recore_css::TextTransform::Capitalize => crate::theme::Transform::Capitalize,
    }
}

/// A CSS `box-shadow` as iced's.
/// A CSS linear gradient as iced paints it; radial and conic ones keep the
/// solid colour their region falls back to.
fn css_gradient(gradient: &recore_css::Gradient) -> Option<iced::Gradient> {
    let recore_css::GradientKind::Linear(angle) = gradient.kind else {
        return None;
    };
    let mut linear = iced::gradient::Linear::new(iced::Radians(angle.to_radians()));
    for (offset, color) in gradient.resolved() {
        linear = linear.add_stop(offset, css_color(color));
    }
    Some(linear.into())
}

fn css_shadow(shadow: recore_css::Shadow) -> iced::Shadow {
    iced::Shadow {
        color: css_color(shadow.color),
        offset: iced::Vector::new(shadow.offset_x, shadow.offset_y),
        blur_radius: shadow.blur,
    }
}

/// A core colour as iced's.
pub fn color(colour: Colour) -> Color {
    Color::from_rgba8(
        colour.red(),
        colour.green(),
        colour.blue(),
        f32::from(colour.alpha()) / 255.0,
    )
}

/// The colours Ambxst's wallpaper shader would see: the window's grounds,
/// inks and signals. Twelve are enough for the palette dim's weighted
/// average, and all are colours the window already shows.
fn background_palette(theme: &Theme) -> [[u8; 3]; 12] {
    let rgb = |colour: Color| {
        let [red, green, blue, _] = colour.into_rgba8();
        [red, green, blue]
    };
    [
        rgb(theme.chat),
        rgb(theme.rail),
        rgb(theme.sidebar),
        rgb(theme.raised),
        rgb(theme.field),
        rgb(theme.sunken),
        rgb(theme.line),
        rgb(theme.text),
        rgb(theme.secondary),
        rgb(theme.accent),
        rgb(theme.link),
        rgb(theme.danger),
    ]
}

/// A CSS colour as iced's.
fn css_color(colour: recore_css::Rgba) -> Color {
    Color::from_rgba8(colour.r, colour.g, colour.b, f32::from(colour.a) / 255.0)
}

/// A CSS colour as the core's.
fn core_colour(colour: recore_css::Rgba) -> Colour {
    Colour::from_argb(
        (u32::from(colour.a) << 24)
            | (u32::from(colour.r) << 16)
            | (u32::from(colour.g) << 8)
            | u32::from(colour.b),
    )
}

/// The path's real identity, so a theme moved between client folders is
/// still the same theme after a reload.
fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Opens a folder in the desktop's file manager, for Appearance's button.
pub fn open_folder(path: &Path) {
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(windows)]
    let opener = "explorer";
    let _ = std::process::Command::new(opener).arg(path).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LAYOUT;

    #[test]
    fn a_layout_without_a_theme_shows_recore_and_an_unknown_one_too() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(
            Themes::read(folder.path(), &Layout::read(folder.path()))
                .shown()
                .id,
            "recore"
        );
        std::fs::write(
            folder.path().join(LAYOUT),
            r#"{"theme": "gone", "zoom": 1.2}"#,
        )
        .unwrap();
        assert_eq!(
            Themes::read(folder.path(), &Layout::read(folder.path()))
                .shown()
                .id,
            "recore"
        );
    }

    #[test]
    fn a_renamed_css_theme_keeps_its_choice_and_options() {
        let folder = tempfile::tempdir().unwrap();
        let themes = folder.path().join("themes");
        std::fs::create_dir_all(&themes).unwrap();
        std::fs::write(
            themes.join("one.theme.css"),
            "/** @name One */ :root { --background-primary: #101010; }",
        )
        .unwrap();
        std::fs::write(
            folder.path().join(LAYOUT),
            r#"{"theme": "css:one", "css_options": {"css:one": {"--x": "1"}}}"#,
        )
        .unwrap();
        let mut loaded = Themes::read(folder.path(), &Layout::read(folder.path()));
        assert_eq!(loaded.chosen_id(), "css:one");
        assert!(loaded.overrides.contains_key("css:one"));
        std::fs::write(
            themes.join("one.theme.css"),
            "/** @name Two */ :root { --background-primary: #101010; }",
        )
        .unwrap();
        loaded.reload_local();
        assert_eq!(loaded.chosen_id(), "css:two");
        assert!(loaded.overrides.contains_key("css:two"));
        assert!(!loaded.overrides.contains_key("css:one"));
    }

    #[test]
    fn choosing_a_theme_keeps_the_rest_of_the_layout() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join(LAYOUT),
            r##"{"zoom": 1.2, "custom_themes": [{"id": "custom:1", "name": "Plum",
                "base": "sakura", "colours": {"chat": "#2a1a3a"}}]}"##,
        )
        .unwrap();
        let mut themes = Themes::read(folder.path(), &Layout::read(folder.path()));
        let plum = themes.find("custom:1").unwrap();
        assert!(!plum.dark, "Sakura's");
        assert_eq!(plum.chat, crate::style::hex(0x2a1a3a));
        themes.choose(folder.path(), "custom:1");
        crate::layout::flush(folder.path());
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.path().join(LAYOUT)).unwrap()).unwrap();
        assert_eq!(written["theme"], "custom:1");
        assert_eq!(written["zoom"], 1.2);
        assert_eq!(
            Themes::read(folder.path(), &Layout::read(folder.path())).shown(),
            plum
        );
    }

    #[test]
    fn the_worked_out_colours_follow_the_theme() {
        let recore = Themes {
            custom: Vec::new(),
            css: Vec::new(),
            problems: Vec::new(),
            folders: Vec::new(),
            root: PathBuf::new(),
            chosen: Some("recore".into()),
            accent: None,
            effects: true,
            low_power: false,
            recore: RecoreOptions::default(),
            overrides: BTreeMap::new(),
            background: None,
            background_picture: None,
            paint: None,
            background_dim: 0.0,
            fingerprint: 0,
        }
        .shown();
        assert_eq!(recore.accent, crate::style::hex(0xe94221));
        assert_eq!(recore.chat, crate::style::hex(0x080907));
        let light = Themes {
            chosen: Some("recore-light".into()),
            ..Themes {
                custom: Vec::new(),
                css: Vec::new(),
                problems: Vec::new(),
                folders: Vec::new(),
                root: PathBuf::new(),
                chosen: None,
                accent: None,
                effects: true,
                low_power: false,
                recore: RecoreOptions::default(),
                overrides: BTreeMap::new(),
                background: None,
                background_picture: None,
                paint: None,
                background_dim: 0.0,
                fingerprint: 0,
            }
        }
        .shown();
        assert_eq!(light.chat, crate::style::hex(0xf3e9d7));
    }

    #[test]
    fn a_recore_accent_is_kept_and_recolors_the_theme() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join(LAYOUT),
            r##"{"theme": "recore", "recore_accent": "#3ddc97"}"##,
        )
        .unwrap();
        let mut themes = Themes::read(folder.path(), &Layout::read(folder.path()));
        assert_eq!(themes.shown().accent, crate::style::hex(0x3ddc97));
        themes.set_accent(
            folder.path(),
            Some(recore_core::theme::Colour::rgb(0x4fb8c9)),
        );
        crate::layout::flush(folder.path());
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.path().join(LAYOUT)).unwrap()).unwrap();
        assert_eq!(written["recore_accent"], "#4fb8c9");
        assert_eq!(
            Themes::read(folder.path(), &Layout::read(folder.path()))
                .shown()
                .accent,
            crate::style::hex(0x4fb8c9)
        );
    }

    #[test]
    fn a_remote_file_that_failed_is_not_asked_again_at_once() {
        let url = "https://preview.invalid/a-file-that-is-not-there.css";
        note_remote_failure(url);
        assert!(remote_failed_recently(url));
        note_remote_success(url);
        assert!(!remote_failed_recently(url));
    }

    #[test]
    fn a_css_theme_in_the_themes_folder_is_offered_and_paints() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(folder.path().join("themes")).unwrap();
        std::fs::write(
            folder.path().join("themes/plum.theme.css"),
            r#"/** @name Plum */
            :root { --background-primary: #201020; --text-normal: #f0e0f0;
                --brand-experiment: #c04080; --font-primary: serif; }"#,
        )
        .unwrap();
        let themes = Themes::read(folder.path(), &Layout::read(folder.path()));
        let plum = themes
            .css
            .iter()
            .find(|theme| theme.id == "css:plum")
            .expect("the CSS theme is listed");
        assert_eq!(plum.name, "Plum");
        let shown = themes.find("css:plum").expect("its colours");
        assert_eq!(shown.chat, crate::style::hex(0x201020));
        assert_eq!(shown.accent, crate::style::hex(0xc04080));
    }

    #[test]
    fn a_chosen_css_page_is_painted_with_its_operations() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(folder.path().join("themes")).unwrap();
        std::fs::write(
            folder.path().join("themes/paint.theme.css"),
            r#"/** @name Paint */
            :root { --background-primary: #101010; }
            html { border: 12px solid #40a0c0; }"#,
        )
        .unwrap();
        let layout = Layout {
            theme: Some("css:paint".into()),
            ..Layout::default()
        };
        let themes = Themes::read(folder.path(), &layout);
        assert!(themes.paint.is_some(), "the page's paint image is made");
    }

    #[test]
    fn a_css_themes_gradients_reach_the_rail_and_chat() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(folder.path().join("themes")).unwrap();
        std::fs::write(
            folder.path().join("themes/gradient.theme.css"),
            r#"/** @name Gradient */
            :root { --background-primary: #101010; }
            [class*="sidebarList_"] { background: linear-gradient(180deg, #111111, #222222); }
            [class*="chat_"] { background: linear-gradient(90deg, #010101, #020202); }"#,
        )
        .unwrap();
        let themes = Themes::read(folder.path(), &Layout::read(folder.path()));
        let shown = themes.find("css:gradient").expect("its colours");
        assert!(shown.navigation_gradient.is_some());
        assert!(shown.chat_gradient.is_some());
    }

    #[test]
    fn a_flex_css_page_names_the_sidebar_width() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(folder.path().join("themes")).unwrap();
        std::fs::write(
            folder.path().join("themes/flexed.theme.css"),
            r#"/** @name Flexed */
            body { display: flex; }
            [class*="guilds_"] { width: 64px; }
            [class*="sidebar_"] { width: 260px; }
            [class*="members_"] { width: 240px; }
            [class*="chat_"] { flex: 1; }"#,
        )
        .unwrap();
        let themes = Themes::read(folder.path(), &Layout::read(folder.path()));
        let shown = themes.find("css:flexed").expect("its colours");
        assert_eq!(shown.sidebar_width, Some(260.0));
        assert_eq!(shown.members_width, Some(240.0));
    }

    #[test]
    fn a_panelled_css_theme_reaches_the_window_layout() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(folder.path().join("themes")).unwrap();
        std::fs::write(
            folder.path().join("themes/panelled.theme.css"),
            r#"/** @name Panelled */
            :root { --background-base-lower: #0a0a0a; --app-frame-background: #111111;
                --gap: 7px; --roundness: 6px; }
            body { background: transparent; }"#,
        )
        .unwrap();
        let themes = Themes::read(folder.path(), &Layout::read(folder.path()));
        let shown = themes.find("css:panelled").expect("its colours");
        assert_eq!(shown.chat, crate::style::hex(0x0a0a0a));
        assert_eq!(shown.frame, crate::style::hex(0x111111));
        assert_eq!(shown.panel_gap, 7.0);
        assert_eq!(shown.panel_radius, 6.0);
    }

    #[test]
    fn a_css_themes_new_regions_and_decorations_reach_the_window() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(folder.path().join("themes")).unwrap();
        std::fs::write(
            folder.path().join("themes/ornate.theme.css"),
            r#"/** @name Ornate */
            :root { --background-primary: #101010; }
            [class*="stage_"] { background: #0a0b0c; }
            [class*="profileBanner_"] { background: #0d0e0f; }
            [class*="membersWrap_"] { background: #070809; }
            [class*="sidebar_"] { opacity: 0.5; }
            ::selection { background: #112233; }
            input::placeholder { color: #445566; }
            ::-webkit-scrollbar-thumb { background: #778899; }
            ::-webkit-scrollbar-thumb:hover { background: #aabbcc; }
            [class*="menu_"] {
                box-shadow: 0 4px 12px rgba(0, 0, 0, 0.6);
                transition: opacity 240ms ease;
            }
            [class*="userPopout_"] { box-shadow: 0 8px 30px #00000080; }
            [class*="sidebarList_"] [class*="category_"] {
                font-size: 11px;
                text-transform: uppercase;
            }
            [class*="membersGroup_"] {
                font-size: 12px;
                text-transform: lowercase;
            }
            [class*="header_"] {
                font-size: 15px;
                text-transform: capitalize;
            }
            "#,
        )
        .unwrap();
        let mut layout = Layout::read(folder.path());
        layout.theme = Some("css:ornate".into());
        let themes = Themes::read(folder.path(), &layout);
        let shown = themes.find("css:ornate").expect("its colours");
        assert_eq!(shown.stage, crate::style::hex(0x0a0b0c));
        assert_eq!(shown.raised, crate::style::hex(0x0d0e0f));
        assert_eq!(shown.panel, crate::style::hex(0x070809));
        assert!(
            (shown.sidebar.a - 0.5).abs() < 0.01,
            "the sidebar's own opacity fades its ground"
        );
        assert_eq!(shown.selection, crate::style::hex(0x112233));
        assert_eq!(shown.placeholder, crate::style::hex(0x445566));
        assert_eq!(shown.scrollbar_rest, crate::style::hex(0x778899));
        assert_eq!(shown.scrollbar_pointed, crate::style::hex(0xaabbcc));
        assert_eq!(shown.raised_shadow.offset.y, 4.0);
        assert_eq!(shown.raised_shadow.blur_radius, 12.0);
        assert_eq!(shown.card_shadow.offset.y, 8.0);
        assert_eq!(shown.card_shadow.blur_radius, 30.0);
        assert_eq!(shown.category_size, 11.0);
        assert_eq!(shown.category_transform, crate::theme::Transform::Uppercase);
        assert_eq!(shown.member_heading_size, 12.0);
        assert_eq!(
            shown.member_heading_transform,
            crate::theme::Transform::Lowercase
        );
        assert_eq!(shown.header_size, 15.0);
        assert_eq!(shown.header_transform, crate::theme::Transform::Capitalize);
        assert_eq!(
            themes.motion_pace(),
            Some((std::time::Duration::from_millis(240), 0))
        );
        // Appearance's tile reads the same selector colours from the core's
        // palette.
        let plum = themes
            .css
            .iter()
            .find(|css| css.id == "css:ornate")
            .unwrap();
        let tile = css_palette(plum);
        assert_eq!(tile.stage, Colour::rgb(0x0a0b0c));
        assert_eq!(tile.raised, Colour::rgb(0x0d0e0f));
        assert_eq!(tile.panel, Colour::rgb(0x070809));
    }
}
