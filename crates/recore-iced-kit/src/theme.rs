//! The window's colours, as every widget reads them, in iced's colour type,
//! with the colours worked out from them. The app fills one from a theme of
//! its own ([`Theme::new`], then [`Theme::worked_out`]). See docs/themes.md.

use std::borrow::Cow;

use iced::Color;

/// How a line of the window's own text changes case, as a CSS theme's
/// `text-transform` asks for the region it names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

impl Transform {
    /// `text` with this transformation applied.
    pub fn apply(self, text: &str) -> Cow<'_, str> {
        match self {
            Self::None => Cow::Borrowed(text),
            Self::Uppercase => Cow::Owned(text.to_uppercase()),
            Self::Lowercase => Cow::Owned(text.to_lowercase()),
            Self::Capitalize => {
                let mut capital = true;
                let mut result = String::with_capacity(text.len());
                for character in text.chars() {
                    if capital {
                        result.extend(character.to_uppercase());
                    } else {
                        result.push(character);
                    }
                    capital = character.is_whitespace();
                }
                Cow::Owned(result)
            }
        }
    }
}

/// Calls `$then!` with the names of [`Theme`]'s own colours, so the app
/// that fills them names each once too.
#[macro_export]
macro_rules! for_theme_colours {
    ($then:ident) => {
        $then! {
        rail,
        sidebar,
        panel,
        chat,
        stage,
        raised,
        field,
        sunken,
        hover,
        active,
        row_hover,
        line,
        edge,
        outline,
        outline_chat,
        outline_field,
        text,
        secondary,
        muted,
        faint,
        accent,
        accent_hover,
        on_accent,
        accent_soft,
        danger,
        danger_hover,
        on_danger,
        live,
        loud,
        link,
        button_default,
        button_default_hover,
        overlay,
        shimmer,
        reply_spine,
        tooltip,
        avatar,
        }
    };
}

/// Declares [`Theme`] with its colours.
macro_rules! declare_theme {
    ($($name:ident,)*) => {
        /// A theme's colours, as the widgets paint with them.
        #[derive(Clone, Debug, PartialEq)]
        pub struct Theme {
            /// The theme's id, as `layout.json` names it.
            pub id: String,
            pub dark: bool,
            /// Bonfire or a theme made from it, which draws a few things in
            /// Discord's measured dark-theme inks instead of its palette's.
            pub bonfire: bool,
            $(pub $name: Color,)*
            /// What's worked out from those: a context menu's ground and
            /// outline, the composer's ground and placeholder, a destructive
            /// row's text.
            pub menu: Color,
            pub menu_line: Color,
            pub composer: Color,
            pub placeholder: Color,
            pub danger_text: Color,
            /// The channel list's inks: a read channel's name, glyph and a
            /// category's name, and the open or unread channel's name. Bonfire
            /// draws Discord's, measured; other themes the muted and text
            /// colours.
            pub channel_read: Color,
            pub channel_white: Color,
            /// The Friends and Requests rows above the DM list: inactive,
            /// and open (Discord's on Bonfire; the muted and text colours on
            /// other themes).
            pub nav_read: Color,
            pub nav_open: Color,
            /// Text selected in the timeline and fields: the accent at 35%.
            pub selection: Color,
            /// A mention's colour without one of its own.
            pub mention: Color,
            /// Whether panels, menus and buttons use square corners, as
            /// ReCord's terminal theme does.
            pub square: bool,
            /// The ground behind the gaps between panels, and how wide those
            /// gaps and the panels' corners are. Zero keeps ReCore's own
            /// full-bleed layout.
            pub frame: Color,
            pub panel_gap: f32,
            pub panel_radius: f32,
            /// The channel and member list widths a CSS page-level flex or
            /// grid names, until the person drags an edge.
            pub sidebar_width: Option<f32>,
            pub members_width: Option<f32>,
            /// A channel list's category heading, a member list's group
            /// heading and the conversation's title: their size and case, as
            /// a CSS theme's rules on those regions name them.
            pub category_size: f32,
            pub category_transform: Transform,
            pub member_heading_size: f32,
            pub member_heading_transform: Transform,
            pub header_size: f32,
            pub header_transform: Transform,
            /// The shadows a CSS theme's `box-shadow` gives raised panels
            /// (menus, popouts, tooltips) and cards (dialogs, profile
            /// popouts). Other themes keep ReCore's own.
            pub raised_shadow: iced::Shadow,
            pub card_shadow: iced::Shadow,
            /// A CSS theme's linear gradient behind the rail and sidebar, and
            /// behind the chat; the matching colour is the fallback.
            pub navigation_gradient: Option<iced::Gradient>,
            pub chat_gradient: Option<iced::Gradient>,
            /// A scrollbar's thumb at rest and under the pointer. Transparent
            /// keeps ReCore's own shades.
            pub scrollbar_rest: Color,
            pub scrollbar_pointed: Color,
            /// What iced's own styles read: the theme's ground, text and
            /// accent.
            iced: iced::Theme,
        }

        impl Theme {
            /// A theme named `id`, every colour clear, for the app to fill
            /// and then [`work out`](Theme::worked_out).
            pub fn new(id: &str, dark: bool) -> Self {
                Self {
                    id: id.to_owned(),
                    dark,
                    bonfire: false,
                    $($name: Color::TRANSPARENT,)*
                    menu: Color::TRANSPARENT,
                    menu_line: Color::TRANSPARENT,
                    composer: Color::TRANSPARENT,
                    placeholder: Color::TRANSPARENT,
                    danger_text: Color::TRANSPARENT,
                    channel_read: Color::TRANSPARENT,
                    channel_white: Color::TRANSPARENT,
                    nav_read: Color::TRANSPARENT,
                    nav_open: Color::TRANSPARENT,
                    selection: Color::TRANSPARENT,
                    mention: Color::TRANSPARENT,
                    square: false,
                    frame: Color::TRANSPARENT,
                    panel_gap: 0.0,
                    panel_radius: 0.0,
                    sidebar_width: None,
                    members_width: None,
                    category_size: 14.0,
                    category_transform: Transform::None,
                    member_heading_size: 14.0,
                    member_heading_transform: Transform::None,
                    header_size: 16.0,
                    header_transform: Transform::None,
                    raised_shadow: crate::style::default_raised_shadow(),
                    card_shadow: crate::style::default_card_shadow(),
                    navigation_gradient: None,
                    chat_gradient: None,
                    scrollbar_rest: Color::TRANSPARENT,
                    scrollbar_pointed: Color::TRANSPARENT,
                    iced: iced::Theme::Dark,
                }
            }
        }
    };
}

for_theme_colours!(declare_theme);

/// WCAG's contrast ratio between two opaque colours.
pub fn contrast(a: Color, b: Color) -> f32 {
    let luminance = |colour: Color| {
        let channel = |value: f32| {
            if value <= 0.040_45 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(colour.r) + 0.7152 * channel(colour.g) + 0.0722 * channel(colour.b)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `ink` moved towards `toward` until it reads on `ground` at `least`, in
/// tenths; an ink that already reads is returned as it is.
pub fn readable_on(ink: Color, toward: Color, ground: Color, least: f32) -> Color {
    if contrast(ink, ground) >= least {
        return ink;
    }
    let mut best = ink;
    for step in 1..=10 {
        // `mix` takes the share of its first colour, so this walks from the
        // ink towards the theme's text in tenths.
        best = mix(ink, toward, 1.0 - step as f32 / 10.0);
        if contrast(best, ground) >= least {
            break;
        }
    }
    // A palette whose two inks are both too close to the ground keeps the
    // nearest it has rather than turning black.
    best
}

impl Theme {
    /// The colours worked out from the others, once those are set.
    pub fn worked_out(self) -> Self {
        let dark = self.dark;
        let iced = iced::Theme::custom(
            format!("ReCore {}", self.id),
            iced::theme::Palette {
                background: self.chat,
                text: self.text,
                primary: self.accent,
                success: self.live,
                warning: self.loud,
                danger: self.danger,
            },
        );
        Self {
            menu: if dark {
                mix(self.raised, Color::WHITE, 0.98)
            } else {
                self.raised
            },
            menu_line: if dark {
                mix(self.line, Color::BLACK, 0.95)
            } else {
                self.line
            },
            composer: if dark {
                mix(self.field, Color::WHITE, 0.99)
            } else {
                self.field
            },
            placeholder: if dark {
                mix(self.muted, self.field, 0.66)
            } else {
                self.muted
            },
            // An error reads on the panel it sits on: a light palette's own
            // red can sit under 4.5:1 there, so it is darkened towards the
            // palette's ink until it reads.
            danger_text: if dark {
                crate::style::hex(0xfa777c)
            } else {
                readable_on(self.danger, self.text, self.panel, 4.5)
            },
            channel_read: if self.bonfire {
                crate::style::hex(0x81828a)
            } else {
                self.muted
            },
            channel_white: if self.bonfire {
                crate::style::hex(0xfbfbfb)
            } else {
                self.text
            },
            nav_read: if self.bonfire {
                crate::style::hex(0x96979e)
            } else {
                self.muted
            },
            nav_open: if self.bonfire {
                crate::style::hex(0xefeff1)
            } else {
                self.text
            },
            selection: Color {
                a: 0.35,
                ..self.accent
            },
            iced,
            ..self
        }
    }

    /// iced's own theme in these colours, for the styles iced draws itself.
    pub fn iced(&self) -> iced::Theme {
        self.iced.clone()
    }

    /// The overlay colour at `alpha`: what lifts a surface under the
    /// pointer, white on a dark theme, dark on a light one.
    pub fn wash(&self, alpha: f32) -> Color {
        Color {
            a: alpha,
            ..self.overlay
        }
    }
}

/// An opaque colour as `0xrrggbb`, for the client's own pictures (icons,
/// dots), which are drawn once per colour.
pub fn rgb(color: Color) -> u32 {
    let [r, g, b, _] = color.into_rgba8();
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// Slint's `mix`: `factor` of `color` and the rest of `other`, weighted by
/// their alpha as Sass mixes them.
pub fn mix(color: Color, other: Color, factor: f32) -> Color {
    let factor = factor.clamp(0.0, 1.0);
    let normal = 2.0 * factor - 1.0;
    let distance = color.a - other.a;
    let weighted = normal * distance;
    let combined = if weighted == -1.0 {
        normal
    } else {
        (normal + distance) / (1.0 + weighted)
    };
    let channels = (combined + 1.0) / 2.0;
    let lerp = |a: f32, b: f32, f: f32| (a * f + b * (1.0 - f)).clamp(0.0, 1.0);
    Color {
        r: lerp(color.r, other.r, channels),
        g: lerp(color.g, other.g, channels),
        b: lerp(color.b, other.b, channels),
        a: lerp(color.a, other.a, factor),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_ink_reads_on_a_light_panel() {
        // Rosé Pine Dawn's red on its light panel: under 4.5:1 as written,
        // over it once darkened towards the palette's ink.
        let ink = crate::style::hex(0xb4637a);
        let text = crate::style::hex(0x575279);
        let panel = crate::style::hex(0xdfdad9);
        assert!(contrast(ink, panel) < 4.5);
        let read = readable_on(ink, text, panel, 4.5);
        assert!(contrast(read, panel) >= 4.5, "{:?}", contrast(read, panel));

        // An ink that already reads is untouched.
        let dark = crate::style::hex(0x050505);
        assert_eq!(readable_on(dark, text, panel, 4.5), dark);
    }
}
