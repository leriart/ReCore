//! Reads a Discord CSS theme into the colours and look ReCore paints with.
//!
//! A theme is a set of custom properties on the page's root (`:root`,
//! `html`, `.theme-dark`, `.theme-light`), usually Discord's own tokens
//! (`--background-primary`, `--text-normal`, `--brand-experiment`), plus the
//! theme's own variables. [`load`] follows them through `var()` and
//! `color-mix()`, settles `@layer`, `@media` and `@container style(...)`
//! conditions, picks the dark or light set for the client's appearance, and
//! maps them onto ReCore's [`Palette`]. ReCord's `--rl-*` palette is read as
//! well, so ReCord's own CSS theme gives the same colours as the built-in
//! theme.
//!
//! What CSS cannot carry into a native window is styling for Discord's DOM:
//! rules on `.messageContent_...`, `@keyframes`, layouts and so on have no
//! nodes to match here. Themes load their colours, fonts, radius, background
//! picture and the ReCord effects they name; the rest is kept in the file for
//! people, not painted.
//!
//! `@import`s are local by default. [`load_with_imports`] and
//! [`scan_with_imports`] take a fetch callback and also read `http:` and
//! `https:` imports, their nested imports and remote `@font-face` files.
//! A remote picture is returned as [`Image::Url`] for the caller to fetch
//! when it shows the theme.

use crate::{
    base64, color,
    css::{self, Variables},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
};

pub use crate::color::Rgba;
pub use crate::css::{Meta, Stylesheet};

/// Which appearance a theme is read for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// The theme's own root colours decide, or the desktop's appearance.
    #[default]
    Auto,
    Dark,
    Light,
}

/// How much motion the theme asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Motion {
    #[default]
    Full,
    Subtle,
    Off,
}

/// The ReCord CRT effects a theme names, from 0 to 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Effects {
    pub scanlines: f32,
    pub grain: f32,
    pub vignette: f32,
    pub flicker: bool,
    pub beam: bool,
    pub frame: f32,
    pub avatar_filter: Option<String>,
    pub avatar_scanline: f32,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            scanlines: 0.25,
            grain: 0.18,
            vignette: 0.55,
            flicker: false,
            beam: true,
            frame: 0.6,
            avatar_filter: None,
            avatar_scanline: 0.14,
        }
    }
}

/// A picture a theme puts behind the window.
#[derive(Clone, Debug, PartialEq)]
pub enum Image {
    /// The bytes a `data:` URI carries.
    Data(Vec<u8>),
    /// A file beside the theme.
    File(PathBuf),
    /// A picture on the web, which the caller may choose to fetch.
    Url(String),
    /// A CSS gradient, painted by the client at the window's background size.
    Gradient(Gradient),
}

/// How a CSS gradient runs across the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientKind {
    /// Degrees, as CSS names them: 0 points up, 90 to the right.
    Linear(f32),
    Radial,
    Conic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub color: Rgba,
    /// The stop's position, 0..1, or `None` to distribute it evenly.
    pub position: Option<f32>,
}

/// A `linear-gradient()`, `radial-gradient()` or `conic-gradient()`.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub stops: Vec<GradientStop>,
}

impl Gradient {
    /// The colour at `position` along the gradient, with unpositioned stops
    /// spread between their positioned neighbours. Painters that sample many
    /// pixels should resolve once with [`resolved`](Self::resolved) and call
    /// [`sample`](Self::sample).
    pub fn at(&self, position: f32) -> Rgba {
        Self::sample(&self.resolved(), position)
    }

    /// The stops with every position filled in, in order.
    pub fn resolved(&self) -> Vec<(f32, Rgba)> {
        let stops = &self.stops;
        let mut resolved = Vec::with_capacity(stops.len());
        let mut previous = 0.0f32;
        for (index, stop) in stops.iter().enumerate() {
            let position = match stop.position {
                Some(position) => position.clamp(0.0, 1.0),
                None if index == 0 => 0.0,
                None if index + 1 == stops.len() => 1.0,
                None => {
                    let next = stops[index..]
                        .iter()
                        .enumerate()
                        .find_map(|(at, stop)| stop.position.map(|position| (at, position)));
                    let (gaps, target) =
                        next.map_or((stops.len() - index, 1.0), |(at, target)| (at + 1, target));
                    previous + (target - previous) / gaps as f32
                }
            };
            previous = position.max(previous);
            resolved.push((position, stop.color));
        }
        resolved
    }

    /// Samples an already resolved stop list.
    pub fn sample(stops: &[(f32, Rgba)], position: f32) -> Rgba {
        if stops.is_empty() {
            return Rgba::TRANSPARENT;
        }
        if stops.len() == 1 {
            return stops[0].1;
        }
        let position = position.clamp(0.0, 1.0);
        for pair in stops.windows(2) {
            let (start, start_color) = pair[0];
            let (end, end_color) = pair[1];
            if position <= end {
                let span = (end - start).max(f32::EPSILON);
                return start_color.mix(end_color, ((position - start) / span).clamp(0.0, 1.0));
            }
        }
        // Past the last stop, a gradient keeps its last colour, even when
        // that stop sits before 1.
        stops[stops.len() - 1].1
    }
}

/// A font a theme carries, ready for the window's font system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font {
    pub family: String,
    pub weight: u16,
    pub italic: bool,
    pub bytes: Vec<u8>,
}

/// How a background picture fills the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackgroundFit {
    #[default]
    Cover,
    Contain,
    Fill,
}

/// The panel widths a theme's page-level flex or grid settles at, in CSS
/// pixels. The window lets the person drag the sidebar; this is the width a
/// theme asks for when nobody has.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PageLayout {
    pub rail: f32,
    pub sidebar: f32,
    pub chat: f32,
    pub members: f32,
}

/// What a theme asks of the window besides its colours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub letter_spacing: Option<f32>,
    pub radius: Option<f32>,
    /// The window's ground behind the gaps between panels.
    pub frame: Option<Rgba>,
    /// The space between the window's panels, and their corner radius, as
    /// themes like Ryoku and Midnight lay them out.
    pub panel_gap: Option<f32>,
    pub panel_radius: Option<f32>,
    pub motion: Motion,
    pub pixel_icons: bool,
    pub effects: Effects,
    pub background: Option<Image>,
    /// How far the background picture is dimmed, 0..1.
    pub background_dim: Option<f32>,
    /// How opaque the panels over a background picture are, 0..1.
    pub panel_opacity: Option<f32>,
    /// How the background picture fills the window.
    pub background_fit: BackgroundFit,
    /// The accent ReCore's own controls use, as the theme names it.
    pub accent: Option<Rgba>,
    /// Colours and opacity keyframes a root or body animation writes. They
    /// are painted as the window's moving ground; animations on Discord's
    /// DOM-only elements stay in the file.
    pub animations: Vec<Animation>,
}

/// One step of a colour or opacity animation ReCore can paint.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimationFrame {
    pub offset: f32,
    pub background: Option<Rgba>,
    pub color: Option<Rgba>,
    pub opacity: Option<f32>,
}

/// A `@keyframes` animation on the window's own root or body.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Animation {
    pub name: String,
    pub duration: f32,
    pub delay: f32,
    /// `None` is CSS's infinite.
    pub iterations: Option<f32>,
    pub direction: AnimationDirection,
    pub frames: Vec<AnimationFrame>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

impl Animation {
    /// The colour and opacity at `elapsed` seconds into the window's life.
    /// `None` once a finite animation has finished. A missing channel is
    /// interpolated from its nearest named step, as CSS does.
    pub fn at(&self, elapsed: f32) -> Option<AnimationFrame> {
        if self.frames.is_empty() || self.duration <= 0.0 {
            return None;
        }
        let local = elapsed - self.delay.max(0.0);
        if local < 0.0 {
            return self.frames.first().cloned();
        }
        let cycle = local / self.duration;
        let mut index = cycle.floor() as u32;
        if let Some(iterations) = self.iterations
            && cycle >= iterations
        {
            return None;
        }
        let mut progress = cycle.fract();
        let alternate = matches!(
            self.direction,
            AnimationDirection::Alternate | AnimationDirection::AlternateReverse
        );
        if alternate && index % 2 == 1 {
            progress = 1.0 - progress;
        }
        if matches!(
            self.direction,
            AnimationDirection::Reverse | AnimationDirection::AlternateReverse
        ) {
            progress = 1.0 - progress;
        }
        // A float cycle can land a hair below the next iteration's start.
        if progress >= 1.0 {
            progress = 0.0;
            index = index.wrapping_add(1);
        }
        let _ = index;
        let before = self
            .frames
            .iter()
            .take_while(|frame| frame.offset <= progress)
            .last()
            .unwrap_or(&self.frames[0]);
        let after = self
            .frames
            .iter()
            .find(|frame| frame.offset >= progress)
            .unwrap_or_else(|| self.frames.last().expect("a frame"));
        let span = (after.offset - before.offset).max(f32::EPSILON);
        let mix = ((progress - before.offset) / span).clamp(0.0, 1.0);
        Some(AnimationFrame {
            offset: progress,
            background: mix_colour(before.background, after.background, mix),
            color: mix_colour(before.color, after.color, mix),
            opacity: mix_value(before.opacity, after.opacity, mix),
        })
    }
}

fn mix_colour(left: Option<Rgba>, right: Option<Rgba>, mix: f32) -> Option<Rgba> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.mix(right, mix)),
        (Some(colour), None) | (None, Some(colour)) => Some(colour),
        (None, None) => None,
    }
}

fn mix_value(left: Option<f32>, right: Option<f32>, mix: f32) -> Option<f32> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left + (right - left) * mix),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

/// A theme's colours, one field per colour ReCore's window paints with. The
/// names are the ones `recore_core::theme::Theme` uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub dark: bool,
    pub rail: Rgba,
    pub sidebar: Rgba,
    pub panel: Rgba,
    pub chat: Rgba,
    pub stage: Rgba,
    pub raised: Rgba,
    pub field: Rgba,
    pub sunken: Rgba,
    pub hover: Rgba,
    pub active: Rgba,
    pub row_hover: Rgba,
    pub line: Rgba,
    pub edge: Rgba,
    pub outline: Rgba,
    pub outline_chat: Rgba,
    pub outline_field: Rgba,
    pub text: Rgba,
    pub secondary: Rgba,
    pub muted: Rgba,
    pub faint: Rgba,
    pub accent: Rgba,
    pub accent_hover: Rgba,
    pub on_accent: Rgba,
    pub accent_soft: Rgba,
    pub danger: Rgba,
    pub danger_hover: Rgba,
    pub on_danger: Rgba,
    pub live: Rgba,
    pub loud: Rgba,
    pub link: Rgba,
    pub button_default: Rgba,
    pub button_default_hover: Rgba,
    pub overlay: Rgba,
    pub shimmer: Rgba,
    pub reply_spine: Rgba,
    pub tooltip: Rgba,
    pub avatar: Rgba,
}

/// A theme read from CSS, with what it says about itself.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeFile {
    pub id: String,
    pub name: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub license: Option<String>,
    pub source: Option<String>,
    pub path: Option<PathBuf>,
    pub palette: Palette,
    pub look: Look,
    /// Colours and shape values won by selectors that name the native
    /// sidebar, chat, member list and so on.
    pub native: crate::NativeStyle,
    /// The panel widths a page-level flex or grid names.
    pub page_layout: Option<PageLayout>,
    /// The page's paint list from style-engine, when paint was asked for.
    pub paint: Option<crate::Paint>,
    pub fonts: Vec<Font>,
    /// The custom properties Appearance can change, with their labels.
    pub options: Vec<crate::ThemeOption>,
    pub warnings: Vec<String>,
    /// What the reader left out and where, when [`Options::diagnostics`] asked
    /// for it.
    pub diagnostics: Vec<Diagnostic>,
    /// Whether it names a remote import, font or picture, which a local scan
    /// leaves for the window to fetch when the theme is chosen.
    pub remote: bool,
}

impl ThemeFile {
    /// The CSS file it came from, if it came from one.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

/// What [`scan`] found.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub themes: Vec<ThemeFile>,
    pub problems: Vec<Problem>,
}

/// A theme file that could not be read, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub path: PathBuf,
    pub message: String,
}

/// What the reader did with a rule or declaration it could not use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// Read, and kept in the file, but not painted here.
    Skipped,
    /// Read and dropped: the CSS has no meaning at that point.
    Dropped,
    /// The reader itself could not do something: a missing import, a block
    /// left open, an at-rule it does not read.
    Warning,
}

/// One thing the reader could not use, with where it sits in the theme.
///
/// A theme is written for another client, so a reader drops rules all the
/// time; `ThemeFile::warnings` says what in a sentence, and these say where,
/// for an editor or Appearance to point at. `Options::diagnostics` turns the
/// collection on, since reading every scanned theme for them costs a second
/// parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// The file the declaration sits in, when it is not the theme itself.
    pub file: Option<PathBuf>,
    /// A 1-based line of the file, or 0 when the reader cannot say.
    pub line: u32,
    /// A 1-based column, or 0.
    pub column: u32,
    pub message: String,
}

/// How to read a theme.
#[derive(Clone, Debug)]
pub struct Options {
    pub mode: Mode,
    /// A colour to force on the theme, as ReCore's accent picker does.
    pub accent: Option<Rgba>,
    /// Custom properties to force over the theme's, for the settings the
    /// window writes (`--rl-scanline-opacity`, `--rl-motion`, and so on).
    pub overrides: Vec<(String, String)>,
    /// Whether to read the fonts the theme carries.
    pub fonts: bool,
    /// Read the chosen page's style-engine paint list. `false` keeps the
    /// scan to the semantic values only.
    pub paint: bool,
    /// Collect a [`Diagnostic`] for every rule or declaration left out, with
    /// its line and column. Off by default: a scan of many themes pays a
    /// second parse for it.
    pub diagnostics: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            mode: Mode::Auto,
            accent: None,
            overrides: Vec::new(),
            fonts: true,
            paint: false,
            diagnostics: false,
        }
    }
}

impl Options {
    pub fn with_mode(mode: Mode) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }
}

/// Reads a theme file and the fonts and pictures beside it.
pub fn load(path: impl AsRef<Path>) -> Result<ThemeFile, String> {
    load_with(path, &Options::default())
}

/// The largest theme file, imported sheet or font the reader keeps, in bytes.
///
/// A theme is data and may name anything; a local `@import` of a device, a
/// pipe or a multi-gigabyte file would otherwise stop the window from ever
/// opening. Eight mebibytes holds every theme and font the known clients
/// ship (the largest bundled theme is 447 KiB) and matches the remote limit
/// `docs/themes.md` gives.
const LOCAL_LIMIT: u64 = 8 * 1024 * 1024;

/// Reads a local file the theme names, refusing anything that is not a
/// regular file of a bounded size. `Err` says why, for the caller's warning.
fn read_local(path: &Path, what: &str) -> Result<Vec<u8>, String> {
    // A device, a FIFO or a directory is refused before it is opened: opening
    // a pipe waits for its writer, and reading a device has no end. A link
    // that turns into one is caught again below.
    if fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_file()) {
        let followed =
            fs::metadata(path).map_err(|error| format!("could not read the {what}: {error}"))?;
        if !followed.is_file() {
            return Err(format!("the {what} is not a regular file; left out"));
        }
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    // A file that turns into a pipe between the check and the open still
    // cannot hold the window: with the flag, opening one does not wait for a
    // writer.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x800);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("could not read the {what}: {error}"))?;
    // The metadata comes from the open file, so a swap besides the open
    // cannot change what is read.
    let metadata = file
        .metadata()
        .map_err(|error| format!("could not read the {what}: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("the {what} is not a regular file; left out"));
    }
    let mut bytes = Vec::new();
    (&file)
        .take(LOCAL_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read the {what}: {error}"))?;
    if bytes.len() as u64 > LOCAL_LIMIT {
        return Err(format!(
            "the {what} is larger than {} MiB; left out",
            LOCAL_LIMIT / (1024 * 1024)
        ));
    }
    Ok(bytes)
}

/// Reads a theme file with `options`, downloading the `http:` and `https:`
/// imports and fonts it names through `fetch`. A remote picture stays an
/// [`Image::Url`], which the caller fetches when it shows the theme.
pub fn load_with_imports(
    path: impl AsRef<Path>,
    options: &Options,
    fetch: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
) -> Result<ThemeFile, String> {
    let path = path.as_ref();
    let css = read_local(path, "theme")
        .and_then(|bytes| {
            String::from_utf8(bytes).map_err(|_| "the theme is not readable UTF-8".to_owned())
        })
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    load_css_with_imports(path, &css, options, fetch)
}

/// [`load_with_imports`] from CSS already read.
fn load_css_with_imports(
    path: &Path,
    css: &str,
    options: &Options,
    fetch: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
) -> Result<ThemeFile, String> {
    let directory = path.parent().map(Path::to_path_buf);
    let theme = load_str_with_imports(css, directory.as_deref(), options, fetch)?;
    Ok(named(theme, path))
}

/// A theme parsed from `path`, named and numbered after the file when the
/// CSS itself names neither.
fn named(mut theme: ThemeFile, path: &Path) -> ThemeFile {
    theme.path = Some(path.to_path_buf());
    for diagnostic in &mut theme.diagnostics {
        diagnostic.file.get_or_insert_with(|| path.to_path_buf());
    }
    if theme.name == "CSS theme"
        && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
    {
        stem.trim_end_matches(".theme").clone_into(&mut theme.name);
    }
    theme.id = format!("css:{}", slug(theme.name.as_str()));
    theme
}

/// Reads a theme file with `options`.
pub fn load_with(path: impl AsRef<Path>, options: &Options) -> Result<ThemeFile, String> {
    load_with_imports(path, options, &mut |_| None)
}

/// Reads the CSS `css`, with `directory` as the base for its imports, fonts
/// and pictures.
pub fn load_str(
    css: &str,
    directory: Option<&Path>,
    options: &Options,
) -> Result<ThemeFile, String> {
    load_str_with_imports(css, directory, options, &mut |_| None)
}

/// [`load_str`], downloading remote imports and fonts through `fetch`.
pub fn load_str_with_imports(
    css: &str,
    directory: Option<&Path>,
    options: &Options,
    fetch: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
) -> Result<ThemeFile, String> {
    let meta = Meta::read(css);
    let mut warnings = Vec::new();
    let mut remote = false;
    let sheet = css::parse_in(css, directory, &mut |base, path| {
        if is_remote(path) || base.is_some_and(remote_path) {
            remote = true;
            let url = remote_target(base, path);
            match fetch(&url) {
                Some(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => Some((text, PathBuf::from(url))),
                    Err(_) => {
                        warnings.push(format!("the @import {url} is not UTF-8 CSS"));
                        None
                    }
                },
                None => None,
            }
        } else {
            read_import(base, path, &mut warnings)
        }
    });
    let mut fonts = Vec::new();
    if options.fonts {
        for face in &sheet.fonts {
            let mut bytes = face.data.clone();
            if bytes.is_none() {
                if let Some(url) = remote_source(&face.source, face.origin.as_deref()) {
                    remote = true;
                    bytes = fetch(&url);
                    if bytes.is_none() {
                        warnings.push(format!("could not download the font {url}"));
                    }
                } else if !face.origin.as_deref().is_some_and(is_remote) {
                    let base = face
                        .origin
                        .as_deref()
                        .filter(|origin| !is_remote(origin))
                        .map(Path::new)
                        .or(directory);
                    bytes = read_font(base, &face.source, &mut warnings);
                }
            }
            if let Some(bytes) = bytes
                && !bytes.is_empty()
            {
                fonts.push(Font {
                    family: face.family.clone(),
                    weight: face.weight,
                    italic: face.italic,
                    bytes,
                });
            }
        }
    }
    let mut sheet = sheet;
    let has_containers = sheet.rules.iter().any(|rule| !rule.containers.is_empty());
    // A container query must not decide the palette before its own
    // condition is settled; the rules outside one are the base it reads.
    let appearance = if has_containers {
        collect_where(&sheet, |rule| rule.containers.is_empty())
    } else {
        collect(&sheet)
    };
    // The first pass only settles which containers apply; Ryoku's light
    // template keeps its ground inside `@container style(--light-mode: on)`,
    // so the appearance must be chosen again once those rules are in.
    let provisional = if has_containers {
        choose_dark(&appearance, options.mode, &mut Vec::new())
    } else {
        choose_dark(&appearance, options.mode, &mut warnings)
    };
    if has_containers {
        let base = settle_vars(&sheet, &appearance, provisional, options);
        sheet.rules.retain(|rule| {
            rule.containers
                .iter()
                .all(|condition| css::container_applies(condition, &base))
        });
    }
    let collected = collect(&sheet);
    let dark = if has_containers {
        choose_dark(&collected, options.mode, &mut warnings)
    } else {
        provisional
    };
    let vars = settle_vars(&sheet, &collected, dark, options);
    let semantic = crate::semantic::compute(&sheet, &vars, dark);
    let (palette, mut native, page_layout, page_gap) = match semantic {
        Some(computed) => (
            computed.palette,
            computed.native,
            computed.page_layout,
            computed.gap,
        ),
        // A theme the engine cannot settle still gets the lightweight
        // reader's colours, never a blank window.
        None => (
            palette(&vars, dark),
            crate::NativeStyle::compute(&sheet.rules, &vars, dark),
            None,
            None,
        ),
    };
    region_opacities(&mut native, &vars);
    let animations = animations(&sheet, &collected, dark, &vars);
    let direct_background = direct_background(&sheet, &vars, directory, dark);
    let paint = options
        .paint
        .then(|| crate::semantic::paint(&sheet, &vars, dark))
        .flatten();
    warnings.extend(sheet.warnings);
    let mut look = look(
        &vars,
        palette,
        directory,
        &fonts,
        options,
        animations,
        direct_background,
        &mut warnings,
    );
    look.radius = native.radius.or(look.radius);
    look.panel_gap = look.panel_gap.or(page_gap);
    look.font_family = native.font_family.clone().or(look.font_family);
    look.font_size = native.font_size.or(look.font_size);
    look.letter_spacing = native.letter_spacing.or(look.letter_spacing);
    remote |= matches!(look.background, Some(Image::Url(_)));
    let mut specs = crate::options::merge(
        crate::options::detect_with_notes(&vars, dark, &sheet.notes, &sheet.properties),
        crate::options::detect_with_notes(
            &collected.vars(!dark),
            !dark,
            &sheet.notes,
            &sheet.properties,
        ),
        !dark,
    );
    crate::options::apply_meta(&mut specs, &meta.vars);
    let name = meta.name.clone().unwrap_or_else(|| "CSS theme".to_owned());
    let diagnostics = if options.diagnostics {
        diagnostics_of(css, &warnings)
    } else {
        Vec::new()
    };
    Ok(ThemeFile {
        id: format!("css:{}", slug(&name)),
        name,
        author: meta.author,
        description: meta.description,
        version: meta.version,
        license: meta.license,
        source: meta.source.or(meta.website),
        path: None,
        palette,
        look,
        native,
        page_layout,
        paint,
        fonts,
        options: specs,
        warnings,
        diagnostics,
        remote,
    })
}

/// What the engine drops or skips when it reads the theme's own text, with
/// the line and column the file writes them on, plus the reader's own notes.
fn diagnostics_of(css: &str, warnings: &[String]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let sheet = crate::style_engine::css::stylesheet::parse_stylesheet(css);
    for warning in &sheet.report.warnings {
        let severity = match warning.severity {
            crate::style_engine::error::ParseSeverity::Dropped => Severity::Dropped,
            _ => Severity::Skipped,
        };
        let message = warning.message.as_str();
        // The engine also reports the selector it wraps a dropped
        // declaration in as invalid, and the bare "invalid rule skipped"
        // that follows a reported selector; either would say it twice.
        if message.contains("invalid selector ':is(") || message == "invalid rule skipped" {
            continue;
        }
        // The pseudo-elements ReCore paints itself, from its own rules.
        const PAINTED: &[&str] = &[
            "::-webkit-scrollbar",
            "::selection",
            "::placeholder",
            "::before",
            "::after",
            "-moz-selection",
            "-moz-placeholder",
        ];
        if PAINTED.iter().any(|pseudo| message.contains(pseudo)) {
            continue;
        }
        // The engine's message carries its own parser's debug form after the
        // selector; the line and column already say where it is.
        let message = message
            .split_once("': ParseError")
            .map_or_else(|| message.to_owned(), |(before, _)| format!("{before}'"));
        let diagnostic = Diagnostic {
            severity,
            file: None,
            line: warning.line,
            column: warning.column,
            message,
        };
        if !diagnostics.contains(&diagnostic) {
            diagnostics.push(diagnostic);
        }
    }
    for warning in warnings {
        diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            file: None,
            line: 0,
            column: 0,
            message: warning.clone(),
        });
    }
    diagnostics
}

/// Whether `path` was written as an absolute web address.
fn is_remote(path: &str) -> bool {
    path.get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || path
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        || path.starts_with("//")
}

fn remote_path(path: &Path) -> bool {
    is_remote(&path.to_string_lossy())
}

/// The web address a remote import names, resolved against the remote file
/// that imports it.
fn remote_target(base: Option<&Path>, path: &str) -> String {
    if path.starts_with("//") {
        return format!("https:{path}");
    }
    if is_remote(path) {
        return path.to_owned();
    }
    let Some(base) = base.filter(|base| remote_path(base)) else {
        return path.to_owned();
    };
    let base = base.to_string_lossy();
    let scheme = base.split("://").next().unwrap_or("https");
    if let Some(path) = path.strip_prefix('/') {
        let host = base
            .split("://")
            .nth(1)
            .unwrap_or("")
            .split('/')
            .next()
            .unwrap_or("");
        return format!("{scheme}://{host}/{path}");
    }
    normalize_remote(&format!("{}/{}", base.trim_end_matches('/'), path))
}

/// Collapses `./` and `../` in a web address, as a server would.
fn normalize_remote(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let Some((host, path)) = rest.split_once('/') else {
        return url.to_owned();
    };
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    if segments.is_empty() {
        format!("{scheme}://{host}")
    } else {
        format!("{scheme}://{host}/{}", segments.join("/"))
    }
}

/// The first `url(...)` in a `src` or `background` value when it is a web
/// address, resolved against a remote stylesheet's own folder.
fn remote_source(source: &str, origin: Option<&str>) -> Option<String> {
    let start = css::find_ignore_case(source, "url(")?;
    let rest = &source[start + 4..];
    let end = rest.find(')')?;
    let path = rest[..end].trim().trim_matches(['"', '\'']);
    if path.starts_with("data:") {
        return None;
    }
    if is_remote(path) {
        return Some(remote_target(None, path));
    }
    if origin.is_some_and(is_remote) && !path.starts_with("file://") {
        return Some(remote_target(origin.map(Path::new), path));
    }
    None
}

fn read_import(
    directory: Option<&Path>,
    path: &str,
    warnings: &mut Vec<String>,
) -> Option<(String, PathBuf)> {
    let path = path.trim().trim_start_matches("file://");
    let full = resolve_path(directory, path)?;
    match read_local(&full, "import") {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => Some((text, full)),
            Err(_) => {
                warnings.push(format!("the import {} is not UTF-8 CSS", full.display()));
                None
            }
        },
        Err(error) => {
            warnings.push(format!("{error}: {}", full.display()));
            None
        }
    }
}

fn resolve_path(directory: Option<&Path>, path: &str) -> Option<PathBuf> {
    let path = Path::new(path);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    Some(directory?.join(path))
}

/// Reads a font named by a `src: url(...)`.
fn read_font(
    directory: Option<&Path>,
    source: &str,
    warnings: &mut Vec<String>,
) -> Option<Vec<u8>> {
    let start = css::find_ignore_case(source, "url(")?;
    let rest = &source[start + 4..];
    let end = rest.find(')')?;
    let path = rest[..end].trim().trim_matches(['"', '\'']);
    if is_remote(path) {
        // The caller downloads remote fonts; a missing fetcher leaves none.
        return None;
    }
    let path = path.trim_start_matches("file://");
    let full = resolve_path(directory, path)?;
    match read_local(&full, "font") {
        Ok(bytes) => Some(bytes),
        Err(error) => {
            warnings.push(format!("{error}: {}", full.display()));
            None
        }
    }
}

/// Every declaration that writes on the root, with the appearance it applies
/// to, in source order: `None` for both, `Some(true)` for the dark palette and
/// `Some(false)` for the light one.
#[derive(Default)]
struct Collected {
    entries: Vec<Entry>,
}

struct Entry {
    declaration: css::Declaration,
    dark: Option<bool>,
    specificity: u32,
    order: usize,
    origin: Option<String>,
    layer: Option<u32>,
}

fn collect(sheet: &Stylesheet) -> Collected {
    collect_where(sheet, |_| true)
}

/// [`collect`] over the rules `allow` names, so a `@container style(...)`
/// that does not match the theme's root values cannot set the palette.
fn collect_where(sheet: &Stylesheet, allow: impl Fn(&css::Rule) -> bool) -> Collected {
    let mut collected = Collected::default();
    let mut order = 0usize;
    for rule in &sheet.rules {
        if !allow(rule) {
            continue;
        }
        for selector in &rule.selectors {
            let Some((selector_dark, specificity)) =
                css::root_selector_with_specificity(selector, true)
            else {
                continue;
            };
            let explicit = selector_is_explicit(selector);
            let scope = match (explicit.then_some(selector_dark), rule.color_scheme) {
                // `.theme-light` inside `prefers-color-scheme: dark` can never
                // apply, and neither can the other way around.
                (Some(a), Some(b)) if a != b => continue,
                (Some(dark), _) => Some(dark),
                (None, Some(dark)) => Some(dark),
                (None, None) => None,
            };
            for declaration in &rule.declarations {
                collected.entries.push(Entry {
                    declaration: declaration.clone(),
                    dark: scope,
                    specificity,
                    order,
                    origin: rule.origin.clone(),
                    layer: rule.layer,
                });
                order += 1;
            }
        }
    }
    collected
}

/// Whether the selector names an appearance itself (`.theme-dark`,
/// `.theme-light`), rather than leaving it to the client.
fn selector_is_explicit(selector: &str) -> bool {
    let lower = selector.to_ascii_lowercase();
    lower.contains("theme-light")
        || lower.contains("theme-dark")
        || lower.contains("theme-darker")
        || lower.contains("theme-midnight")
        || lower.contains("theme-ash")
        || lower.contains("data-rl-mode=\"light\"")
        || lower.contains("data-rl-mode='light'")
        || lower.contains("data-rl-mode=\"dark\"")
        || lower.contains("data-rl-mode='dark'")
}

impl Entry {
    /// CSS layers outrank specificity: a later layer beats an earlier one,
    /// and unlayered rules beat every layer. The weight packs the layer in
    /// above the selector's specificity.
    fn layer_specificity(&self) -> u32 {
        let layer = self.layer.map_or(1_000_000_000, |rank| {
            rank.saturating_add(1).saturating_mul(1_000_000)
        });
        layer.saturating_add(self.specificity.min(999_999))
    }
}

impl Collected {
    /// The cascade for `dark`.
    fn vars_with(&self, dark: bool) -> css::Vars {
        let mut vars = css::Vars::default();
        for entry in &self.entries {
            if entry.dark.is_some_and(|scope| scope != dark) {
                continue;
            }
            vars.insert_weighted(
                &entry.declaration,
                entry.layer_specificity(),
                entry.order,
                entry.origin.as_deref(),
            );
        }
        vars
    }

    fn vars(&self, dark: bool) -> css::Vars {
        self.vars_with(dark)
    }

    /// The appearance the theme asks for: an explicit `color-scheme`, then
    /// the ground it paints. The dark and light blocks are read before the
    /// unscoped base, because a base ground is what a dark block overrides;
    /// reading the base first made a dark theme that starts from Discord's
    /// light defaults look light.
    fn appearance(&self) -> Option<bool> {
        let dark_vars = self.vars_with(true);
        if let Some(dark) = appearance_of(&dark_vars, false) {
            return Some(dark);
        }
        let light_vars = self.vars_with(false);
        if let Some(dark) = appearance_of(&light_vars, true) {
            return Some(dark);
        }
        let mut vars = css::Vars::default();
        for entry in &self.entries {
            if entry.dark.is_none() {
                vars.insert_weighted(
                    &entry.declaration,
                    entry.layer_specificity(),
                    entry.order,
                    entry.origin.as_deref(),
                );
            }
        }
        appearance_of(&vars, true)
    }
}

/// The appearance a set of root declarations names: `color-scheme`, then the
/// brightness of the ground. `light` chooses `light-dark()`'s branch.
fn appearance_of(vars: &css::Vars, light: bool) -> Option<bool> {
    if let Some(scheme) = vars.get("color-scheme") {
        let lower = scheme.to_ascii_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();
        // `light dark` lets the client choose; the ground's brightness does.
        match (words.contains(&"dark"), words.contains(&"light")) {
            (true, false) => return Some(true),
            (false, true) => return Some(false),
            _ => {}
        }
    }
    let background = vars.first_color(
        &[
            "--background-primary",
            "--background-base-lower",
            "--background-application",
            "--chat-background",
            "--background",
            "--background-secondary",
            "--backgroundsecondary",
            "--backgroundprimary",
        ],
        light,
    )?;
    Some(background.is_dark())
}

fn choose_dark(collected: &Collected, mode: Mode, warnings: &mut Vec<String>) -> bool {
    match mode {
        Mode::Dark => true,
        Mode::Light => false,
        Mode::Auto => {
            if let Some(dark) = collected.appearance() {
                return dark;
            }
            // A missing ground means Discord's dark default. A `.theme-light`
            // block alone is not a declaration that the whole theme is light:
            // most dark themes carry one for Discord's light mode.
            warnings.push(
                "the theme names no background; read as a dark theme. Choose Light in Appearance if it is a light one."
                    .into(),
            );
            true
        }
    }
}

/// Reads a colour in `names`, with the fallback chain `then` and finally
/// `fallback`.
fn pick<V: Variables + ?Sized>(
    vars: &V,
    names: &[&str],
    light: bool,
    then: &[&str],
    fallback: Rgba,
) -> Rgba {
    vars.first_color(names, light)
        .or_else(|| vars.first_color(then, light))
        .unwrap_or(fallback)
}

/// A translucent token laid over `ground`, as the browser paints it.
fn over<V: Variables + ?Sized>(
    vars: &V,
    names: &[&str],
    light: bool,
    ground: Rgba,
) -> Option<Rgba> {
    vars.first_color(names, light)
        .map(|color| color.over(ground))
}

/// A length in a custom property: `px`, `rem` or `em`, or a `calc()`,
/// `min()`, `max()` or `clamp()` over them. Both relative units are 16 px,
/// the size the window starts a page at.
fn length(vars: &css::Vars, names: &[&str]) -> Option<f32> {
    for name in names {
        let Some(raw) = vars.get(name) else {
            continue;
        };
        let Some(value) = vars.resolve(raw) else {
            continue;
        };
        let value = value.trim().to_ascii_lowercase();
        // Absolute and viewport units become pixels before the expression is
        // read, so `calc(9pt + 1vmin)` settles here as well as in the engine.
        let value = crate::css::normalize_units(&value);
        if let Some(value) = eval_length(&value) {
            return Some(value);
        }
    }
    None
}

/// How deeply `calc()` and parentheses may nest before the value is refused,
/// so a hostile file cannot exhaust the stack.
const MAX_LENGTH_NESTING: usize = 32;

/// Evaluates a length expression: a `px`, `rem` or `em` number, or `calc()`,
/// `min()`, `max()` and `clamp()` over them with `+`, `-`, `*`, `/` and
/// nested parentheses. A percentage or an unknown unit has no context here
/// and gives `None`.
fn eval_length(value: &str) -> Option<f32> {
    let mut parser = LengthParser {
        text: value,
        index: 0,
    };
    let length = parser.sum(0)?;
    parser.skip_space();
    (parser.index == value.len() && length.is_finite()).then_some(length)
}

/// The parser behind [`eval_length`]: CSS's sums and products of lengths,
/// with nested parentheses and the four maths functions.
struct LengthParser<'a> {
    text: &'a str,
    index: usize,
}

impl LengthParser<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.index).copied()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.index += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn sum(&mut self, depth: usize) -> Option<f32> {
        let mut value = self.product(depth)?;
        loop {
            self.skip_space();
            match self.peek() {
                Some(b'+') => {
                    self.index += 1;
                    value += self.product(depth)?;
                }
                Some(b'-') => {
                    self.index += 1;
                    value -= self.product(depth)?;
                }
                _ => break,
            }
        }
        value.is_finite().then_some(value)
    }

    fn product(&mut self, depth: usize) -> Option<f32> {
        let mut value = self.term(depth)?;
        loop {
            self.skip_space();
            match self.peek() {
                Some(b'*') => {
                    self.index += 1;
                    value *= self.term(depth)?;
                }
                Some(b'/') => {
                    self.index += 1;
                    let divisor = self.term(depth)?;
                    if divisor == 0.0 {
                        return None;
                    }
                    value /= divisor;
                }
                _ => break,
            }
        }
        value.is_finite().then_some(value)
    }

    fn term(&mut self, depth: usize) -> Option<f32> {
        self.skip_space();
        let sign = match self.peek() {
            Some(b'+') => {
                self.index += 1;
                1.0
            }
            Some(b'-') => {
                self.index += 1;
                -1.0
            }
            _ => 1.0,
        };
        let value = sign * self.atom(depth)?;
        value.is_finite().then_some(value)
    }

    fn atom(&mut self, depth: usize) -> Option<f32> {
        if depth > MAX_LENGTH_NESTING {
            return None;
        }
        if self.eat(b'(') {
            let value = self.sum(depth + 1)?;
            self.skip_space();
            return self.eat(b')').then_some(value);
        }
        if self.peek().is_some_and(|byte| byte.is_ascii_alphabetic()) {
            return self.function(depth);
        }
        self.number()
    }

    fn function(&mut self, depth: usize) -> Option<f32> {
        let start = self.index;
        while self.peek().is_some_and(|byte| byte.is_ascii_alphabetic()) {
            self.index += 1;
        }
        let name = self.text.get(start..self.index)?.to_ascii_lowercase();
        self.skip_space();
        if !self.eat(b'(') {
            return None;
        }
        let value = match name.as_str() {
            "calc" => {
                let value = self.sum(depth + 1)?;
                self.skip_space();
                if !self.eat(b')') {
                    return None;
                }
                value
            }
            "min" | "max" => self.min_max(depth + 1, name == "min")?,
            "clamp" => self.clamp(depth + 1)?,
            _ => return None,
        };
        value.is_finite().then_some(value)
    }

    fn min_max(&mut self, depth: usize, minimum: bool) -> Option<f32> {
        let mut value = self.sum(depth)?;
        loop {
            self.skip_space();
            if self.eat(b',') {
                let next = self.sum(depth)?;
                value = if minimum {
                    value.min(next)
                } else {
                    value.max(next)
                };
                continue;
            }
            return self.eat(b')').then_some(value);
        }
    }

    fn clamp(&mut self, depth: usize) -> Option<f32> {
        let minimum = self.sum(depth)?;
        self.skip_space();
        if !self.eat(b',') {
            return None;
        }
        let value = self.sum(depth)?;
        self.skip_space();
        if !self.eat(b',') {
            return None;
        }
        let maximum = self.sum(depth)?;
        self.skip_space();
        if !self.eat(b')') {
            return None;
        }
        Some(value.min(maximum).max(minimum))
    }

    fn number(&mut self) -> Option<f32> {
        let start = self.index;
        let mut digits = 0usize;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.index += 1;
            digits += 1;
        }
        if self.peek() == Some(b'.') {
            self.index += 1;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.index += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return None;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let exponent = self.index;
            self.index += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.index += 1;
            }
            let exponent_start = self.index;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.index += 1;
            }
            if self.index == exponent_start {
                self.index = exponent;
            }
        }
        let number = self.text.get(start..self.index)?.parse::<f32>().ok()?;
        let unit_start = self.index;
        while self.peek().is_some_and(|byte| byte.is_ascii_alphabetic()) {
            self.index += 1;
        }
        let unit = self.text.get(unit_start..self.index)?;
        let factor = match unit.to_ascii_lowercase().as_str() {
            "" | "px" => 1.0,
            "rem" | "em" => 16.0,
            _ => return None,
        };
        let length = number * factor;
        length.is_finite().then_some(length)
    }
}

/// A percentage custom property as 0..1 (`25`, `25%` or `0.25`).
fn fraction(vars: &css::Vars, names: &[&str]) -> Option<f32> {
    for name in names {
        let Some(raw) = vars.get(name) else {
            continue;
        };
        let Some(value) = vars.resolve(raw) else {
            continue;
        };
        let value = value.trim();
        if let Some(percent) = value.strip_suffix('%') {
            if let Ok(value) = percent.trim().parse::<f32>() {
                return Some((value / 100.0).clamp(0.0, 1.0));
            }
        } else if let Ok(value) = value.parse::<f32>() {
            // ReCord writes 0.25; older themes write 25 for the same level.
            return Some(if value > 1.0 {
                (value / 100.0).clamp(0.0, 1.0)
            } else {
                value.clamp(0.0, 1.0)
            });
        }
    }
    None
}

/// A CSS duration in seconds (`6s`, `250ms` or `0`).
fn duration(vars: &css::Vars, name: &str) -> Option<f32> {
    let raw = vars.get(name)?;
    let value = vars.resolve(raw)?;
    let value = value.trim().to_ascii_lowercase();
    if let Some(number) = value.strip_suffix("ms") {
        number
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| value / 1000.0)
    } else if let Some(number) = value.strip_suffix('s') {
        number.trim().parse::<f32>().ok()
    } else {
        value.parse::<f32>().ok()
    }
}

fn image(vars: &css::Vars, directory: Option<&Path>, names: &[&str]) -> Option<Image> {
    for name in names {
        let Some(raw) = vars.get(name) else {
            continue;
        };
        let Some(value) = vars.resolve(raw) else {
            continue;
        };
        if let Some(image) = image_value(&value, vars.origin(name), directory) {
            return Some(image);
        }
    }
    None
}

/// A `url(...)` or a bare path, as a variable or a `body` rule names it.
fn image_value(value: &str, origin: Option<&str>, directory: Option<&Path>) -> Option<Image> {
    if value.trim().eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some(gradient) = gradient(value) {
        return Some(Image::Gradient(gradient));
    }
    let path = if let Some(start) = css::find_ignore_case(value, "url(") {
        let rest = &value[start + 4..];
        let end = rest.find(')')?;
        rest[..end].trim().trim_matches(['"', '\''])
    } else {
        // A field the window wrote may hold the path without `url()`.
        let path = value.trim().trim_matches(['"', '\'']);
        if path.is_empty() {
            return None;
        }
        path
    };
    let path = path.trim().trim_start_matches("file://");
    if path.starts_with("data:") {
        css_data_uri(path).map(Image::Data)
    } else if is_remote(path) {
        Some(Image::Url(remote_target(None, path)))
    } else if origin.is_some_and(is_remote) {
        // A relative picture in a downloaded stylesheet is downloaded from
        // beside it, not looked for on this machine.
        Some(Image::Url(remote_target(origin.map(Path::new), path)))
    } else {
        let base = origin.filter(|origin| !is_remote(origin)).map(Path::new);
        resolve_path(base.or(directory), path)
            .filter(|full| full.exists())
            .map(Image::File)
    }
}

/// Reads a CSS gradient in a `background`/`background-image` value. ReCore
/// paints the common three kinds; repeating variants use the same stops.
pub(crate) fn gradient(value: &str) -> Option<Gradient> {
    let lower = value.to_ascii_lowercase();
    // The repeating name contains the plain one, so it is matched first.
    let (kind, at) = if let Some(at) = css::find_ignore_case(value, "repeating-linear-gradient(") {
        (GradientKind::Linear(180.0), at)
    } else if let Some(at) = css::find_ignore_case(value, "repeating-radial-gradient(") {
        (GradientKind::Radial, at)
    } else if let Some(at) = css::find_ignore_case(value, "repeating-conic-gradient(") {
        (GradientKind::Conic, at)
    } else if let Some(at) = css::find_ignore_case(value, "linear-gradient(") {
        (GradientKind::Linear(180.0), at)
    } else if let Some(at) = css::find_ignore_case(value, "radial-gradient(") {
        (GradientKind::Radial, at)
    } else {
        let at = css::find_ignore_case(value, "conic-gradient(")?;
        (GradientKind::Conic, at)
    };
    let open = lower[at..].find('(')? + at;
    let close = matching_paren(value, open)?;
    let inside = &value[open + 1..close];
    let parts = color::split_top(inside, ',');
    if parts.len() < 2 {
        return None;
    }
    let mut kind = kind;
    let mut first = 0;
    if matches!(kind, GradientKind::Linear(_)) {
        let head = parts[0].trim();
        let head_lower = head.to_ascii_lowercase();
        if head_lower.starts_with("to ")
            || head_lower.ends_with("deg")
            || head_lower.ends_with("turn")
            || head_lower.ends_with("rad")
        {
            let angle = if let Some(angle) = head_lower.strip_suffix("deg") {
                angle.trim().parse::<f32>().ok()
            } else if let Some(angle) = head_lower.strip_suffix("turn") {
                angle.trim().parse::<f32>().ok().map(|turns| turns * 360.0)
            } else if let Some(angle) = head_lower.strip_suffix("rad") {
                angle.trim().parse::<f32>().ok().map(f32::to_degrees)
            } else {
                match head_lower.as_str() {
                    "to top" => Some(0.0),
                    "to right" => Some(90.0),
                    "to bottom" => Some(180.0),
                    "to left" => Some(270.0),
                    "to top right" | "to right top" => Some(45.0),
                    "to bottom right" | "to right bottom" => Some(135.0),
                    "to bottom left" | "to left bottom" => Some(225.0),
                    "to top left" | "to left top" => Some(315.0),
                    _ => None,
                }
            };
            if let Some(angle) = angle {
                kind = GradientKind::Linear(angle);
                first = 1;
            }
        }
    } else {
        // A radial's shape/position and a conic's `from` are not painted by
        // the simple painter; their stops still make the gradient.
        let head = parts[0].trim().to_ascii_lowercase();
        if head.contains(" at ")
            || head.starts_with("circle")
            || head.starts_with("ellipse")
            || head.starts_with("closest")
            || head.starts_with("farthest")
            || head.starts_with("from ")
            || head.starts_with("at ")
        {
            first = 1;
        }
    }
    let mut stops = Vec::new();
    for part in parts.into_iter().skip(first) {
        let mut tokens = color::split_top(part.trim(), ' ');
        let mut color = None;
        let mut position = None;
        for token in &mut tokens {
            let token = token.trim();
            if let Some(found) = color::parse(token, false) {
                color = Some(found);
            } else if let Some(value) = token.strip_suffix('%') {
                position = value
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .map(|value| (value / 100.0).clamp(0.0, 1.0));
            }
        }
        if let Some(color) = color {
            stops.push(GradientStop { color, position });
        }
    }
    (stops.len() >= 2).then_some(Gradient { kind, stops })
}

fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0u32;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in text[open..].char_indices() {
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
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Reads a root/body rule's direct `background` and `color`, as themes that
/// do not use Discord's custom properties paint the page. The values become
/// the same synthetic variables the palette reads, so a theme that writes
/// `body { background: #111; color: #eee; }` gets the right ground and text
/// even when it never names `--background-primary` or `--text-normal`.
/// The root custom properties a theme settles to: its cascade, the colours
/// its page rules paint directly, the initial values `@property` declares,
/// the accent and the options the person changed.
fn settle_vars(
    sheet: &Stylesheet,
    collected: &Collected,
    dark: bool,
    options: &Options,
) -> css::Vars {
    let mut vars = collected.vars(dark);
    direct_vars(sheet, &mut vars, dark);
    for (name, spec) in &sheet.properties {
        if vars.contains(name) {
            continue;
        }
        if let Some(initial) = &spec.initial {
            vars.insert(&css::Declaration {
                name: name.clone(),
                value: initial.clone(),
                important: false,
            });
        }
    }
    if let Some(accent) = options.accent {
        let hex = accent.hex();
        for name in [
            "--rl-accent",
            "--rl-signal",
            "--brand-experiment",
            "--brand-500",
            "--brand-360",
            "--control-primary-background-default",
        ] {
            vars.insert(&css::Declaration {
                name: name.into(),
                value: hex.clone(),
                important: true,
            });
        }
        let tint = vars
            .first_color(&["--rl-tint"], !dark)
            .unwrap_or(accent.ink())
            .hex();
        for (name, value) in [
            (
                "--rl-signal-hot",
                format!("color-mix(in srgb, {hex} 80%, {tint})"),
            ),
            (
                "--rl-signal-soft",
                format!("color-mix(in srgb, {hex} 58%, {tint})"),
            ),
        ] {
            vars.insert(&css::Declaration {
                name: name.into(),
                value,
                important: true,
            });
        }
    }
    for (name, value) in &options.overrides {
        vars.insert(&css::Declaration {
            name: name.clone(),
            value: value.clone(),
            important: true,
        });
    }
    if vars.contains("--rl-accent") && !vars.contains("--rl-signal") {
        let value = vars
            .get("--rl-accent")
            .map(str::to_owned)
            .unwrap_or_else(|| "var(--rl-accent)".into());
        vars.insert(&css::Declaration {
            name: "--rl-signal".into(),
            value,
            important: false,
        });
    }
    vars
}

fn direct_vars(sheet: &Stylesheet, vars: &mut css::Vars, dark: bool) {
    let mut found: BTreeMap<&'static str, String> = BTreeMap::new();
    for rule in &sheet.rules {
        if rule
            .color_scheme
            .is_some_and(|color_scheme| color_scheme != dark)
        {
            continue;
        }
        let root = rule.selectors.iter().any(|selector| {
            let selector = selector.split("::").next().unwrap_or(selector);
            css::root_selector(selector, dark).is_some()
        });
        if !root {
            continue;
        }
        for declaration in &rule.declarations {
            let name = match declaration.name.trim().to_ascii_lowercase().as_str() {
                "background" | "background-color" => "--background-primary",
                "color" => "--text-normal",
                "background-size" => "--background-size",
                _ => continue,
            };
            let Some(value) = vars.resolve(&declaration.value) else {
                continue;
            };
            let value = value.trim();
            if value.is_empty()
                || value.len() > 4096
                || value.contains("url(")
                || value.contains("gradient(")
                || value.eq_ignore_ascii_case("none")
                || value.eq_ignore_ascii_case("transparent")
            {
                continue;
            }
            let color = color::parse(value, !dark);
            if name == "--background-size" || color.is_some_and(|color| color.a > 0) {
                found.insert(name, value.to_owned());
            }
        }
    }
    for (name, value) in found {
        vars.insert(&css::Declaration {
            name: name.into(),
            value,
            important: false,
        });
    }
}

/// A background a root or body rule paints directly, as many Discord themes
/// do (`body { background-image: url(...) }`), rather than through a custom
/// property. Pseudo-elements count too: a theme's `body::before` is where
/// its wallpaper commonly lives.
fn direct_background(
    sheet: &Stylesheet,
    vars: &css::Vars,
    directory: Option<&Path>,
    dark: bool,
) -> Option<Image> {
    let mut found = None;
    for rule in &sheet.rules {
        if rule
            .color_scheme
            .is_some_and(|color_scheme| color_scheme != dark)
        {
            continue;
        }
        let root = rule.selectors.iter().any(|selector| {
            let selector = selector.split("::").next().unwrap_or(selector);
            css::root_selector(selector, dark).is_some()
        });
        if !root {
            continue;
        }
        for declaration in &rule.declarations {
            if !matches!(
                declaration.name.trim().to_ascii_lowercase().as_str(),
                "background" | "background-image"
            ) {
                continue;
            }
            let Some(value) = vars.resolve(&declaration.value) else {
                continue;
            };
            if let Some(image) = image_value(&value, rule.origin.as_deref(), directory) {
                found = Some(image);
            }
        }
    }
    found
}

fn css_data_uri(text: &str) -> Option<Vec<u8>> {
    let inside = text.strip_prefix("data:")?;
    let (meta, data) = inside.split_once(',')?;
    if meta.contains(";base64") {
        base64::decode(data)
    } else {
        Some(percent_decode(data))
    }
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

/// Maps a theme's custom properties onto ReCore's colours.
pub fn palette<V: Variables + ?Sized>(vars: &V, dark: bool) -> Palette {
    let light = !dark;
    // The modern Discord tokens, then the ones before Discord's 2024 redesign.
    let ground = pick(
        vars,
        &[
            "--background-primary",
            "--backgroundprimary",
            "--background-base-lower",
            "--chat-background",
            "--background",
        ],
        light,
        &[
            "--background-secondary",
            "--backgroundsecondary",
            "--background-base-lowest",
            "--background-tertiary",
        ],
        if dark {
            Rgba::rgb(0x31, 0x33, 0x38)
        } else {
            Rgba::rgb(0xff, 0xff, 0xff)
        },
    );
    let text = pick(
        vars,
        &[
            "--text-normal",
            "--text-default",
            "--normal-text",
            "--header-primary",
            "--textbrightest",
        ],
        light,
        &["--primary-100", "--text-strong", "--textbright", "--white"],
        ground.ink(),
    );
    let accent = pick(
        vars,
        &[
            "--brand-experiment",
            "--brand-500",
            "--control-primary-background-default",
            "--background-brand",
            "--blurple-50",
            "--accentcolor",
            "--main-color",
        ],
        light,
        &["--brand-360", "--text-brand", "--accentcolor2"],
        Rgba::rgb(0x58, 0x65, 0xf2),
    );
    let wash = if dark {
        Rgba::rgb(255, 255, 255)
    } else {
        Rgba::rgb(0, 0, 0)
    };
    let raised = pick(
        vars,
        &[
            "--background-floating",
            "--backgroundfloating",
            "--background-surface-high",
            "--modal-background",
            "--card-background-default",
            "--background-nested-floating",
        ],
        light,
        &[
            "--background-secondary-alt",
            "--background-secondary",
            "--backgroundsecondary",
        ],
        ground.mix(wash, 0.06),
    );
    let field = pick(
        vars,
        &[
            "--channeltextarea-background",
            "--input-background-default",
            "--backgroundcode",
            "--background-code",
        ],
        light,
        &[
            "--background-secondary-alt",
            "--backgroundsecondaryalt",
            "--background-tertiary",
            "--backgroundtertiary",
        ],
        ground.mix(if dark { Rgba::rgb(0, 0, 0) } else { wash }, 0.05),
    );
    let sunken = pick(
        vars,
        &[
            "--background-base-lowest",
            "--background-tertiary",
            "--backgroundtertiary",
            "--background-accent",
        ],
        light,
        &["--background-secondary-alt", "--backgroundsecondaryalt"],
        ground.mix(if dark { Rgba::rgb(0, 0, 0) } else { wash }, 0.08),
    );
    let sidebar = pick(
        vars,
        &[
            "--background-secondary",
            "--backgroundsecondary",
            "--background-base-low",
            "--panel-bg",
        ],
        light,
        &["--background-secondary-alt", "--backgroundsecondaryalt"],
        ground.mix(wash, 0.03),
    );
    let line = pick(
        vars,
        &[
            "--border-subtle",
            "--border-normal",
            "--border-muted",
            "--border-strong",
            "--background-modifier-accent",
            "--app-frame-border",
        ],
        light,
        &["--border", "--background-modifier-strong"],
        text.mix(ground, 0.76),
    );
    let hover = over(
        vars,
        &[
            "--background-modifier-hover",
            "--interactive-background-hover",
            "--message-background-hover",
            "--background-message-hover",
            "--background-modifier-selected",
        ],
        light,
        ground,
    )
    .unwrap_or_else(|| ground.mix(accent, 0.06));
    let active = over(
        vars,
        &[
            "--background-modifier-active",
            "--interactive-background-active",
            "--background-modifier-selected",
        ],
        light,
        ground,
    )
    .unwrap_or_else(|| ground.mix(accent, 0.12));
    let row_hover = over(
        vars,
        &[
            "--message-background-hover",
            "--background-message-hover",
            "--background-mentioned",
            "--background-mentioned-hover",
            "--background-modifier-hover",
        ],
        light,
        ground,
    )
    .unwrap_or_else(|| ground.mix(accent, 0.045));
    let danger = pick(
        vars,
        &[
            "--status-danger",
            "--control-critical-primary-background-default",
            "--red-400",
            "--danger",
            "--dangercolor",
            "--text-danger",
            "--icon-feedback-critical",
        ],
        light,
        &["--red-500"],
        Rgba::rgb(0xf2, 0x3f, 0x43),
    );
    let on_accent = vars
        .first_color(
            &["--control-primary-text-default", "--text-invert", "--white"],
            light,
        )
        .unwrap_or_else(|| accent.ink());
    let secondary = pick(
        vars,
        &[
            "--header-secondary",
            "--text-secondary",
            "--muted-text",
            "--text-muted",
            "--interactive-normal",
        ],
        light,
        &["--text-subtle"],
        text.mix(ground, 0.35),
    );
    let muted = pick(
        vars,
        &[
            "--muted-text",
            "--text-muted",
            "--text-faint",
            "--interactive-normal",
            "--channels-default",
        ],
        light,
        &["--interactive-muted"],
        text.mix(ground, 0.52),
    );
    let faint = pick(
        vars,
        &[
            "--text-subtle",
            "--muted-text",
            "--text-faint",
            "--interactive-muted",
            "--channel-icon",
        ],
        light,
        &[],
        text.mix(ground, 0.65),
    );
    let live = pick(
        vars,
        &[
            "--status-positive",
            "--green-360",
            "--online",
            "--successcolor",
            "--text-positive",
        ],
        light,
        &["--status-green"],
        Rgba::rgb(0x23, 0xa5, 0x5a),
    );
    let loud = pick(
        vars,
        &[
            "--status-warning",
            "--yellow-300",
            "--idle",
            "--warningcolor",
            "--text-warning",
        ],
        light,
        &["--status-yellow"],
        Rgba::rgb(0xf0, 0xb2, 0x32),
    );
    let link = pick(
        vars,
        &["--text-link", "--link", "--brand-360", "--linkcolor"],
        light,
        &["--text-brand"],
        accent,
    );
    let button = pick(
        vars,
        &[
            "--control-secondary-background-default",
            "--button-secondary-background",
            "--background-modifier-selected",
        ],
        light,
        &["--background-secondary-alt"],
        raised,
    );
    let button_hover = pick(
        vars,
        &[
            "--control-secondary-background-hover",
            "--button-secondary-background-hover",
        ],
        light,
        &[],
        button.mix(wash, 0.08),
    );
    let danger_hover = pick(
        vars,
        &["--control-critical-primary-background-hover", "--red-500"],
        light,
        &[],
        danger.mix(wash, 0.12),
    );
    let on_danger = vars
        .first_color(&["--control-critical-primary-text-default"], light)
        .unwrap_or_else(|| danger.ink());
    let tooltip = pick(
        vars,
        &["--background-tooltip", "--background-floating"],
        light,
        &[],
        raised,
    );
    let overlay = if dark {
        Rgba::rgb(255, 255, 255)
    } else {
        Rgba::rgb(0, 0, 0)
    };

    // ReCord's own palette, when the file is ReCord: it has a more precise
    // mapping for every part than the generic Discord tokens above.
    if vars.contains("--rl-ink") {
        return record_palette(vars, dark);
    }

    Palette {
        dark,
        rail: pick(
            vars,
            &[
                "--background-base-lowest",
                "--background-tertiary",
                "--app-frame-background",
            ],
            light,
            &["--background-secondary-alt"],
            sunken,
        ),
        sidebar,
        panel: sidebar,
        chat: ground,
        stage: ground,
        raised,
        field,
        sunken,
        hover,
        active,
        row_hover,
        line,
        edge: pick(
            vars,
            &["--app-frame-border", "--border-strong"],
            light,
            &[],
            line,
        ),
        outline: line,
        outline_chat: line,
        outline_field: line,
        text,
        secondary,
        muted,
        faint,
        accent,
        accent_hover: pick(
            vars,
            &[
                "--brand-560",
                "--control-primary-background-hover",
                "--brand-530",
            ],
            light,
            &[],
            accent.mix(wash, if dark { 0.12 } else { 0.18 }),
        ),
        on_accent,
        accent_soft: accent.with_alpha(0x26),
        danger,
        danger_hover,
        on_danger,
        live,
        loud,
        link,
        button_default: button,
        button_default_hover: button_hover,
        overlay,
        shimmer: if dark {
            Rgba::rgba(255, 255, 255, 0x10)
        } else {
            Rgba::rgba(255, 255, 255, 0x80)
        },
        reply_spine: pick(vars, &["--spine-default"], light, &[], line),
        tooltip,
        avatar: pick(
            vars,
            &["--avatar-background", "--background-modifier-accent"],
            light,
            &[],
            muted,
        ),
    }
}

/// ReCord's `--rl-*` tokens, which name every part it paints.
fn record_palette<V: Variables + ?Sized>(vars: &V, dark: bool) -> Palette {
    let light = !dark;
    let color = |name: &str, fallback: Rgba| vars.first_color(&[name], light).unwrap_or(fallback);
    let ink = color(
        "--rl-ink",
        if dark {
            Rgba::rgb(0x08, 0x09, 0x07)
        } else {
            Rgba::rgb(0xf3, 0xe9, 0xd7)
        },
    );
    let panel = color(
        "--rl-panel",
        if dark {
            Rgba::rgb(0x0e, 0x10, 0x0c)
        } else {
            Rgba::rgb(0xe4, 0xd6, 0xbd)
        },
    );
    let void = color(
        "--rl-void",
        if dark {
            Rgba::rgb(0x05, 0x06, 0x05)
        } else {
            Rgba::rgb(0xef, 0xe4, 0xd0)
        },
    );
    let raised = color(
        "--rl-raised",
        if dark {
            Rgba::rgb(0x13, 0x15, 0x11)
        } else {
            Rgba::rgb(0xdc, 0xcd, 0xb2)
        },
    );
    let text = color(
        "--rl-bone",
        if dark {
            Rgba::rgb(0xe4, 0xcd, 0xb1)
        } else {
            Rgba::rgb(0x2a, 0x1d, 0x14)
        },
    );
    let high = color("--rl-high", raised.mix(text, 0.12));
    let line = color(
        "--rl-line",
        if dark {
            Rgba::rgb(0x2c, 0x24, 0x1d)
        } else {
            Rgba::rgb(0xb7, 0xa3, 0x83)
        },
    );
    let secondary = color("--rl-smoke", text.mix(ink, 0.38));
    let muted = color("--rl-ash", text.mix(ink, 0.5));
    let faint = color("--rl-faint", text.mix(ink, 0.62));
    let accent = color("--rl-signal", Rgba::rgb(0xe9, 0x42, 0x21));
    let accent_hover = color("--rl-signal-hot", accent.mix(color("--rl-tint", text), 0.2));
    let danger = color("--rl-red", Rgba::rgb(0xff, 0x46, 0x26));
    let live = color("--rl-green", Rgba::rgb(0x79, 0xb5, 0x9a));
    let loud = color("--rl-amber", Rgba::rgb(0xdd, 0xa0, 0x52));
    let hover = over(
        vars,
        &[
            "--interactive-background-hover",
            "--background-modifier-hover",
        ],
        light,
        ink,
    )
    .unwrap_or_else(|| ink.mix(accent, 0.065));
    let active = over(
        vars,
        &[
            "--interactive-background-active",
            "--background-modifier-active",
        ],
        light,
        ink,
    )
    .unwrap_or_else(|| ink.mix(accent, 0.12));
    let row_hover = over(
        vars,
        &["--message-background-hover", "--background-modifier-hover"],
        light,
        ink,
    )
    .unwrap_or_else(|| ink.mix(accent, 0.045));
    Palette {
        dark,
        rail: ink,
        sidebar: panel,
        panel,
        chat: ink,
        stage: ink,
        raised,
        field: void,
        sunken: void,
        hover,
        active,
        row_hover,
        line,
        edge: void,
        outline: line,
        outline_chat: line,
        outline_field: line,
        text,
        secondary,
        muted,
        faint,
        accent,
        accent_hover,
        on_accent: void,
        accent_soft: accent.with_alpha(0x29),
        danger,
        danger_hover: color(
            "--control-critical-primary-background-hover",
            danger.mix(
                if dark {
                    Rgba::rgb(255, 255, 255)
                } else {
                    Rgba::rgb(0, 0, 0)
                },
                0.12,
            ),
        ),
        on_danger: void,
        live,
        loud,
        link: color(
            "--rl-signal-soft",
            accent.mix(color("--rl-tint", text), 0.42),
        ),
        button_default: raised,
        button_default_hover: high,
        overlay: if dark {
            Rgba::rgb(255, 255, 255)
        } else {
            Rgba::rgb(0, 0, 0)
        },
        shimmer: if dark {
            Rgba::rgba(255, 255, 255, 0x10)
        } else {
            Rgba::rgba(255, 255, 255, 0x80)
        },
        reply_spine: line,
        tooltip: high,
        avatar: muted,
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the look is assembled from the palette, pictures, fonts and animation in one place"
)]
/// The per-section opacities a theme names, so a wallpaper can show through
/// one region more than another as the native theme packs allow. A selector
/// that already set a region's `opacity` wins over the root variable.
fn region_opacities(native: &mut crate::NativeStyle, vars: &css::Vars) {
    for (names, slot) in [
        (&["--rl-rail-opacity"][..], &mut native.rail_opacity),
        (&["--rl-sidebar-opacity"][..], &mut native.sidebar_opacity),
        (&["--rl-chat-opacity"][..], &mut native.chat_opacity),
        (&["--rl-members-opacity"][..], &mut native.panel_opacity),
        (&["--rl-composer-opacity"][..], &mut native.field_opacity),
        (&["--rl-raised-opacity"][..], &mut native.raised_opacity),
        (&["--rl-tooltip-opacity"][..], &mut native.tooltip_opacity),
    ] {
        if slot.is_none()
            && let Some(value) = fraction(vars, names)
        {
            *slot = Some(value);
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the parsed theme's separate parts are gathered here once"
)]
fn look(
    vars: &css::Vars,
    palette: Palette,
    directory: Option<&Path>,
    fonts: &[Font],
    options: &Options,
    animations: Vec<Animation>,
    direct_background: Option<Image>,
    warnings: &mut Vec<String>,
) -> Look {
    let font_family = vars
        .get("--font-primary")
        .or_else(|| vars.get("--rl-font"))
        .or_else(|| vars.get("--font"))
        .and_then(|value| vars.resolve(value))
        .and_then(|value| first_family(value.as_ref()));
    if let Some(family) = &font_family
        && options.fonts
        && !fonts
            .iter()
            .any(|font| font.family.eq_ignore_ascii_case(family))
        && !system_family(family)
    {
        warnings.push(format!(
            "the theme names the font {family}, which it does not carry; the system one is used if it has it"
        ));
    }
    let motion = match vars
        .resolve(vars.get("--rl-motion").unwrap_or("full"))
        .unwrap_or_else(|| "full".into())
        .to_ascii_lowercase()
        .as_str()
    {
        "off" => Motion::Off,
        "subtle" => Motion::Subtle,
        _ => Motion::Full,
    };
    let mut effects = Effects {
        scanlines: fraction(vars, &["--rl-scanline-opacity"]).unwrap_or(0.0),
        grain: fraction(vars, &["--rl-grain-opacity"]).unwrap_or(0.0),
        vignette: fraction(vars, &["--rl-vignette-opacity"]).unwrap_or(0.0),
        flicker: duration(vars, "--rl-flicker-duration").is_some_and(|seconds| seconds > 0.0),
        beam: fraction(vars, &["--rl-beam-opacity"]).is_some_and(|value| value > 0.0),
        frame: fraction(vars, &["--rl-frame-opacity"]).unwrap_or(0.0),
        avatar_filter: vars
            .resolve(vars.get("--rl-avatar-filter").unwrap_or("none"))
            .map(|value| value.into_owned())
            .filter(|value| !value.eq_ignore_ascii_case("none")),
        avatar_scanline: fraction(vars, &["--rl-avatar-scanline"]).unwrap_or(0.0),
    };
    // ReCord's low power drops the moving and static noise; scanlines and the
    // vignette stay, as they do in the theme.
    if vars
        .resolve(vars.get("--rl-lite").unwrap_or("off"))
        .is_some_and(|value| value.eq_ignore_ascii_case("on"))
    {
        effects.grain = 0.0;
        effects.beam = false;
        effects.flicker = false;
    }
    Look {
        font_family,
        font_size: length(vars, &["--font-size", "--text-size"]).and_then(|size| {
            if size > 0.0 {
                crate::css::bounds::within(size, crate::css::bounds::TEXT)
            } else {
                None
            }
        }),
        letter_spacing: length(vars, &["--rl-tracking", "--letter-spacing"])
            .and_then(|spacing| crate::css::bounds::within(spacing, crate::css::bounds::TRACKING)),
        radius: length(vars, &["--rl-radius", "--radius-md", "--radius-sm"])
            .and_then(|radius| crate::css::bounds::within(radius, crate::css::bounds::RADIUS)),
        frame: vars.first_color(
            &[
                "--app-frame-background",
                "--background-primary",
                "--background-base-lower",
            ],
            !palette.dark,
        ),
        panel_gap: length(vars, &["--gap", "--spacing", "--rl-gap"])
            .and_then(|gap| crate::css::bounds::within(gap, crate::css::bounds::GAP)),
        panel_radius: length(
            vars,
            &[
                "--roundness",
                "--roundness-l",
                "--radius-lg",
                "--radius-md",
                "--rl-radius",
            ],
        )
        .and_then(|radius| crate::css::bounds::within(radius, crate::css::bounds::RADIUS)),
        motion,
        pixel_icons: vars
            .resolve(vars.get("--rl-icons").unwrap_or("off"))
            .is_some_and(|value| value.eq_ignore_ascii_case("on")),
        effects,
        background: image(
            vars,
            directory,
            &[
                "--rl-background-image",
                "--background-image",
                "--background",
                "--recore-background",
            ],
        )
        .or(direct_background),
        background_dim: fraction(vars, &["--rl-background-dim"]),
        panel_opacity: fraction(vars, &["--rl-panel-opacity", "--rl-surface"]),
        background_fit: background_fit(vars),
        accent: Some(palette.accent),
        animations,
    }
}

/// Reads the animations that write the window's root or body. A theme's
/// animated pseudo-elements and DOM nodes still have no native target, but a
/// moving ground, glow or pulse is something the window can paint.
fn animations(
    sheet: &Stylesheet,
    _collected: &Collected,
    dark: bool,
    vars: &css::Vars,
) -> Vec<Animation> {
    /// An animation's name as a theme writes it, without the quotes CSS allows
    /// around an identifier: `@keyframes "wash"` and `animation: wash` name the
    /// same animation.
    fn animation_name(name: &str) -> String {
        name.trim().trim_matches(['"', '\'']).to_owned()
    }

    /// What one rule says about one animation name, before the keyframes are
    /// turned into frames.
    #[derive(Clone, Default)]
    struct Timing {
        duration: Option<f32>,
        delay: f32,
        iterations: Option<f32>,
        direction: AnimationDirection,
        /// Whether the theme named a count, including `infinite`.
        count_seen: bool,
    }
    const TIMING_PROPERTIES: &[&str] = &[
        "animation-name",
        "animation",
        "animation-duration",
        "animation-delay",
        "animation-iteration-count",
        "animation-direction",
    ];
    let mut timings: BTreeMap<String, Timing> = BTreeMap::new();
    for rule in &sheet.rules {
        if rule
            .color_scheme
            .is_some_and(|color_scheme| color_scheme != dark)
        {
            continue;
        }
        let root = rule.selectors.iter().any(|selector| {
            let selector = selector.split("::").next().unwrap_or(selector);
            crate::css::root_selector(selector, dark).is_some()
        });
        if !root {
            continue;
        }
        let mut names = Vec::new();
        let mut shorthand = Vec::new();
        for declaration in &rule.declarations {
            if !TIMING_PROPERTIES
                .iter()
                .any(|name| declaration.name.trim().eq_ignore_ascii_case(name))
            {
                continue;
            }
            let Some(value) = vars.resolve(&declaration.value) else {
                continue;
            };
            match declaration.name.trim().to_ascii_lowercase().as_str() {
                "animation-name" => {
                    names.extend(
                        crate::color::split_top(&value, ',')
                            .into_iter()
                            .map(animation_name),
                    );
                }
                "animation" => {
                    let lower = value.to_ascii_lowercase();
                    if lower.trim() == "none" {
                        continue;
                    }
                    shorthand.extend(
                        value
                            .split_whitespace()
                            .map(animation_name)
                            .filter(|token| sheet.keyframes.contains_key(token)),
                    );
                }
                _ => {}
            }
        }
        names.retain(|name| sheet.keyframes.contains_key(name));
        if !shorthand.is_empty() {
            names.extend(shorthand.iter().cloned());
        }
        for (index, name) in names.iter().enumerate() {
            let timing = timings.entry(name.clone()).or_default();
            let mut duration_seen = 0;
            let mut delay_seen = 0;
            for declaration in &rule.declarations {
                if !TIMING_PROPERTIES
                    .iter()
                    .any(|name| declaration.name.trim().eq_ignore_ascii_case(name))
                {
                    continue;
                }
                let Some(value) = vars.resolve(&declaration.value) else {
                    continue;
                };
                let value = value.trim();
                match declaration.name.trim().to_ascii_lowercase().as_str() {
                    "animation-duration" => {
                        if let Some(part) = nth_split_top(value, ',', index) {
                            timing.duration = seconds(part);
                        }
                    }
                    "animation-delay" => {
                        if let Some(part) = nth_split_top(value, ',', index) {
                            timing.delay = seconds(part).unwrap_or(0.0);
                        }
                    }
                    "animation-iteration-count" => {
                        if let Some(part) = nth_split_top(value, ',', index) {
                            timing.iterations = iterations(part);
                            timing.count_seen = true;
                        }
                    }
                    "animation-direction" => {
                        if let Some(part) = nth_split_top(value, ',', index) {
                            timing.direction = direction(part);
                        }
                    }
                    "animation" => {
                        // The shorthand carries its own duration, delay,
                        // iteration count and direction.
                        for token in value.split_whitespace() {
                            if let Some(seconds) = seconds(token) {
                                if duration_seen == 0 {
                                    timing.duration = Some(seconds);
                                    duration_seen += 1;
                                } else if delay_seen == 0 {
                                    timing.delay = seconds;
                                    delay_seen += 1;
                                }
                                continue;
                            }
                            if token.eq_ignore_ascii_case("infinite") {
                                timing.iterations = None;
                                timing.count_seen = true;
                            } else if let Some(count) = iterations(token) {
                                timing.iterations = Some(count);
                                timing.count_seen = true;
                            }
                            if matches!(
                                token.to_ascii_lowercase().as_str(),
                                "normal" | "reverse" | "alternate" | "alternate-reverse"
                            ) {
                                timing.direction = direction(token);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let mut found = Vec::new();
    for (name, timing) in timings {
        let Some(keyframes) = sheet.keyframes.get(&name) else {
            continue;
        };
        let mut frames = Vec::new();
        for frame in &keyframes.frames {
            let mut painted = AnimationFrame {
                offset: frame.offset,
                ..AnimationFrame::default()
            };
            for declaration in &frame.declarations {
                let Some(value) = vars.resolve(&declaration.value) else {
                    continue;
                };
                match declaration.name.trim().to_ascii_lowercase().as_str() {
                    "background" | "background-color" => {
                        painted.background = crate::color::parse(value.trim(), !dark);
                    }
                    "color" => {
                        painted.color = crate::color::parse(value.trim(), !dark);
                    }
                    name if name.starts_with("--")
                        && sheet
                            .properties
                            .get(name)
                            .and_then(|spec| spec.syntax.as_deref())
                            .is_some_and(|syntax| {
                                syntax.to_ascii_lowercase().contains("color")
                            }) =>
                    {
                        // A registered custom property can animate a theme's
                        // accent or ground through `@property`; use its
                        // value for the moving wash too.
                        painted.color = crate::color::parse(value.trim(), !dark);
                    }
                    "opacity" => {
                        painted.opacity = value
                            .trim()
                            .strip_suffix('%')
                            .and_then(|value| value.trim().parse::<f32>().ok())
                            .map(|value| (value / 100.0).clamp(0.0, 1.0))
                            .or_else(|| {
                                value
                                    .trim()
                                    .parse::<f32>()
                                    .ok()
                                    .map(|value| value.clamp(0.0, 1.0))
                            });
                    }
                    _ => {}
                }
            }
            if painted.background.is_some() || painted.color.is_some() || painted.opacity.is_some()
            {
                frames.push(painted);
            }
        }
        if frames.len() < 2 {
            continue;
        }
        // CSS's defaults are 0 s and one iteration. A name alone is not a
        // moving wash; only a duration the theme named makes one.
        let Some(duration) = timing.duration.filter(|duration| *duration > 0.0) else {
            continue;
        };
        found.push(Animation {
            name,
            duration: duration.max(0.05),
            delay: timing.delay.max(0.0),
            iterations: if timing.count_seen {
                timing.iterations
            } else {
                Some(1.0)
            },
            direction: timing.direction,
            frames,
        });
    }
    found
}

/// One part of a comma-separated `animation-*` value.
fn nth_split_top(value: &str, delimiter: char, index: usize) -> Option<&str> {
    crate::color::split_top(value, delimiter)
        .get(index)
        .copied()
}

/// A CSS time in seconds, including `ms`.
fn seconds(value: &str) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    if let Some(value) = value.strip_suffix("ms") {
        return value.trim().parse::<f32>().ok().map(|value| value / 1000.0);
    }
    value
        .strip_suffix('s')
        .and_then(|value| value.trim().parse::<f32>().ok())
}

fn iterations(value: &str) -> Option<f32> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("infinite") {
        None
    } else {
        value.parse::<f32>().ok().filter(|count| *count >= 0.0)
    }
}

fn direction(value: &str) -> AnimationDirection {
    match value.trim().to_ascii_lowercase().as_str() {
        "reverse" => AnimationDirection::Reverse,
        "alternate" => AnimationDirection::Alternate,
        "alternate-reverse" => AnimationDirection::AlternateReverse,
        _ => AnimationDirection::Normal,
    }
}

/// How a theme names the background's fit, from `background-size`.
fn background_fit(vars: &css::Vars) -> BackgroundFit {
    let value = vars
        .get("--rl-background-size")
        .or_else(|| vars.get("--background-size"))
        .and_then(|value| vars.resolve(value))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if value.contains("contain") {
        BackgroundFit::Contain
    } else if value.contains("100% 100%") || value == "fill" {
        BackgroundFit::Fill
    } else {
        BackgroundFit::Cover
    }
}

/// The first concrete family in a CSS `font-family` list.
fn first_family(value: &str) -> Option<String> {
    for family in color::split_top(value, ',') {
        let family = family.trim().trim_matches(['"', '\'']).trim();
        if family.is_empty() {
            continue;
        }
        let lower = family.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "sans-serif"
                | "serif"
                | "monospace"
                | "cursive"
                | "fantasy"
                | "system-ui"
                | "ui-sans-serif"
                | "ui-monospace"
                | "ui-serif"
                | "ui-rounded"
                | "emoji"
                | "math"
                | "fangsong"
                | "inherit"
                | "initial"
                | "unset"
        ) {
            continue;
        }
        return Some(family.to_owned());
    }
    None
}

fn system_family(family: &str) -> bool {
    matches!(
        family.to_ascii_lowercase().as_str(),
        "courier new"
            | "consolas"
            | "monospace"
            | "system-ui"
            | "roboto"
            | "roboto condensed"
            | "noto sans"
            | "inter"
            | "arial"
            | "helvetica"
            | "segoe ui"
            | "ubuntu"
            | "cantarell"
            | "dejavu sans"
    )
}

/// A cheap fingerprint of the theme files in `directories`: their paths,
/// sizes and modification times. The window uses it to reload when a theme
/// file is saved outside ReCore.
pub fn theme_files_fingerprint(directories: &[PathBuf]) -> u64 {
    let mut files = Vec::new();
    for directory in directories {
        let _ = collect_css_files(directory, 0, &mut files);
    }
    files.sort();
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for path in files {
        let canonical = fs::canonicalize(&path).unwrap_or(path);
        let meta = fs::metadata(&canonical).ok();
        let len = meta.as_ref().map_or(0, fs::Metadata::len);
        let modified = meta
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |time| time.as_nanos());
        for byte in canonical.to_string_lossy().as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= len;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        hash ^= modified as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Reads every `.css` theme in `directories`, in name order, skipping the ones
/// already read and the ones that fail (kept in [`Scan::problems`]).
///
/// Reading the local files is one pass; parsing them is spread over up to
/// four threads, since a folder of large themes is the window's first frame.
pub fn scan(directories: &[PathBuf]) -> Scan {
    let mut scan = Scan::default();
    let mut seen = BTreeSet::new();
    let mut seen_content = BTreeSet::new();
    let mut files = Vec::new();
    for directory in directories {
        if !directory.exists() {
            continue;
        }
        let mut found = Vec::new();
        if let Err(error) = collect_css_files(directory, 0, &mut found) {
            scan.problems.push(Problem {
                path: directory.clone(),
                message: format!("could not read the themes folder: {error}"),
            });
            continue;
        }
        found.sort();
        for path in found {
            let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if !seen.insert(canonical) {
                continue;
            }
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    let message = format!("could not read {}: {error}", path.display());
                    scan.problems.push(Problem { path, message });
                    continue;
                }
            };
            let content = fingerprint(&bytes);
            if !seen_content.insert(content) {
                continue;
            }
            match String::from_utf8(bytes) {
                Ok(css) => files.push((path, css, content)),
                Err(_) => scan.problems.push(Problem {
                    path,
                    message: "the theme is not UTF-8 CSS".into(),
                }),
            }
        }
    }
    let mut ids = BTreeSet::new();
    for (path, result) in parse_all(files) {
        match result {
            Ok(mut theme) => {
                let base = theme.id.clone();
                let mut number = 2;
                while !ids.insert(theme.id.clone()) {
                    theme.id = format!("{base}-{number}");
                    number += 1;
                }
                scan.themes.push(theme);
            }
            Err(message) => scan.problems.push(Problem { path, message }),
        }
    }
    scan
}

/// Parses the files' CSS in parallel, returning the results in file order.
fn parse_all(files: Vec<(PathBuf, String, u64)>) -> Vec<(PathBuf, Result<ThemeFile, String>)> {
    let count = files.len();
    // Two or three small files parse before the threads have started; a
    // folder of themes is where the parallel pass pays.
    if count < 4 {
        return files
            .into_iter()
            .map(|(path, css, fingerprint)| {
                let result = parse_one(&path, &css, fingerprint);
                (path, result)
            })
            .collect();
    }
    let threads = std::thread::available_parallelism().map_or(1, |cores| cores.get().min(4));
    let next = AtomicUsize::new(0);
    let (send, receive) = mpsc::channel();
    std::thread::scope(|scope| {
        for _ in 0..threads.min(count) {
            let send = send.clone();
            let files = &files;
            let next = &next;
            scope.spawn(move || {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some((path, css, fingerprint)) = files.get(index) else {
                        break;
                    };
                    let result = parse_one(path, css, *fingerprint);
                    if send.send((index, path.clone(), result)).is_err() {
                        break;
                    }
                }
            });
        }
    });
    drop(send);
    let mut parsed: Vec<_> = receive.into_iter().collect();
    parsed.sort_by_key(|(index, ..)| *index);
    parsed
        .into_iter()
        .map(|(_, path, result)| (path, result))
        .collect()
}

/// A parsed theme from before, while its file has the same bytes. Starting
/// the window reads the themes once for the chosen font and again for the
/// window; this keeps the second read to a clone.
fn parse_one(path: &Path, css: &str, fingerprint: u64) -> Result<ThemeFile, String> {
    let key = (path.to_path_buf(), fingerprint);
    if let Some(theme) = cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key)
    {
        return Ok(theme.clone());
    }
    let theme = named(load_str(css, path.parent(), &Options::default())?, path);
    let mut cache = cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.len() >= 32 {
        cache.clear();
    }
    cache.insert(key, theme.clone());
    Ok(theme)
}

fn cache() -> &'static Mutex<BTreeMap<(PathBuf, u64), ThemeFile>> {
    static CACHE: OnceLock<Mutex<BTreeMap<(PathBuf, u64), ThemeFile>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Finds `.css` files in the folder and in its own subfolders, which theme
/// packs often use. Hidden folders, dependency trees and build caches are
/// left alone.
fn collect_css_files(
    directory: &Path,
    depth: usize,
    files: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    if depth > 4 {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            if matches!(
                name.as_ref(),
                "node_modules" | "target" | ".git" | ".cache" | "vendor"
            ) {
                continue;
            }
            // A folder inside a theme pack may not be readable; keep the
            // files already found rather than failing the whole folder.
            let _ = collect_css_files(&path, depth + 1, files);
        } else if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("css"))
        {
            files.push(path);
        }
    }
    Ok(())
}

/// A cheap content fingerprint, so the same theme copied into Vesktop's and
/// ReCore's folders appears once.
fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^ (bytes.len() as u64)
}

/// [`scan`], downloading each theme's remote imports and fonts through
/// `fetch`.
pub fn scan_with_imports(
    directories: &[PathBuf],
    fetch: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
) -> Scan {
    let mut scan = Scan::default();
    let mut seen = BTreeSet::new();
    let mut seen_content = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for directory in directories {
        if !directory.exists() {
            // Most people don't have every client installed; a folder that
            // isn't there is not a theme that failed to read.
            continue;
        }
        let mut files = Vec::new();
        if let Err(error) = collect_css_files(directory, 0, &mut files) {
            scan.problems.push(Problem {
                path: directory.clone(),
                message: format!("could not read the themes folder: {error}"),
            });
            continue;
        }
        files.sort();
        for path in files {
            let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if !seen.insert(canonical) {
                continue;
            }
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    let message = format!("could not read {}: {error}", path.display());
                    scan.problems.push(Problem { path, message });
                    continue;
                }
            };
            if !seen_content.insert(fingerprint(&bytes)) {
                continue;
            }
            let css = match String::from_utf8(bytes) {
                Ok(css) => css,
                Err(_) => {
                    scan.problems.push(Problem {
                        path,
                        message: "the theme is not UTF-8 CSS".into(),
                    });
                    continue;
                }
            };
            match load_css_with_imports(&path, &css, &Options::default(), fetch) {
                Ok(mut theme) => {
                    let base = theme.id.clone();
                    let mut number = 2;
                    while !ids.insert(theme.id.clone()) {
                        theme.id = format!("{base}-{number}");
                        number += 1;
                    }
                    scan.themes.push(theme);
                }
                Err(message) => scan.problems.push(Problem { path, message }),
            }
        }
    }
    scan
}

/// The folders Vencord, Vesktop, Equicord, BetterDiscord, Legcord and ReCord
/// keep local themes in, when this desktop has them.
pub fn theme_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(config) = config_directory() {
        for name in [
            "Vencord",
            "vesktop",
            "Equicord",
            "Equibop",
            "BetterDiscord",
            "Legcord",
            "Dorion",
            "GoofCord",
            "replugged",
        ] {
            folders.push(config.join(name).join("themes"));
        }
    }
    folders
}

fn config_directory() -> Option<PathBuf> {
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config));
    }
    if cfg!(windows)
        && let Some(appdata) = std::env::var_os("APPDATA")
    {
        return Some(PathBuf::from(appdata));
    }
    if cfg!(target_os = "macos")
        && let Some(home) = std::env::var_os("HOME")
    {
        return Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support"),
        );
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
}

fn slug(text: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "theme".into()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD: &str = r#"
/** @name ReCord @author leriart @description Terminal */
:root {
    --rl-accent: #e94221;
    --rl-signal: var(--rl-accent);
    --rl-signal-hot: color-mix(in srgb, var(--rl-accent) 80%, #fff3d6);
    --rl-signal-soft: color-mix(in srgb, var(--rl-accent) 58%, #fff3d6);
    --rl-void: #050605;
    --rl-ink: #080907;
    --rl-panel: #0e100c;
    --rl-raised: #131511;
    --rl-high: #191c16;
    --rl-line: #2c241d;
    --rl-bone: #e4cdb1;
    --rl-smoke: #a58979;
    --rl-ash: #8c7b6c;
    --rl-faint: #6f5d4f;
    --rl-green: #79b59a;
    --rl-amber: #dda052;
    --rl-red: #ff4626;
    --rl-motion: subtle;
    --rl-icons: on;
    --background-primary: var(--rl-ink);
    --background-secondary: var(--rl-panel);
    --text-normal: var(--rl-bone);
    --font-primary: "Pixel Operator", monospace;
}
html.theme-light:not([data-rl-mode="dark"]) {
    --rl-void: #efe4d0;
    --rl-ink: #f3e9d7;
    --rl-panel: #e4d6bd;
    --rl-raised: #dccdb2;
    --rl-line: #b7a383;
    --rl-bone: #2a1d14;
    --rl-smoke: #57422f;
    --rl-ash: #6b5644;
    --rl-faint: #8a7560;
}
"#;

    #[test]
    fn record_css_gives_record_colours() {
        let theme = load_str(RECORD, None, &Options::with_mode(Mode::Dark)).unwrap();
        assert_eq!(theme.name, "ReCord");
        assert_eq!(theme.author.as_deref(), Some("leriart"));
        assert_eq!(theme.palette.rail, Rgba::rgb(0x08, 0x09, 0x07));
        assert_eq!(theme.palette.sidebar, Rgba::rgb(0x0e, 0x10, 0x0c));
        assert_eq!(theme.palette.chat, Rgba::rgb(0x08, 0x09, 0x07));
        assert_eq!(theme.palette.accent, Rgba::rgb(0xe9, 0x42, 0x21));
        assert_eq!(theme.palette.accent_hover, Rgba::rgb(0xed, 0x65, 0x45));
        assert_eq!(theme.palette.text, Rgba::rgb(0xe4, 0xcd, 0xb1));
        assert_eq!(theme.look.font_family.as_deref(), Some("Pixel Operator"));
        assert_eq!(theme.look.motion, Motion::Subtle);
        assert!(theme.look.pixel_icons);
        assert!(theme.palette.dark);
    }

    #[test]
    fn the_light_selector_gives_the_paper_palette() {
        let theme = load_str(RECORD, None, &Options::with_mode(Mode::Light)).unwrap();
        assert_eq!(theme.palette.chat, Rgba::rgb(0xf3, 0xe9, 0xd7));
        assert_eq!(theme.palette.text, Rgba::rgb(0x2a, 0x1d, 0x14));
        assert!(!theme.palette.dark);
    }

    #[test]
    fn a_common_discord_theme_maps_its_tokens() {
        let css = r#"
        :root {
            --background-primary: #313338;
            --background-secondary: #2b2d31;
            --background-secondary-alt: #1e1f22;
            --background-floating: #111214;
            --channeltextarea-background: #383a40;
            --text-normal: #dbdee1;
            --text-muted: #949ba4;
            --header-primary: #f2f3f5;
            --header-secondary: #b5bac1;
            --brand-experiment: #5865f2;
            --brand-560: #4752c4;
            --status-positive: #23a55a;
            --status-danger: #f23f43;
            --text-link: #00a8fc;
            --border-subtle: #3f4147;
            --font-primary: "gg sans", "Noto Sans", sans-serif;
        }
        .theme-light {
            --background-primary: #ffffff;
            --background-secondary: #f2f3f5;
            --text-normal: #313338;
        }
        "#;
        let theme = load_str(css, None, &Options::with_mode(Mode::Dark)).unwrap();
        assert_eq!(theme.palette.chat, Rgba::rgb(0x31, 0x33, 0x38));
        assert_eq!(theme.palette.sidebar, Rgba::rgb(0x2b, 0x2d, 0x31));
        assert_eq!(theme.palette.accent, Rgba::rgb(0x58, 0x65, 0xf2));
        assert_eq!(theme.palette.accent_hover, Rgba::rgb(0x47, 0x52, 0xc4));
        assert_eq!(theme.palette.text, Rgba::rgb(0xdb, 0xde, 0xe1));
        assert_eq!(theme.look.font_family.as_deref(), Some("gg sans"));
        let light = load_str(css, None, &Options::with_mode(Mode::Light)).unwrap();
        assert_eq!(light.palette.chat, Rgba::rgb(0xff, 0xff, 0xff));
        assert_eq!(light.palette.text, Rgba::rgb(0x31, 0x33, 0x38));
    }

    #[test]
    fn cascade_layers_beat_specificity_and_unlayered_wins_over_all() {
        let css = r#"
        @layer base {
            .theme-dark:root { --background-primary: #111111; --background-secondary: #444444; }
        }
        @layer theme {
            :root { --background-primary: #222222; }
        }
        :root { --background-secondary: #333333; }
        "#;
        let theme = load_str(css, None, &Options::with_mode(Mode::Dark)).unwrap();
        assert_eq!(theme.palette.chat, Rgba::rgb(0x22, 0x22, 0x22));
        assert_eq!(theme.palette.sidebar, Rgba::rgb(0x33, 0x33, 0x33));
    }

    #[test]
    fn a_diagnostic_says_what_was_left_out_and_where() {
        let css = "\n:root { colour: #ff0000; color: #eeeeee; }\n[class*=\"sidebar_\"] { background: not-a-colour; }\n";
        let options = Options {
            diagnostics: true,
            ..Options::with_mode(Mode::Dark)
        };
        let theme = load_str(css, None, &options).expect("readable");
        let dropped: Vec<&Diagnostic> = theme
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Dropped)
            .collect();
        assert!(
            dropped
                .iter()
                .any(|diagnostic| diagnostic.line == 2 && diagnostic.message.contains("colour")),
            "the unknown property, on its line: {:?}",
            theme.diagnostics
        );
        assert!(
            dropped.iter().any(|diagnostic| diagnostic.line == 3
                && diagnostic.message.to_lowercase().contains("background")),
            "the bad value, on its line: {:?}",
            theme.diagnostics
        );
        assert!(
            theme
                .diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.message.contains("invalid selector ':is(")),
            "the engine's own wrapper is not a fault of the theme"
        );
        // Reading without asking keeps the collection empty.
        let plain = load_str(css, None, &Options::with_mode(Mode::Dark)).expect("readable");
        assert!(plain.diagnostics.is_empty());
    }

    #[test]
    fn a_quoted_animation_name_finds_its_keyframes() {
        let theme = load_str(
            r#"
            @keyframes "wash" { from { background: #101014; } to { background: #e94221; } }
            body { animation: "wash" 2s infinite; }
            "#,
            None,
            &Options::with_mode(Mode::Dark),
        )
        .expect("theme");
        assert_eq!(theme.look.animations.len(), 1);
        assert_eq!(theme.look.animations[0].name, "wash");
    }

    #[test]
    fn a_layer_statement_fixes_the_order_of_later_blocks() {
        // The statement names `a` before `b`, so `b` wins, even though the
        // block for `b` is written first. A browser and the engine both hold
        // to the statement; `style-engine` needs the names alone, as it skips
        // a list.
        let css = r#"
        @layer a, b;
        @layer b { :root { --background-primary: #111111; } }
        @layer a { :root { --background-primary: #222222; } }
        "#;
        let theme = load_str(css, None, &Options::with_mode(Mode::Dark)).unwrap();
        assert_eq!(theme.palette.chat, Rgba::rgb(0x11, 0x11, 0x11));

        // Without the statement the order is the blocks' own.
        let css = r#"
        @layer b { :root { --background-primary: #111111; } }
        @layer a { :root { --background-primary: #222222; } }
        "#;
        let theme = load_str(css, None, &Options::with_mode(Mode::Dark)).unwrap();
        assert_eq!(theme.palette.chat, Rgba::rgb(0x22, 0x22, 0x22));
    }

    #[test]
    fn auto_picks_dark_for_a_dark_ground_and_light_for_a_light_one() {
        let dark = load_str(
            ":root{--background-primary:#111;--text-normal:#eee}",
            None,
            &Options::default(),
        )
        .unwrap();
        assert!(dark.palette.dark);
        let light = load_str(
            ":root{--background-primary:#fff;--text-normal:#111}",
            None,
            &Options::default(),
        )
        .unwrap();
        assert!(!light.palette.dark);
    }

    #[test]
    fn option_overrides_reach_the_palette_and_the_controls() {
        let mut options = Options::with_mode(Mode::Dark);
        options.overrides = vec![
            ("--rl-accent".into(), "#3ddc97".into()),
            ("--rl-scanline-opacity".into(), "0.5".into()),
        ];
        let theme = load_str(RECORD, None, &options).unwrap();
        assert_eq!(theme.palette.accent, Rgba::rgb(0x3d, 0xdc, 0x97));
        assert_eq!(theme.look.effects.scanlines, 0.5);
        let accent = theme
            .options
            .iter()
            .find(|option| option.name == "--rl-accent")
            .unwrap();
        assert_eq!(accent.value, "#3ddc97");
        let scanlines = theme
            .options
            .iter()
            .find(|option| option.name == "--rl-scanline-opacity")
            .unwrap();
        assert_eq!(scanlines.value, "0.5");
        assert!(matches!(
            scanlines.kind,
            crate::OptionKind::Percent {
                value: 50.0,
                percent: false
            }
        ));
    }

    #[test]
    fn an_accent_override_recolors_the_palette() {
        let mut options = Options::with_mode(Mode::Dark);
        options.accent = Some(Rgba::rgb(0x3d, 0xdc, 0x97));
        let theme = load_str(RECORD, None, &options).unwrap();
        assert_eq!(theme.palette.accent, Rgba::rgb(0x3d, 0xdc, 0x97));
    }

    #[test]
    fn light_dark_follows_the_chosen_appearance() {
        let css = r#"
        :root { color-scheme: light dark;
            --background-primary: light-dark(#fff8f7, #1a1111);
            --text-normal: light-dark(#221a19, #f1dedd);
            --brand-experiment: #904a46; }
        .theme-light { color-scheme: light; }
        "#;
        let dark = load_str(css, None, &Options::with_mode(Mode::Dark)).unwrap();
        assert_eq!(dark.palette.chat, Rgba::rgb(0x1a, 0x11, 0x11));
        assert_eq!(dark.palette.text, Rgba::rgb(0xf1, 0xde, 0xdd));
        let light = load_str(css, None, &Options::with_mode(Mode::Light)).unwrap();
        assert_eq!(light.palette.chat, Rgba::rgb(0xff, 0xf8, 0xf7));
        assert_eq!(light.palette.text, Rgba::rgb(0x22, 0x1a, 0x19));
    }

    #[test]
    fn a_light_dark_scheme_follows_the_ground() {
        let theme = load_str(
            ":root { color-scheme: light dark; --background-primary: #1a1111; }",
            None,
            &Options::default(),
        )
        .unwrap();
        assert!(theme.palette.dark);
    }

    #[test]
    fn discord_recolors_triplet_tokens_become_colours() {
        let css = r#"
        :root {
            --backgroundprimary: 30, 30, 30;
            --backgroundsecondary: 20, 20, 20;
            --backgroundsecondaryalt: 15, 15, 15;
            --backgroundfloating: 0, 0, 0;
            --textbrightest: 255, 255, 255;
            --accentcolor: 255, 179, 174;
            --font: "Roboto Condensed", sans-serif;
        }
        "#;
        let theme = load_str(css, None, &Options::with_mode(Mode::Dark)).unwrap();
        assert_eq!(theme.palette.chat, Rgba::rgb(0x1e, 0x1e, 0x1e));
        assert_eq!(theme.palette.sidebar, Rgba::rgb(0x14, 0x14, 0x14));
        assert_eq!(theme.palette.rail, Rgba::rgb(0x0f, 0x0f, 0x0f));
        assert_eq!(theme.palette.raised, Rgba::rgb(0, 0, 0));
        assert_eq!(theme.palette.text, Rgba::rgb(255, 255, 255));
        assert_eq!(theme.palette.accent, Rgba::rgb(255, 179, 174));
        assert_eq!(theme.look.font_family.as_deref(), Some("Roboto Condensed"));
    }

    #[test]
    fn the_dark_rule_after_root_wins_and_root_after_dark_wins() {
        let after_root = load_str(
            ":root { --background-primary: #111111; } .theme-dark { --background-primary: #222222; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(after_root.palette.chat, Rgba::rgb(0x22, 0x22, 0x22));
        let after_dark = load_str(
            ".theme-dark { --background-primary: #222222; } :root { --background-primary: #111111; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(after_dark.palette.chat, Rgba::rgb(0x11, 0x11, 0x11));
    }

    #[test]
    fn an_important_rule_beats_higher_specificity() {
        let theme = load_str(
            ":root { --background-primary: #111111 !important; } \
             #app-mount { --background-primary: #222222; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.palette.chat, Rgba::rgb(0x11, 0x11, 0x11));
    }

    #[test]
    fn the_bundled_ambxst_theme_reads_as_its_palette() {
        let css = include_str!("../../../themes/Ambxst.theme.css");
        let dark = load_str(css, None, &Options::with_mode(Mode::Dark)).unwrap();
        assert_eq!(dark.name, "Ambxst");
        assert_eq!(dark.palette.chat, Rgba::rgb(0x1a, 0x11, 0x11));
        assert_eq!(dark.palette.sidebar, Rgba::rgb(0x23, 0x19, 0x19));
        assert_eq!(dark.palette.accent, Rgba::rgb(0xff, 0xb3, 0xae));
        assert_eq!(dark.palette.text, Rgba::rgb(0xf1, 0xde, 0xdd));
        assert_eq!(dark.look.radius, Some(16.0));
        assert_eq!(dark.look.font_family.as_deref(), Some("Roboto Condensed"));
        assert_eq!(dark.native.rail, Some(Rgba::rgb(0x14, 0x0c, 0x0c)));
        assert!(
            dark.look
                .animations
                .iter()
                .any(|animation| animation.name == "ambxst-drift"),
            "{:?}",
            dark.look.animations
        );
        assert_eq!(dark.look.background, None);
        let light = load_str(css, None, &Options::with_mode(Mode::Light)).unwrap();
        assert_eq!(light.palette.chat, Rgba::rgb(0xff, 0xf8, 0xf7));
        assert_eq!(light.palette.accent, Rgba::rgb(0x90, 0x4a, 0x46));
    }

    #[test]
    fn the_bundled_record_theme_keeps_its_sidebar_background() {
        let css = include_str!("../../../themes/ReCord.theme.css");
        let theme = load_str(css, None, &Options::with_mode(Mode::Dark)).unwrap();
        assert_eq!(theme.native.sidebar, Some(Rgba::rgb(0x0e, 0x10, 0x0c)));
        assert_ne!(theme.native.sidebar, Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn remote_imports_and_their_nested_imports_are_downloaded() {
        let mut calls = Vec::new();
        let css =
            "@import \"https://example.test/base/theme.css\";\n:root { --text-normal: #ffffff; }";
        let theme = load_str_with_imports(css, None, &Options::with_mode(Mode::Dark), &mut |url| {
            calls.push(url.to_owned());
            match url {
                "https://example.test/base/theme.css" => Some(
                    b"@import \"../shared/colors.css\"; :root { --text-normal: #ffffff; }".to_vec(),
                ),
                "https://example.test/shared/colors.css" => Some(
                    b":root { --background-primary: #123456; --brand-experiment: #abcdef; }"
                        .to_vec(),
                ),
                _ => None,
            }
        })
        .unwrap();
        assert_eq!(
            calls,
            [
                "https://example.test/base/theme.css",
                "https://example.test/shared/colors.css"
            ]
        );
        assert_eq!(theme.palette.chat, Rgba::rgb(0x12, 0x34, 0x56));
        assert_eq!(theme.palette.accent, Rgba::rgb(0xab, 0xcd, 0xef));
        assert_eq!(theme.palette.text, Rgba::rgb(255, 255, 255));
        assert!(theme.warnings.is_empty(), "{:?}", theme.warnings);
    }

    #[test]
    fn a_remote_font_is_downloaded_with_its_css() {
        let css = r#"
        @font-face {
            font-family: "Remote";
            font-weight: 700;
            src: url("https://example.test/remote.ttf") format("truetype");
        }
        :root { --background-primary: #111111; --text-normal: #eeeeee; }
        "#;
        let theme = load_str_with_imports(css, None, &Options::with_mode(Mode::Dark), &mut |url| {
            (url == "https://example.test/remote.ttf").then(|| vec![1, 2, 3, 4])
        })
        .unwrap();
        assert_eq!(theme.fonts.len(), 1);
        assert_eq!(theme.fonts[0].family, "Remote");
        assert_eq!(theme.fonts[0].weight, 700);
        assert_eq!(theme.fonts[0].bytes, [1, 2, 3, 4]);
        assert!(theme.warnings.is_empty(), "{:?}", theme.warnings);
    }

    #[test]
    fn a_remote_import_without_a_fetcher_warns_and_keeps_the_local_theme() {
        let theme = load_str(
            "@import \"https://example.test/theme.css\"; :root { --background-primary: #101010; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.palette.chat, Rgba::rgb(0x10, 0x10, 0x10));
        assert!(
            theme
                .warnings
                .iter()
                .any(|warning| warning.contains("could not read the @import")),
            "{:?}",
            theme.warnings
        );
    }

    #[test]
    fn options_unique_to_the_other_appearance_are_still_offered() {
        let theme = load_str(
            ":root { --background-primary: #111111; } \
             .theme-light { --paper-only: #123456; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert!(
            theme
                .options
                .iter()
                .any(|option| option.name == "--paper-only"
                    && matches!(option.kind, crate::OptionKind::Color { .. })),
            "{:?}",
            theme.options
        );
    }

    #[test]
    fn fonts_and_pictures_beside_an_imported_file_use_its_folder() {
        let folder = std::env::temp_dir().join(format!(
            "recore-css-test-{}-{}",
            std::process::id(),
            "relative-assets"
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("sub")).unwrap();
        fs::write(
            folder.join("main.theme.css"),
            "@import \"sub/one.css\"; :root { --background-primary: #111111; }",
        )
        .unwrap();
        fs::write(
            folder.join("sub/one.css"),
            r#"
            @font-face {
                font-family: "Beside";
                src: local("Missing Local"), url("font.ttf") format("truetype");
            }
            :root { --background-image: url("picture.png"); }
            "#,
        )
        .unwrap();
        fs::write(folder.join("sub/font.ttf"), [9, 8, 7]).unwrap();
        fs::write(folder.join("sub/picture.png"), b"not decoded here").unwrap();
        let theme = load(folder.join("main.theme.css")).unwrap();
        assert_eq!(theme.fonts.len(), 1);
        assert_eq!(theme.fonts[0].family, "Beside");
        assert_eq!(theme.fonts[0].bytes, [9, 8, 7]);
        assert_eq!(
            theme.look.background,
            Some(Image::File(folder.join("sub/picture.png")))
        );
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_remote_import_resolves_its_relative_fonts_and_pictures() {
        let css = "@import \"https://example.test/base/theme.css\";";
        let theme =
            load_str_with_imports(
                css,
                None,
                &Options::with_mode(Mode::Dark),
                &mut |url| match url {
                    "https://example.test/base/theme.css" => Some(
                        br#"
                    @font-face {
                        font-family: "Remote beside";
                        src: url("fonts/remote.ttf") format("truetype");
                    }
                    :root {
                        --background-primary: #101010;
                        --background-image: url("images/wp.png");
                    }
                    "#
                        .to_vec(),
                    ),
                    "https://example.test/base/fonts/remote.ttf" => Some(vec![4, 5, 6]),
                    _ => None,
                },
            )
            .unwrap();
        assert_eq!(theme.fonts.len(), 1);
        assert_eq!(theme.fonts[0].bytes, [4, 5, 6]);
        assert_eq!(
            theme.look.background,
            Some(Image::Url("https://example.test/base/images/wp.png".into()))
        );
        assert!(theme.warnings.is_empty(), "{:?}", theme.warnings);
    }

    #[test]
    fn a_bare_tag_does_not_paint_a_region() {
        let theme = load_str(
            "div { background: #123456; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.native.rail, None);
        assert_eq!(theme.native.chat, None);
    }

    #[test]
    fn the_bundled_record_theme_reads_as_recores_palette() {
        let css = include_str!("../../../themes/ReCord.theme.css");
        let theme = load_str(css, None, &Options::default()).unwrap();
        assert_eq!(theme.name, "ReCord");
        assert!(theme.palette.dark);
        assert_eq!(theme.palette.chat, Rgba::rgb(0x08, 0x09, 0x07));
        assert_eq!(theme.palette.text, Rgba::rgb(0xe4, 0xcd, 0xb1));
        assert_eq!(theme.palette.accent, Rgba::rgb(0xe9, 0x42, 0x21));
        assert_eq!(theme.palette.accent_hover, Rgba::rgb(0xed, 0x65, 0x45));
        assert_eq!(theme.look.font_family.as_deref(), Some("Pixel Operator"));
        assert!(
            theme
                .fonts
                .iter()
                .any(|font| font.family == "Pixel Operator" && font.weight == 700)
        );
    }

    #[test]
    fn record_css_exposes_its_options() {
        let css = include_str!("../../../themes/ReCord.theme.css");
        let theme = load_str(css, None, &Options::default()).unwrap();
        let find = |name: &str| {
            theme
                .options
                .iter()
                .find(|option| option.name == name)
                .unwrap_or_else(|| panic!("{name} is offered; got {:?}", theme.options.len()))
        };
        assert!(matches!(
            find("--rl-accent").kind,
            crate::OptionKind::Color { .. }
        ));
        assert!(matches!(
            find("--rl-motion").kind,
            crate::OptionKind::Choice { .. }
        ));
        assert!(matches!(
            find("--rl-scanline-opacity").kind,
            crate::OptionKind::Percent { .. }
        ));
        assert!(matches!(
            find("--rl-background-image").kind,
            crate::OptionKind::Image { .. }
        ));
        assert!(
            theme.options.len() > 40,
            "ReCord has many options: {}",
            theme.options.len()
        );
    }

    #[test]
    fn imports_are_read_beside_the_file_that_names_them() {
        let folder = std::env::temp_dir().join(format!(
            "recore-css-test-{}-{}",
            std::process::id(),
            "imports"
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("sub")).unwrap();
        fs::write(
            folder.join("main.theme.css"),
            "@import \"sub/one.css\"; :root { --background-primary: #111; }",
        )
        .unwrap();
        fs::write(
            folder.join("sub/one.css"),
            "@import \"two.css\"; :root { --text-normal: #eee; }",
        )
        .unwrap();
        fs::write(
            folder.join("sub/two.css"),
            ":root { --brand-experiment: #123456; }",
        )
        .unwrap();
        let theme = load(folder.join("main.theme.css")).unwrap();
        assert_eq!(theme.palette.accent, Rgba::rgb(0x12, 0x34, 0x56));
        assert_eq!(theme.palette.chat, Rgba::rgb(0x11, 0x11, 0x11));
        assert!(theme.warnings.is_empty(), "{:?}", theme.warnings);
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_css_gradient_becomes_a_background_picture() {
        let theme = load_str(
            ":root { --background-image: linear-gradient(90deg, #101014 0%, #e94221 100%); }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        let Some(Image::Gradient(gradient)) = theme.look.background else {
            panic!(
                "expected a gradient background: {:?}",
                theme.look.background
            );
        };
        assert_eq!(gradient.kind, GradientKind::Linear(90.0));
        assert_eq!(gradient.stops.len(), 2);
        assert_eq!(gradient.at(0.0), Rgba::rgb(0x10, 0x10, 0x14));
        assert_eq!(gradient.at(1.0), Rgba::rgb(0xe9, 0x42, 0x21));
    }

    #[test]
    fn a_gradient_keeps_its_last_stop_and_reads_repeating_ones() {
        let theme = load_str(
            ":root { --background-image: repeating-linear-gradient(90deg, #000 0%, #fff 50%); }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        let Some(Image::Gradient(gradient)) = theme.look.background else {
            panic!(
                "expected a gradient background: {:?}",
                theme.look.background
            );
        };
        assert_eq!(gradient.kind, GradientKind::Linear(90.0));
        // Past the last stop the last colour stays, not the one before it.
        assert_eq!(gradient.at(0.75), Rgba::rgb(0xff, 0xff, 0xff));
    }

    #[test]
    fn a_panelled_theme_keeps_its_frame_gap_and_radius() {
        let theme = load_str(
            ":root { --background-base-lower: #0a0a0a; --app-frame-background: #111111; \
             --gap: 7px; --roundness: 6px; } body { background: transparent; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        // A transparent body means the panels show, not an empty chat.
        assert_eq!(theme.palette.chat, Rgba::rgb(0x0a, 0x0a, 0x0a));
        assert_eq!(theme.look.frame, Some(Rgba::rgb(0x11, 0x11, 0x11)));
        assert_eq!(theme.look.panel_gap, Some(7.0));
        assert_eq!(theme.look.panel_radius, Some(6.0));
    }

    #[test]
    fn a_body_background_and_text_colour_reach_the_palette() {
        let theme = load_str(
            "body { background: #202124; color: #e8eaed; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.palette.chat, Rgba::rgb(0x20, 0x21, 0x24));
        assert_eq!(theme.palette.text, Rgba::rgb(0xe8, 0xea, 0xed));
    }

    #[test]
    fn a_body_background_gradient_becomes_the_wallpaper() {
        let theme = load_str(
            "body::before { background: linear-gradient(135deg, #111111, #ffb3ae); }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert!(matches!(theme.look.background, Some(Image::Gradient(_))));
    }

    #[test]
    fn a_body_animation_on_the_ground_is_read() {
        let theme = load_str(
            "@keyframes drift { from { background-color: #101014; opacity: 0.2; } to { background-color: #e94221; opacity: 0.6; } } \
             body { animation: drift 4s ease-in-out infinite; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(
            theme.look.animations.len(),
            1,
            "{:?}",
            theme.look.animations
        );
        let animation = &theme.look.animations[0];
        assert_eq!(animation.duration, 4.0);
        let middle = animation.at(2.0).unwrap();
        assert!(middle.background.is_some());
        let opacity = middle.opacity.unwrap();
        assert!((opacity - 0.4).abs() < 0.02, "{opacity}");
    }

    #[test]
    fn an_animation_without_a_duration_uses_css_defaults() {
        let keyframes = "@keyframes drift { from { background-color: #101014; } to { background-color: #e94221; } }";
        // A name alone is 0 s, so it never moves.
        let still = load_str(
            &format!("{keyframes} body {{ animation-name: drift; }}"),
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert!(still.look.animations.is_empty());
        // A duration without a count runs once, and `infinite` still means
        // forever.
        let once = load_str(
            &format!("{keyframes} body {{ animation: drift 2s; }}"),
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        let animation = &once.look.animations[0];
        assert_eq!((animation.duration, animation.iterations), (2.0, Some(1.0)));
        let forever = load_str(
            &format!("{keyframes} body {{ animation: drift 2s infinite; }}"),
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(forever.look.animations[0].iterations, None);
    }

    #[test]
    fn a_theme_can_fade_each_section_differently() {
        let theme = load_str(
            ":root { --rl-rail-opacity: 40%; --rl-sidebar-opacity: 0.6; \
             --rl-chat-opacity: 80; --rl-members-opacity: 0.9; \
             --rl-composer-opacity: 1; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.native.rail_opacity, Some(0.4));
        assert_eq!(theme.native.sidebar_opacity, Some(0.6));
        assert_eq!(theme.native.chat_opacity, Some(0.8));
        assert_eq!(theme.native.panel_opacity, Some(0.9));
        assert_eq!(theme.native.field_opacity, Some(1.0));
    }

    #[test]
    fn a_theme_can_keep_a_panel_opacity() {
        let theme = load_str(
            ":root { --background-image: linear-gradient(#000, #fff); --rl-panel-opacity: 72%; --background-size: contain; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.look.panel_opacity, Some(0.72));
        assert_eq!(theme.look.background_fit, BackgroundFit::Contain);
    }

    #[test]
    fn a_palette_token_no_rule_names_still_reaches_the_palette() {
        let theme = load_str(
            ":root { --button-secondary-background: #123456; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.palette.button_default, Rgba::rgb(0x12, 0x34, 0x56));
    }

    #[test]
    fn a_chosen_theme_can_paint_its_page() {
        let mut options = Options::with_mode(Mode::Dark);
        options.paint = true;
        let theme = load_str("body { background: #123456; }", None, &options).unwrap();
        let paint = theme.paint.expect("the page paint");
        assert!(!paint.ops.is_empty());
    }

    #[test]
    fn a_flex_page_names_its_panel_widths() {
        let theme = load_str(
            "body { display: flex; } \
             [class*=\"guilds_\"] { width: 72px; } \
             [class*=\"sidebar_\"] { width: 280px; } \
             [class*=\"chat_\"] { flex: 1; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        let layout = theme.page_layout.expect("the flex page's boxes");
        assert_eq!(layout.rail, 72.0);
        assert_eq!(layout.sidebar, 280.0);
    }

    #[test]
    fn a_flex_page_gap_reaches_the_panel_layout() {
        let theme = load_str(
            "#app-mount { display: flex; flex-direction: row; gap: 9px; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.look.panel_gap, Some(9.0));
    }

    #[test]
    fn length_functions_in_a_custom_property_settle_to_pixels() {
        for (value, expected) in [
            ("calc(8px + 4px)", 12.0),
            ("min(12px, 20px)", 12.0),
            ("max(4px, 12px)", 12.0),
            ("clamp(4px, 12px, 20px)", 12.0),
            ("calc(6px * 2)", 12.0),
            ("calc(1rem + 4px)", 20.0),
        ] {
            let css = format!(":root {{ --radius-md: {value}; }}");
            let theme = load_str(&css, None, &Options::with_mode(Mode::Dark)).expect("theme");
            assert_eq!(theme.look.radius, Some(expected), "{value}");
        }
    }

    #[test]
    fn a_length_expression_nests_and_divides() {
        let nested = load_str(
            ":root { --radius-md: calc((4px + 2px) * 2); }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(nested.look.radius, Some(12.0));

        let divided = load_str(
            ":root { --radius-md: calc(24px / 2); }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(divided.look.radius, Some(12.0));
    }

    #[test]
    fn an_invalid_length_expression_is_no_length() {
        for value in [
            "calc()",
            "calc(1px / 0)",
            "calc(1px +)",
            "min(1px,)",
            "clamp(1px, 2px)",
            "50%",
        ] {
            let css = format!(":root {{ --radius-md: {value}; }}");
            let theme = load_str(&css, None, &Options::with_mode(Mode::Dark)).expect("theme");
            assert_eq!(theme.look.radius, None, "{value}");
        }
    }

    #[test]
    fn a_heading_size_inherits_the_pages_font_size() {
        let theme = load_str(
            ":root { font-size: 20px; } \
             [class*=\"sidebarList_\"] [class*=\"category_\"] { font-size: 1.5em; }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.native.category_size, Some(30.0));
    }

    #[test]
    fn a_theme_with_both_oldcord_class_blocks_reads_as_dark() {
        let theme = load_str(
            r#"
            html:not(.custom).theme-dark:not(.theme-darker, .theme-midnight) {
                --background-primary: #36393f;
                --text-normal: #ffffff;
            }
            html:not(.custom).theme-light {
                --background-primary: #f0f1f5 !important;
                --text-normal: #535461 !important;
            }
            "#,
            None,
            &Options::default(),
        )
        .unwrap();
        assert!(theme.palette.dark);
        assert_eq!(theme.palette.chat, Rgba::rgb(0x36, 0x39, 0x3f));
        assert_eq!(theme.palette.text, Rgba::rgb(255, 255, 255));
    }

    #[test]
    fn clearvision_names_its_ink_and_accent_with_its_own_variables() {
        let theme = load_str(
            r#"
            :root { --main-color: #2780e6; --normal-text: #dcdcde; }
            .theme-dark {
                --text-default: hsl(from var(--normal-text) h calc(1 * s) l/alpha);
            }
            "#,
            None,
            &Options::default(),
        )
        .unwrap();
        assert!(theme.palette.dark);
        assert_eq!(theme.palette.accent, Rgba::rgb(0x27, 0x80, 0xe6));
        assert_eq!(theme.palette.text, Rgba::rgb(0xdc, 0xdc, 0xde));
    }

    #[test]
    fn a_container_style_query_can_choose_the_light_palette() {
        let theme = load_str(
            r#"
            @property --light-mode {
                syntax: "<custom-ident>";
                inherits: true;
                initial-value: off;
            }
            :root { --paper: #eee; --ink: #222; }
            body { --light-mode: on; }
            @container body style(--light-mode: on) {
                :root {
                    --background-primary: var(--paper);
                    --text-normal: var(--ink);
                }
            }
            "#,
            None,
            &Options::default(),
        )
        .unwrap();
        assert!(!theme.palette.dark);
        assert_eq!(theme.palette.chat, Rgba::rgb(0xee, 0xee, 0xee));
        assert_eq!(theme.palette.text, Rgba::rgb(0x22, 0x22, 0x22));
    }

    #[test]
    fn a_container_style_query_follows_the_property_it_names() {
        let css = r#"
            :root { --mode: off; --background-primary: #101010; }
            @container style(--mode: on) {
                :root { --background-primary: #ffffff; }
            }
            @container not style(--mode: on) {
                :root { --text-normal: #eeeeee; }
            }
        "#;
        let off = load_str(css, None, &Options::with_mode(Mode::Dark)).unwrap();
        assert_eq!(off.palette.chat, Rgba::rgb(0x10, 0x10, 0x10));
        assert_eq!(off.palette.text, Rgba::rgb(0xee, 0xee, 0xee));
        let mut options = Options::with_mode(Mode::Dark);
        options.overrides.push(("--mode".into(), "on".into()));
        let on = load_str(css, None, &options).unwrap();
        assert_eq!(on.palette.chat, Rgba::rgb(0xff, 0xff, 0xff));
        assert_ne!(on.palette.text, Rgba::rgb(0xee, 0xee, 0xee));
    }

    #[test]
    fn a_property_initial_value_fills_a_missing_variable() {
        let theme = load_str(
            r#"
            @property --brand {
                syntax: "<color>";
                inherits: true;
                initial-value: #123456;
            }
            :root { --background-primary: var(--brand); }
            "#,
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.palette.chat, Rgba::rgb(0x12, 0x34, 0x56));
    }

    #[test]
    fn a_color_mix_background_reaches_the_native_region() {
        let theme = load_str(
            "[class*=\"sidebar_\"] { background: color-mix(in srgb, #112233 50%, #445566); }",
            None,
            &Options::with_mode(Mode::Dark),
        )
        .unwrap();
        assert_eq!(theme.native.sidebar, Some(Rgba::rgb(0x2b, 0x3c, 0x4d)));
    }

    #[test]
    fn bundled_css_themes_leave_nothing_in_other() {
        for name in [
            "ReCord.theme.css",
            "Ambxst.theme.css",
            "Catppuccin Mocha.theme.css",
            "Tokyo Night.theme.css",
            "Nord Aurora.theme.css",
            "Gruvbox Ember.theme.css",
            "Solarized Paper.theme.css",
        ] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../themes")
                .join(name);
            let theme = load(path).unwrap();
            let other: Vec<_> = theme
                .options
                .iter()
                .filter(|option| option.group == "Other")
                .collect();
            assert!(other.is_empty(), "{}: {other:?}", theme.name);
        }
    }

    #[test]
    fn a_theme_in_a_subfolder_is_read_and_a_duplicate_is_not() {
        let folder = std::env::temp_dir().join(format!(
            "recore-css-test-{}-{}",
            std::process::id(),
            "nested"
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("pack/themes")).unwrap();
        let css = ":root { --background-primary: #123456; }";
        fs::write(folder.join("pack/themes/nested.theme.css"), css).unwrap();
        fs::write(folder.join("same.theme.css"), css).unwrap();
        let scan = scan(std::slice::from_ref(&folder));
        assert_eq!(scan.themes.len(), 1, "{:?}", scan.problems);
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_missing_theme_folder_is_not_a_problem() {
        let folder = std::env::temp_dir().join(format!(
            "recore-css-test-{}-{}",
            std::process::id(),
            "missing"
        ));
        let scan = scan(&[folder]);
        assert!(scan.themes.is_empty());
        assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    }

    #[test]
    fn themes_in_a_folder_are_read_and_named() {
        let folder =
            std::env::temp_dir().join(format!("recore-css-test-{}-{}", std::process::id(), "scan"));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("one.theme.css"), RECORD).unwrap();
        fs::write(
            folder.join("two.theme.css"),
            ":root{--background-primary:#123456}",
        )
        .unwrap();
        let scan = scan(std::slice::from_ref(&folder));
        assert_eq!(scan.themes.len(), 2);
        assert_eq!(scan.themes[0].name, "ReCord");
        fs::remove_dir_all(&folder).unwrap();
    }
}
