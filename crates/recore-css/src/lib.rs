//! ReCore's reader for Discord CSS themes: Vencord, Vesktop, Equicord,
//! BetterDiscord, Legcord and any other client that loads a `.theme.css`
//! file of custom properties.
//!
//! The loader keeps the theme's files, fonts, pictures and Appearance
//! metadata. It settles media queries at the window's reference size and
//! removes `@container style(...)` rules whose condition does not match the
//! theme's own variables; `style-engine` then settles the selectors,
//! cascade, inheritance and custom properties against ReCore's synthetic
//! regions, and `semantic` maps the computed values to the window. The
//! interpreter reads the theme's palette, accent, fonts, radius,
//! background picture and ReCord's CRT options into [`ThemeFile`], which the
//! window turns into its own colours. It also matches the theme's selectors
//! against ReCore's native regions (sidebar, chat, member list, composer,
//! settings pages, threads, forums, calls, the inbox, profiles, search and
//! polls) and applies the colours, opacity, shape, typography, shadows and
//! popup pacing that win the cascade there. The pseudo-elements a native
//! window can paint are read too: a region's `::before` or `::after`
//! decoration, `::selection`, `::placeholder` and a scrollbar's thumb. CSS
//! for Discord's web DOM has no exact nodes in a native window; a page-level
//! flex or grid names the panel widths, but a region's internal layout,
//! `:nth-child` structure, masks and per-element keyframe animations are read
//! without error but are not painted. A root/body colour or opacity animation
//! is painted as a moving ground, and CSS gradients become background
//! pictures. Local `@import`s are read beside the theme; [`load_with_imports`]
//! and [`scan_with_imports`] take a callback for `http:` and `https:` ones
//! and their remote fonts. A remote picture stays [`Image::Url`] for the
//! caller to fetch. [`theme_folders`] lists the places the common clients
//! keep local themes, and [`scan`] reads them.
//!
//! ```
//! use recore_css::{Mode, Options, load_str};
//!
//! let css = ":root { --background-primary: #101014; --brand-experiment: #e94221; }";
//! let theme = load_str(css, None, &Options::with_mode(Mode::Dark)).unwrap();
//! assert_eq!(theme.palette.accent.hex(), "#e94221");
//! ```

mod base64;
pub mod color;
pub mod css;
mod native;
mod options;
mod semantic;
mod theme;

pub use color::Rgba;
pub use css::{Keyframe, Keyframes, Meta, MetaVar, Note, ParseError, PropertySpec, Stylesheet};
pub use native::{NativeStyle, Shadow, TextTransform};
pub use options::{OptionKind, ThemeOption};
pub use style_engine;
pub use style_engine as paint_engine;
pub use style_engine::{DisplayList as Paint, PaintOp};
pub use theme::{
    Animation, AnimationDirection, AnimationFrame, BackgroundFit, Effects, Font, Gradient,
    GradientKind, GradientStop, Image, Look, Mode, Motion, Options, PageLayout, Palette, Problem,
    Scan, ThemeFile, load, load_str, load_str_with_imports, load_with, load_with_imports, palette,
    scan, scan_with_imports, theme_files_fingerprint, theme_folders,
};

/// The version of the interpreter, shown beside a loaded theme.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_with_no_colours_still_gets_a_usable_palette() {
        let theme = load_str(
            ":root { --font-primary: monospace; }",
            None,
            &Options::default(),
        )
        .expect("readable");
        assert!(theme.palette.chat.contrast(theme.palette.text) > 4.0);
        assert_eq!(theme.look.font_family, None);
    }

    #[test]
    fn the_cache_of_theme_folders_is_only_paths() {
        for folder in theme_folders() {
            assert!(folder.file_name().is_some());
        }
    }
}
