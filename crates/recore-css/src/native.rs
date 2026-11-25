//! CSS rules for Discord's web window, matched against ReCore's own window.
//!
//! A native window has no `.messageContent_...` nodes, so a selector cannot
//! be painted as it would be in a browser. It can still describe a *role*:
//! the sidebar, the chat, the member list, the composer, a selected channel.
//! This module gives every role a small synthetic node (with the class
//! fragments Discord themes commonly target), runs the theme's selectors
//! against those nodes, and keeps the winning colours and shape values.
//!
//! It is deliberately not a browser: it understands the selectors and
//! properties a theme uses to skin those regions, not layout, pseudo-elements
//! or DOM animations. The window applies the result on top of the palette it
//! already reads from the theme's root variables.

use std::{borrow::Cow, collections::BTreeMap};

use crate::{
    Rgba,
    color::{self, parse as parse_color},
    css::{Declaration, Rule, Vars},
};

/// How a region's text is transformed, as CSS's `text-transform`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

impl TextTransform {
    /// `text` with this transformation applied. The native window keeps the
    /// text's words and only changes its case, as CSS does.
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

/// A `box-shadow` a theme gives a region, ready for the window's renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Rgba,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
}

/// The parts of a Discord window a theme can address. A node may match more
/// than one selector fragment (for example `sidebar` and `sidebarList_`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Role {
    Root,
    Rail,
    Sidebar,
    Chat,
    Members,
    Header,
    Messages,
    Message,
    Composer,
    Member,
    Category,
    Thread,
    Forum,
    ForumPost,
    ServerSettings,
    Stage,
    Inbox,
    Profile,
    MembersGroup,
    Uploads,
    SearchResults,
    Poll,
    Card,
    Menu,
    Tooltip,
}

/// One synthetic node in the native window.
pub(crate) struct Node {
    pub(crate) role: Role,
    pub(crate) tags: &'static [&'static str],
    pub(crate) id: Option<&'static str>,
    pub(crate) classes: &'static [&'static str],
    pub(crate) parent: Option<usize>,
    /// The `role` attribute Discord puts on this part, which a theme may
    /// name instead of a class (`[role="tooltip"]`).
    pub(crate) aria_role: Option<&'static str>,
}

const ROOT: usize = 0;
const CHAT: usize = 3;
const MEMBERS: usize = 4;
const MESSAGES: usize = 6;

/// The tree ReCore shows, with the class names themes tend to mention. The
/// class fragments end in `_` because Discord's generated class names do.
pub(crate) const NODES: [Node; 25] = [
    Node {
        role: Role::Root,
        tags: &["html", "body"],
        id: Some("app-mount"),
        classes: &[
            "app",
            "app_",
            "appMount",
            "appMount_",
            "bg",
            "bg_",
            "layers",
            "layers_",
            "theme-dark",
            "theme-light",
        ],
        parent: None,
        aria_role: None,
    },
    Node {
        role: Role::Rail,
        tags: &["nav", "div"],
        id: None,
        classes: &["guilds", "guilds_", "guildsnav___", "guildRail", "rail"],
        parent: Some(ROOT),
        aria_role: None,
    },
    Node {
        role: Role::Sidebar,
        tags: &["div", "nav"],
        id: None,
        classes: &[
            "sidebar",
            "sidebar_",
            "sidebarList_",
            "channels",
            "channelList",
            "privateChannels",
            "privateChannels_",
        ],
        parent: Some(ROOT),
        aria_role: None,
    },
    Node {
        role: Role::Chat,
        tags: &["main", "div"],
        id: None,
        classes: &[
            "chat",
            "chat_",
            "chatContent_",
            "messagesWrapper",
            "messagesWrapper_",
        ],
        parent: Some(ROOT),
        aria_role: None,
    },
    Node {
        role: Role::Members,
        tags: &["aside", "div"],
        id: None,
        classes: &[
            "members",
            "members_",
            "membersWrap_",
            "memberList",
            "membersList",
        ],
        parent: Some(ROOT),
        aria_role: None,
    },
    Node {
        role: Role::Header,
        tags: &["header", "div"],
        id: None,
        classes: &["header", "header_", "title_", "channelHeader", "toolbar"],
        parent: Some(CHAT),
        aria_role: None,
    },
    Node {
        role: Role::Messages,
        tags: &["div"],
        id: None,
        classes: &["messages", "messages_", "scroller", "messageList"],
        parent: Some(CHAT),
        aria_role: None,
    },
    Node {
        role: Role::Message,
        tags: &["div", "li"],
        id: None,
        classes: &[
            "message",
            "message_",
            "messageContent",
            "messageContent_",
            "messageListItem",
        ],
        parent: Some(MESSAGES),
        aria_role: None,
    },
    Node {
        role: Role::Composer,
        tags: &["form", "div"],
        id: None,
        classes: &[
            "channelTextArea",
            "channelTextArea_",
            "textArea",
            "textArea_",
            "composer",
        ],
        parent: Some(CHAT),
        aria_role: None,
    },
    Node {
        role: Role::Member,
        tags: &["div", "li"],
        id: None,
        classes: &["member", "member_", "memberInner", "memberRow"],
        parent: Some(MEMBERS),
        aria_role: None,
    },
    Node {
        role: Role::Card,
        tags: &["div", "section"],
        id: None,
        classes: &[
            "card",
            "card_",
            "modal_",
            "popout_",
            "userPopout_",
            "profilePanel",
        ],
        parent: Some(ROOT),
        aria_role: Some("dialog"),
    },
    // The tooltip Discord draws above a badge, a reaction or a control.
    Node {
        role: Role::Tooltip,
        tags: &["div", "span"],
        id: None,
        classes: &[
            "tooltip",
            "tooltip_",
            "tooltipContainer_",
            "tooltipContent_",
            "tooltipText_",
        ],
        parent: Some(ROOT),
        aria_role: Some("tooltip"),
    },
    // A category heading in the channel list.
    Node {
        role: Role::Category,
        tags: &["div", "li", "h2"],
        id: None,
        classes: &[
            "category",
            "category_",
            "sidebarCategory_",
            "section",
            "section_",
            "sidebarSection_",
        ],
        parent: Some(2),
        aria_role: None,
    },
    // The heading over a member list's group, Online or a role's name.
    Node {
        role: Role::MembersGroup,
        tags: &["div", "h3"],
        id: None,
        classes: &[
            "membersGroup",
            "membersGroup_",
            "memberGroup_",
            "group_",
            "groupContainer_",
            "membersGroupContainer_",
        ],
        parent: Some(MEMBERS),
        aria_role: None,
    },
    // A thread's panel beside the channel it belongs to.
    Node {
        role: Role::Thread,
        tags: &["aside", "div"],
        id: None,
        classes: &[
            "threadSidebar",
            "threadSidebar_",
            "threadSidebarOpen_",
            "threadContainer_",
            "threadPanel_",
        ],
        parent: Some(CHAT),
        aria_role: None,
    },
    // A forum's list of posts and one post's card.
    Node {
        role: Role::Forum,
        tags: &["div", "main"],
        id: None,
        classes: &[
            "forum",
            "forum_",
            "forumContainer_",
            "forumPosts_",
            "forumContent_",
            "forumGuidelines_",
        ],
        parent: Some(CHAT),
        aria_role: None,
    },
    Node {
        role: Role::ForumPost,
        tags: &["div", "article", "li"],
        id: None,
        classes: &[
            "forumPost",
            "forumPost_",
            "forumPostContainer_",
            "forumPostContent_",
            "postCard_",
        ],
        parent: Some(15),
        aria_role: None,
    },
    // A server's or channel's settings page.
    Node {
        role: Role::ServerSettings,
        tags: &["div", "main"],
        id: None,
        classes: &[
            "serverSettings",
            "serverSettings_",
            "guildSettings_",
            "settingsPage_",
            "settingsSidebar_",
            "settingsContent_",
            "contentColumn_",
            "sidebarRegion_",
            "side_",
        ],
        parent: Some(ROOT),
        aria_role: None,
    },
    // A stage channel's audience and the call's controls.
    Node {
        role: Role::Stage,
        tags: &["div", "section"],
        id: None,
        classes: &[
            "stage",
            "stage_",
            "stageContainer_",
            "callContainer_",
            "voice_",
            "participants_",
            "videoWrapper_",
            "callPanel_",
        ],
        parent: Some(CHAT),
        aria_role: None,
    },
    // The inbox: recent mentions and unread messages.
    Node {
        role: Role::Inbox,
        tags: &["div", "aside"],
        id: None,
        classes: &[
            "inbox",
            "inbox_",
            "inboxContainer_",
            "recentMentions_",
            "recentMentionsPopout_",
            "messagesPopout_",
            "mentions_",
        ],
        parent: Some(CHAT),
        aria_role: None,
    },
    // A person's profile card and its banner.
    Node {
        role: Role::Profile,
        tags: &["div", "section"],
        id: None,
        classes: &[
            "profile",
            "profile_",
            "profilePanel_",
            "profileBanner_",
            "profileHeader_",
            "profileBody_",
            "profileNote_",
            "banner_",
        ],
        parent: Some(10),
        aria_role: None,
    },
    // The attachments waiting over the composer.
    Node {
        role: Role::Uploads,
        tags: &["div", "section"],
        id: None,
        classes: &[
            "upload",
            "upload_",
            "uploads_",
            "uploadArea_",
            "uploadContainer_",
            "attachWrapper_",
            "attachmentUpload_",
            "uploadDropModal_",
            "uploadFile_",
        ],
        parent: Some(8),
        aria_role: None,
    },
    // The results panel a search opens.
    Node {
        role: Role::SearchResults,
        tags: &["div", "section"],
        id: None,
        classes: &[
            "searchResults",
            "searchResults_",
            "searchResultsWrap_",
            "searchResult_",
            "searchResultGroup_",
            "searchResultContainer_",
            "searchPage_",
        ],
        parent: Some(CHAT),
        aria_role: None,
    },
    // A poll's card on a message.
    Node {
        role: Role::Poll,
        tags: &["div", "section"],
        id: None,
        classes: &[
            "poll",
            "poll_",
            "pollContainer_",
            "pollAnswer_",
            "pollResults_",
            "pollMessage_",
        ],
        parent: Some(7),
        aria_role: None,
    },
    // A context menu, named by its role attribute or its own fragment.
    Node {
        role: Role::Menu,
        tags: &["div", "ul"],
        id: None,
        classes: &[
            "menu",
            "menu_",
            "menuContainer_",
            "contextMenu_",
            "submenu_",
        ],
        parent: Some(ROOT),
        aria_role: Some("menu"),
    },
];

/// Colours and shape values a theme's selectors give ReCore's regions.
///
/// `None` leaves the value the theme's root variables already picked. Every
/// field is a value that can be applied to both the window's palette and the
/// Appearance tile without introducing a second theme model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeStyle {
    pub rail: Option<Rgba>,
    pub sidebar: Option<Rgba>,
    pub chat: Option<Rgba>,
    /// A linear gradient on the rail, the sidebar or the chat, which the
    /// window paints instead of the solid colour those fields keep as a
    /// fallback.
    pub rail_gradient: Option<crate::Gradient>,
    pub sidebar_gradient: Option<crate::Gradient>,
    pub chat_gradient: Option<crate::Gradient>,
    pub panel: Option<Rgba>,
    pub raised: Option<Rgba>,
    pub field: Option<Rgba>,
    pub stage: Option<Rgba>,
    pub tooltip: Option<Rgba>,
    pub text: Option<Rgba>,
    pub secondary: Option<Rgba>,
    pub muted: Option<Rgba>,
    pub hover: Option<Rgba>,
    pub row_hover: Option<Rgba>,
    pub line: Option<Rgba>,
    pub accent: Option<Rgba>,
    /// The ground of selected text, as `::selection` names it.
    pub selection: Option<Rgba>,
    /// A field's hint text, as `::placeholder` names it.
    pub placeholder: Option<Rgba>,
    /// A scrollbar's thumb at rest and under the pointer.
    pub scrollbar: Option<Rgba>,
    pub scrollbar_hover: Option<Rgba>,
    /// The shadow a menu, popout or tooltip asks for.
    pub raised_shadow: Option<Shadow>,
    /// The shadow a card, dialog or profile popout asks for.
    pub card_shadow: Option<Shadow>,
    /// How opaque a region's ground is, as `opacity` names it. Only the
    /// ground is faded; the window's text keeps its own ink.
    pub rail_opacity: Option<f32>,
    pub sidebar_opacity: Option<f32>,
    pub chat_opacity: Option<f32>,
    pub panel_opacity: Option<f32>,
    pub raised_opacity: Option<f32>,
    pub field_opacity: Option<f32>,
    pub stage_opacity: Option<f32>,
    pub tooltip_opacity: Option<f32>,
    pub radius: Option<f32>,
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub letter_spacing: Option<f32>,
    /// A channel list's category heading, a member list's group heading and
    /// the conversation's title, as their own rules name them.
    pub category_size: Option<f32>,
    pub category_transform: Option<TextTransform>,
    pub member_heading_size: Option<f32>,
    pub member_heading_transform: Option<TextTransform>,
    pub header_size: Option<f32>,
    pub header_transform: Option<TextTransform>,
    /// How long popups open for, and in how many steps, from a `transition`
    /// or `animation` on a menu, popout or dialog.
    pub motion_ms: Option<u32>,
    pub motion_steps: Option<u32>,
}

impl NativeStyle {
    /// Matches `rules` against the native window and returns the values that
    /// win the cascade. `vars` are the root custom properties, already
    /// resolved for `dark`.
    pub fn compute(rules: &[Rule], vars: &Vars, dark: bool) -> Self {
        // The same region and property can be named by hundreds of rules.
        // Keep the winning declaration of each, and resolve only those
        // values: a theme's thousands of custom properties never reach
        // `Vars::resolve` or the cascade sort.
        type Key = (Role, bool, Pseudo, String);
        type Winner<'a> = ((bool, u32, usize, u64), Write<'a>);
        let mut winners: BTreeMap<Key, Winner<'_>> = BTreeMap::new();
        let mut sequence = 0u64;
        for (order, rule) in rules.iter().enumerate() {
            if rule
                .color_scheme
                .is_some_and(|color_scheme| color_scheme != dark)
            {
                continue;
            }
            // Most rules name Discord's DOM and match no native region; the
            // pseudo-element's own shape is only read for the rules that do.
            let mut thin = None;
            let mut opacity = None;
            for selector in &rule.selectors {
                let Some((role, hover, specificity, pseudo)) = match_selector(selector, dark)
                else {
                    continue;
                };
                let thin = *thin.get_or_insert_with(|| rule_thin(rule));
                let opacity = *opacity.get_or_insert_with(|| rule_opacity(rule));
                for declaration in &rule.declarations {
                    if !handled(&declaration.name) {
                        continue;
                    }
                    sequence += 1;
                    let write = Write {
                        important: declaration.important,
                        specificity,
                        order,
                        role,
                        hover,
                        pseudo,
                        thin,
                        opacity,
                        declaration,
                    };
                    let rank = (write.important, write.specificity, write.order, sequence);
                    let key = (
                        role,
                        hover,
                        pseudo,
                        declaration.name.trim().to_ascii_lowercase(),
                    );
                    let replace = winners.get(&key).is_none_or(|(old, _)| rank >= *old);
                    if replace {
                        winners.insert(key, (rank, write));
                    }
                }
            }
        }
        // Normal declarations first, then specificity, then source order;
        // `!important` declarations come after all of them.
        let mut writes: Vec<_> = winners.into_values().collect();
        writes.sort_by_key(|(rank, _)| *rank);

        let mut style = Self::default();
        for (_, write) in writes {
            let Some(value) = vars.resolve(&write.declaration.value) else {
                continue;
            };
            apply(&mut style, write, &value, dark);
        }
        style
    }
}

/// One declaration that matched, with the cascade keys it needs.
struct Write<'a> {
    important: bool,
    specificity: u32,
    order: usize,
    role: Role,
    hover: bool,
    pseudo: Pseudo,
    /// Whether a pseudo-element is a thin bar rather than an overlay.
    thin: bool,
    /// The pseudo-element's own `opacity`, which fades what it paints.
    opacity: Option<f32>,
    declaration: &'a Declaration,
}

/// Whether a native region paints this property at all. Everything else a
/// theme declares — its custom properties, layout, filters and Discord-only
/// fields — is left for `Vars` or for the file.
fn handled(property: &str) -> bool {
    const PAINTED: &[&str] = &[
        "background",
        "background-color",
        "background-image",
        "color",
        "border-radius",
        "font-family",
        "font-size",
        "letter-spacing",
        "text-transform",
        "opacity",
        "box-shadow",
        "transition",
        "transition-duration",
        "animation",
        "animation-duration",
        "border",
        "border-color",
        "border-top",
        "border-right",
        "border-bottom",
        "border-left",
        "border-top-color",
        "border-right-color",
        "border-bottom-color",
        "border-left-color",
        "outline",
        "outline-color",
        "accent-color",
        "caret-color",
    ];
    PAINTED
        .iter()
        .any(|painted| property.eq_ignore_ascii_case(painted))
}

fn apply(style: &mut NativeStyle, write: Write<'_>, value: &str, dark: bool) {
    // Property names are usually already lower case; only a theme that
    // shouts them pays for a new string.
    let name = write.declaration.name.trim();
    let property = if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Owned(name.to_ascii_lowercase())
    } else {
        Cow::Borrowed(name)
    };
    let property = property.as_ref();
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    let role = write.role;
    match write.pseudo {
        Pseudo::Ignored => return,
        Pseudo::Selection => {
            if matches!(
                property,
                "background" | "background-color" | "background-image"
            ) && let Some(color) = background_color(value, !dark)
                && color.a > 0
            {
                style.selection = Some(fade(color, write.opacity));
            }
            return;
        }
        Pseudo::Placeholder => {
            if property == "color"
                && let Some(color) = color_in(value, !dark)
                && color.a > 0
            {
                style.placeholder = Some(fade(color, write.opacity));
            }
            return;
        }
        Pseudo::Scrollbar | Pseudo::ScrollbarTrack => return,
        Pseudo::ScrollbarThumb | Pseudo::ScrollbarThumbHover => {
            if matches!(
                property,
                "background" | "background-color" | "background-image"
            ) && let Some(color) = background_color(value, !dark)
                && color.a > 0
            {
                let color = fade(color, write.opacity);
                if matches!(write.pseudo, Pseudo::ScrollbarThumbHover) {
                    style.scrollbar_hover = Some(color);
                } else {
                    style.scrollbar = Some(color);
                }
            }
            return;
        }
        Pseudo::Before | Pseudo::After => {
            // `body::before` is where a theme's wallpaper usually lives;
            // that path already gives the window its background picture.
            if role != Role::Root {
                apply_pseudo(style, &write, property, value, dark);
            }
            return;
        }
        Pseudo::None => {}
    }
    match property {
        "background" | "background-color" | "background-image" => {
            let gradient = crate::theme::gradient(value);
            let mut cleared = false;
            if let Some(color) = background_color(value, !dark) {
                // A transparent background on a collapsed wrapper lets the
                // region's own colour through; it must not erase the region in
                // the native window.
                if color.a > 0 {
                    set_background(style, role, write.hover, color);
                    cleared = true;
                }
            }
            match gradient {
                Some(gradient) => set_gradient(style, role, write.hover, gradient),
                None if cleared => clear_gradient(style, role, write.hover),
                None => {}
            }
        }
        "color" => {
            if let Some(color) = color_in(value, !dark) {
                set_text(style, role, color);
            }
        }
        "border-radius" => {
            // A radius on a region (not on a row, a button or a hover state)
            // can stand for the window's shape; one on a card or a button
            // must not redefine every surface.
            if !write.hover
                && matches!(
                    role,
                    Role::Root | Role::Rail | Role::Sidebar | Role::Chat | Role::Members
                )
                && let Some(radius) = length(value)
                && let Some(radius) = crate::css::bounds::within(radius, crate::css::bounds::RADIUS)
            {
                style.radius = Some(radius);
            }
        }
        "font-family" => {
            // Typography is one setting for the whole window; only a rule on
            // the page itself can name it. A header's font would otherwise
            // restyle every message.
            if matches!(role, Role::Root)
                && let Some(family) = family(value)
            {
                style.font_family = Some(family);
            }
        }
        "font-size" => {
            // A zero or negative size is not a length CSS accepts (the engine
            // settles a negative one to zero); an absurd one is held to the
            // largest size the window draws a heading at.
            if let Some(size) = length(value)
                && size > 0.0
                && let Some(size) = crate::css::bounds::within(size, crate::css::bounds::TEXT)
            {
                match role {
                    Role::Root => style.font_size = Some(size),
                    Role::Category => style.category_size = Some(size),
                    Role::MembersGroup => style.member_heading_size = Some(size),
                    Role::Header => style.header_size = Some(size),
                    _ => {}
                }
            }
        }
        "letter-spacing" => {
            if matches!(role, Role::Root)
                && let Some(spacing) = length(value)
                && let Some(spacing) =
                    crate::css::bounds::within(spacing, crate::css::bounds::TRACKING)
            {
                style.letter_spacing = Some(spacing);
            }
        }
        "text-transform" => {
            if let Some(transform) = text_transform(value) {
                match role {
                    Role::Category => style.category_transform = Some(transform),
                    Role::MembersGroup => style.member_heading_transform = Some(transform),
                    Role::Header => style.header_transform = Some(transform),
                    // A root rule that changes case would touch every word
                    // the window draws, including messages; it is not a
                    // region's decoration.
                    _ => {}
                }
            }
        }
        "opacity" => {
            if !write.hover
                && let Some(opacity) = fraction(value)
                && let Some(slot) = opacity_slot(style, role)
            {
                *slot = Some(opacity);
            }
        }
        "box-shadow" => {
            if let Some(shadow) = shadow(value, !dark) {
                set_shadow(style, role, shadow);
            }
        }
        "transition" | "transition-duration" | "animation" | "animation-duration" => {
            if matches!(
                role,
                Role::Card | Role::Menu | Role::ServerSettings | Role::Profile
            ) {
                apply_motion(style, property, value);
            }
        }
        "border"
        | "border-color"
        | "border-top"
        | "border-right"
        | "border-bottom"
        | "border-left"
        | "border-top-color"
        | "border-right-color"
        | "border-bottom-color"
        | "border-left-color"
        | "outline"
        | "outline-color" => {
            if let Some(color) = color_in(value, !dark) {
                style.line = Some(color);
            }
        }
        "accent-color" | "caret-color" => {
            if let Some(color) = color_in(value, !dark) {
                style.accent = Some(color);
            }
        }
        _ => {}
    }
}

/// A `::before` or `::after` box: a full overlay is its region's ground, a
/// thin strip is one of the window's lines.
fn apply_pseudo(
    style: &mut NativeStyle,
    write: &Write<'_>,
    property: &str,
    value: &str,
    dark: bool,
) {
    let role = write.role;
    match property {
        "background" | "background-color" | "background-image" => {
            if let Some(color) = background_color(value, !dark)
                && color.a > 0
            {
                let color = fade(color, write.opacity);
                if write.thin {
                    style.line = Some(color);
                } else {
                    set_background(style, role, false, color);
                }
            }
        }
        "border" | "border-color" | "border-top" | "border-right" | "border-bottom"
        | "border-left" | "outline" | "outline-color" => {
            if let Some(color) = color_in(value, !dark) {
                style.line = Some(fade(color, write.opacity));
            }
        }
        "box-shadow" => {
            if let Some(shadow) = shadow(value, !dark) {
                set_shadow(style, role, shadow);
            }
        }
        _ => {}
    }
}

pub(crate) fn opacity_slot(style: &mut NativeStyle, role: Role) -> Option<&mut Option<f32>> {
    match role {
        Role::Root | Role::Chat | Role::Messages | Role::SearchResults => {
            Some(&mut style.chat_opacity)
        }
        Role::Rail => Some(&mut style.rail_opacity),
        Role::Sidebar | Role::Category => Some(&mut style.sidebar_opacity),
        Role::Members | Role::Member | Role::MembersGroup | Role::Thread => {
            Some(&mut style.panel_opacity)
        }
        Role::Composer | Role::Uploads => Some(&mut style.field_opacity),
        Role::Stage => Some(&mut style.stage_opacity),
        Role::Tooltip => Some(&mut style.tooltip_opacity),
        Role::Header
        | Role::Forum
        | Role::ForumPost
        | Role::ServerSettings
        | Role::Inbox
        | Role::Profile
        | Role::Poll
        | Role::Card
        | Role::Menu => Some(&mut style.raised_opacity),
        // A message row's opacity would fade only the row's hover wash;
        // the native window keeps the row's own ink and ground.
        Role::Message => None,
    }
}

pub(crate) fn set_shadow(style: &mut NativeStyle, role: Role, shadow: Shadow) {
    match role {
        Role::Menu | Role::Tooltip => style.raised_shadow = Some(shadow),
        Role::Card | Role::Profile | Role::ServerSettings => style.card_shadow = Some(shadow),
        _ => {}
    }
}

pub(crate) fn fade(color: Rgba, opacity: Option<f32>) -> Rgba {
    match opacity {
        Some(opacity) => color.with_alpha((f32::from(color.a) * opacity.clamp(0.0, 1.0)) as u8),
        None => color,
    }
}

pub(crate) fn set_background(style: &mut NativeStyle, role: Role, hover: bool, color: Rgba) {
    let slot = if hover {
        match role {
            Role::Message | Role::Messages => &mut style.row_hover,
            _ => &mut style.hover,
        }
    } else {
        match role {
            Role::Root | Role::Chat | Role::Messages | Role::Forum | Role::SearchResults => {
                &mut style.chat
            }
            Role::Rail => &mut style.rail,
            Role::Sidebar => &mut style.sidebar,
            Role::Members | Role::Member | Role::MembersGroup | Role::Thread => &mut style.panel,
            Role::Header
            | Role::ForumPost
            | Role::ServerSettings
            | Role::Inbox
            | Role::Profile
            | Role::Uploads
            | Role::Poll
            | Role::Card
            | Role::Menu => &mut style.raised,
            Role::Composer => &mut style.field,
            Role::Stage => &mut style.stage,
            Role::Tooltip => &mut style.tooltip,
            Role::Category => &mut style.sidebar,
            Role::Message => &mut style.row_hover,
        }
    };
    *slot = Some(color);
}

/// Keeps a linear gradient for the region whose ground the window paints as
/// one. Other regions and hover washes keep their solid colour.
pub(crate) fn set_gradient(
    style: &mut NativeStyle,
    role: Role,
    hover: bool,
    gradient: crate::Gradient,
) {
    if hover || !matches!(gradient.kind, crate::GradientKind::Linear(_)) {
        return;
    }
    match role {
        Role::Rail => style.rail_gradient = Some(gradient),
        Role::Sidebar | Role::Category => style.sidebar_gradient = Some(gradient),
        Role::Chat | Role::Messages | Role::Forum | Role::SearchResults => {
            style.chat_gradient = Some(gradient);
        }
        // The page's own gradient is already the window's background
        // picture; the other regions have no gradient painter.
        _ => {}
    }
}

pub(crate) fn clear_gradient(style: &mut NativeStyle, role: Role, hover: bool) {
    if hover {
        return;
    }
    match role {
        Role::Rail => style.rail_gradient = None,
        Role::Sidebar | Role::Category => style.sidebar_gradient = None,
        Role::Chat | Role::Messages | Role::Forum | Role::SearchResults => {
            style.chat_gradient = None;
        }
        _ => {}
    }
}

pub(crate) fn set_text(style: &mut NativeStyle, role: Role, color: Rgba) {
    match role {
        Role::Sidebar => {
            style.secondary = Some(color);
        }
        Role::Category | Role::Members | Role::Member | Role::MembersGroup => {
            style.muted = Some(color);
        }
        _ => style.text = Some(color),
    }
}

pub(crate) fn background_color(value: &str, light: bool) -> Option<Rgba> {
    // A gradient is a real backdrop in the browser. The native window has
    // no gradient painter for every region, so it keeps the first opaque
    // stop as that region's colour rather than dropping the rule entirely.
    if value.contains("gradient(") {
        // A colour beside the gradient, as `#fff linear-gradient(...)`.
        for token in color::split_top(value, ' ') {
            if !token.contains("gradient(")
                && let Some(color) = parse_color(token, light)
                && color.a > 0
            {
                return Some(color);
            }
        }
        for part in color::split_top(value, ',') {
            let Some(open) = part.find('(') else {
                continue;
            };
            let Some(close) = part.rfind(')') else {
                continue;
            };
            for stop in color::split_top(&part[open + 1..close], ',') {
                for token in color::split_top(stop, ' ') {
                    if let Some(color) = parse_color(token, light)
                        && color.a > 0
                    {
                        return Some(color);
                    }
                }
            }
        }
        return None;
    }
    if value.contains("url(") {
        for token in color::split_top(value, ' ') {
            if let Some(color) = parse_color(token, light)
                && color.a > 0
            {
                return Some(color);
            }
        }
        return None;
    }
    color_in(value, light)
}

pub(crate) fn color_in(value: &str, light: bool) -> Option<Rgba> {
    if let Some(color) = parse_color(value, light) {
        return Some(color);
    }
    // A shorthand such as `#fff 1px solid` or `1px solid rgba(0, 0, 0, .4)`.
    for token in color::split_top(value, ' ') {
        if let Some(color) = parse_color(token, light) {
            return Some(color);
        }
    }
    None
}

fn length(value: &str) -> Option<f32> {
    let value = value.trim().trim_end_matches(';').trim();
    let first = color::split_top(value, ' ').into_iter().next()?;
    number(first).or_else(|| {
        first
            .trim()
            .strip_suffix('%')
            .and_then(|percent| percent.trim().parse::<f32>().ok())
            .map(|percent| percent / 100.0 * 16.0)
    })
}

/// One CSS length or number, in pixels where the unit says so.
fn number(token: &str) -> Option<f32> {
    let token = token.trim().trim_end_matches(';').trim();
    if token.is_empty() {
        return None;
    }
    let lower = token.to_ascii_lowercase();
    let parsed = |suffix: &str| {
        lower
            .strip_suffix(suffix)
            .and_then(|number| number.trim().parse::<f32>().ok())
    };
    parsed("px")
        .or_else(|| parsed("rem").map(|number| number * 16.0))
        .or_else(|| parsed("em").map(|number| number * 16.0))
        .or_else(|| lower.parse::<f32>().ok())
}

/// A fraction, from `0.5` or `50%`.
fn fraction(value: &str) -> Option<f32> {
    let value = value.trim().trim_end_matches(';').trim();
    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .trim()
            .parse::<f32>()
            .ok()
            .and_then(|percent| crate::css::bounds::opacity(percent / 100.0));
    }
    value
        .parse::<f32>()
        .ok()
        .and_then(crate::css::bounds::opacity)
}

pub(crate) fn text_transform(value: &str) -> Option<TextTransform> {
    match value.trim().to_ascii_lowercase().as_str() {
        "uppercase" => Some(TextTransform::Uppercase),
        "lowercase" => Some(TextTransform::Lowercase),
        "capitalize" => Some(TextTransform::Capitalize),
        "none" | "initial" | "inherit" | "unset" => Some(TextTransform::None),
        _ => None,
    }
}

/// The first non-`inset` shadow in a `box-shadow` value, in the order CSS
/// writes it: x, y, blur, spread, colour.
pub(crate) fn shadow(value: &str, light: bool) -> Option<Shadow> {
    for part in color::split_top(value, ',') {
        if part.to_ascii_lowercase().contains("inset") {
            continue;
        }
        let mut lengths = Vec::new();
        let mut color = None;
        for token in color::split_top(part, ' ') {
            if let Some(found) = parse_color(token, light) {
                color = Some(found);
            } else if let Some(length) = number(token) {
                lengths.push(length);
            }
        }
        if lengths.len() >= 2
            && let Some(offset_x) =
                crate::css::bounds::within(lengths[0], crate::css::bounds::SHADOW)
            && let Some(offset_y) =
                crate::css::bounds::within(lengths[1], crate::css::bounds::SHADOW)
        {
            return Some(Shadow {
                color: color.unwrap_or(Rgba::rgba(0, 0, 0, 0x80)),
                offset_x,
                offset_y,
                blur: lengths
                    .get(2)
                    .copied()
                    .and_then(|blur| {
                        crate::css::bounds::within(blur.max(0.0), crate::css::bounds::SHADOW)
                    })
                    .unwrap_or(0.0),
            });
        }
    }
    None
}

/// How long a popup takes and how many steps it moves in, from a theme's
/// `transition` or `animation` on a menu, popout or dialog.
fn apply_motion(style: &mut NativeStyle, property: &str, value: &str) {
    let lower = value.to_ascii_lowercase();
    if let Some(ms) = duration_of(&lower)
        && ms <= 2000
    {
        style.motion_ms = Some(ms);
    }
    if let Some(steps) = steps_of(&lower) {
        style.motion_steps = Some(steps);
    } else if matches!(property, "transition" | "animation")
        && (lower.contains("linear") || lower.contains("ease") || lower.contains("cubic-bezier("))
    {
        // A continuous curve: the native popup stops moving in ReCord's
        // discrete steps and follows the time instead.
        style.motion_steps = Some(0);
    }
}

/// The first time in a shorthand, in whole milliseconds.
fn duration_of(value: &str) -> Option<u32> {
    for token in color::split_top(value, ',') {
        for token in color::split_top(token, ' ') {
            let token = token.trim();
            if let Some(ms) = token.strip_suffix("ms")
                && let Ok(ms) = ms.trim().parse::<f32>()
            {
                return Some(ms.max(0.0).round() as u32);
            } else if let Some(seconds) = token.strip_suffix('s')
                && let Ok(seconds) = seconds.trim().parse::<f32>()
            {
                return Some((seconds.max(0.0) * 1000.0).round() as u32);
            }
        }
    }
    None
}

/// The `n` in a `steps(n, end)` timing function.
fn steps_of(value: &str) -> Option<u32> {
    let start = value.find("steps(")? + "steps(".len();
    let inside = &value[start..];
    let end = inside.find(')')?;
    inside[..end]
        .split(',')
        .next()?
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|steps| *steps > 0 && *steps <= 16)
}

/// Whether a rule draws a thin bar rather than an overlay over its region.
fn rule_thin(rule: &Rule) -> bool {
    rule.declarations.iter().any(|declaration| {
        matches!(
            declaration.name.trim().to_ascii_lowercase().as_str(),
            "height" | "width" | "max-height" | "max-width"
        ) && length(&declaration.value).is_some_and(|length| length > 0.0 && length <= 3.0)
    })
}

/// A pseudo-element's own opacity, which fades what it paints.
fn rule_opacity(rule: &Rule) -> Option<f32> {
    rule.declarations
        .iter()
        .find(|declaration| declaration.name.trim().eq_ignore_ascii_case("opacity"))
        .and_then(|declaration| fraction(&declaration.value))
}

fn family(value: &str) -> Option<String> {
    for family in color::split_top(value, ',') {
        let family = family.trim().trim_matches(['"', '\'']).trim();
        if family.is_empty() {
            continue;
        }
        if matches!(
            family.to_ascii_lowercase().as_str(),
            "inherit"
                | "initial"
                | "unset"
                | "revert"
                | "sans-serif"
                | "serif"
                | "monospace"
                | "system-ui"
        ) {
            continue;
        }
        return Some(family.to_owned());
    }
    None
}

/// Whether `selector` matches any native role, and whether it is a hover
/// rule. Every compound must match the role or one of its ancestors.
/// A pseudo-element a theme names. Only the ones a native window can paint
/// are kept: text selection, a field's hint, a scrollbar's thumb, and the
/// decoration a region's `::before` or `::after` draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Pseudo {
    None,
    Before,
    After,
    Selection,
    Placeholder,
    Scrollbar,
    ScrollbarTrack,
    ScrollbarThumb,
    ScrollbarThumbHover,
    Ignored,
}

/// Splits `selector::pseudo` into the selector and the pseudo-element.
fn split_pseudo(selector: &str) -> (&str, Pseudo) {
    let bytes = selector.as_bytes();
    let mut depth = 0u32;
    let mut quote = None;
    let mut index = 0;
    while index + 1 < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'"' | b'\'' if quote == Some(byte) => quote = None,
            b'"' | b'\'' if quote.is_none() => quote = Some(byte),
            b'(' | b'[' if quote.is_none() => depth += 1,
            b')' | b']' if quote.is_none() => depth = depth.saturating_sub(1),
            b':' if quote.is_none() && depth == 0 && bytes[index + 1] == b':' => {
                let base = &selector[..index];
                let after = &selector[index + 2..];
                let (name, rest) = take_identifier(after);
                let name = name.to_ascii_lowercase();
                let hover = rest.to_ascii_lowercase().contains(":hover");
                let pseudo = match name.as_str() {
                    "before" => Pseudo::Before,
                    "after" => Pseudo::After,
                    "selection" | "-moz-selection" => Pseudo::Selection,
                    "placeholder" | "-moz-placeholder" | "-webkit-input-placeholder" => {
                        Pseudo::Placeholder
                    }
                    "-webkit-scrollbar" => Pseudo::Scrollbar,
                    "-webkit-scrollbar-track" => Pseudo::ScrollbarTrack,
                    "-webkit-scrollbar-thumb" if hover => Pseudo::ScrollbarThumbHover,
                    "-webkit-scrollbar-thumb" => Pseudo::ScrollbarThumb,
                    _ => Pseudo::Ignored,
                };
                return (base, pseudo);
            }
            _ => {}
        }
        index += 1;
    }
    (selector, Pseudo::None)
}

/// The role a selector names when it also asks for a pointer or focus
/// state. The semantic pass creates hover/focus variants only for roles a
/// theme actually names, instead of doubling every region.
pub(crate) fn state_role(selector: &str, dark: bool) -> Option<Role> {
    let (base, pseudo) = split_pseudo(selector.trim());
    if !matches!(pseudo, Pseudo::None) {
        return None;
    }
    let lower = base.to_ascii_lowercase();
    if !(lower.contains(":hover") || lower.contains(":focus") || lower.contains(":active")) {
        return None;
    }
    match_selector(selector, dark).map(|(role, _, _, _)| role)
}

fn match_selector(selector: &str, dark: bool) -> Option<(Role, bool, u32, Pseudo)> {
    let (selector, pseudo) = split_pseudo(selector.trim());
    if matches!(pseudo, Pseudo::Ignored) {
        return None;
    }
    let selector = selector.trim();
    if selector.is_empty() {
        // `::selection`, `::placeholder` and `::-webkit-scrollbar-*` are
        // written without a subject; they style the whole window.
        return matches!(
            pseudo,
            Pseudo::Selection
                | Pseudo::Placeholder
                | Pseudo::Scrollbar
                | Pseudo::ScrollbarTrack
                | Pseudo::ScrollbarThumb
                | Pseudo::ScrollbarThumbHover
        )
        .then_some((Role::Root, false, 0, pseudo));
    }
    if selector.starts_with('@') {
        return None;
    }
    let parts = selector_parts(selector);
    let compounds: Vec<String> = parts.iter().map(|(_, compound)| compound.clone()).collect();
    if compounds.is_empty() {
        return None;
    }
    // A bare tag or `*` is too broad to name one native region: `div` would
    // otherwise paint the rail, and `*` the first region it meets. Root tags
    // and the few semantic elements Discord's window wraps its regions in are
    // safe; everything else needs a class, id or attribute to name it.
    let identified = compounds.iter().any(|compound| {
        compound.contains('.')
            || compound.contains('#')
            || compound.contains('[')
            || compound.starts_with(":root")
            || compound.starts_with(":where(html")
            || compound.starts_with(":where(:root")
            || compound.starts_with(":is(html")
            || compound.starts_with(":is(:root")
    });
    if !identified {
        // A window-wide pseudo-element may name the element it belongs to
        // with a bare tag: `input::placeholder`, `*::-webkit-scrollbar`.
        if matches!(
            pseudo,
            Pseudo::Selection
                | Pseudo::Placeholder
                | Pseudo::Scrollbar
                | Pseudo::ScrollbarTrack
                | Pseudo::ScrollbarThumb
                | Pseudo::ScrollbarThumbHover
        ) {
            return Some((Role::Root, false, specificity(&compounds), pseudo));
        }
        let last = compounds.last()?;
        let tag = last
            .trim()
            .trim_start_matches(':')
            .split(|character: char| !character.is_ascii_alphanumeric())
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(tag.as_str(), "html" | "body" | "main" | "aside" | "header") {
            return None;
        }
    }
    let specificity = specificity(&compounds);
    for (index, node) in NODES.iter().enumerate() {
        let mut current = index;
        let mut hover = false;
        let mut matched = true;
        let rightmost = parts.len() - 1;
        if !compound_matches(&parts[rightmost].1, current, dark, &mut hover) {
            continue;
        }
        for at in (0..rightmost).rev() {
            let (_, compound) = &parts[at];
            // The combinator belongs to the part to the right.
            let combinator = parts[at + 1].0;
            let mut found = None;
            match combinator {
                '>' => {
                    if let Some(parent) = NODES[current].parent
                        && compound_matches(compound, parent, dark, &mut hover)
                    {
                        found = Some(parent);
                    }
                }
                '+' => {
                    if let Some(sibling) = previous_sibling(current)
                        && compound_matches(compound, sibling, dark, &mut hover)
                    {
                        found = Some(sibling);
                    }
                }
                '~' => {
                    let mut sibling = previous_sibling(current);
                    while let Some(previous) = sibling {
                        if compound_matches(compound, previous, dark, &mut hover) {
                            found = Some(previous);
                            break;
                        }
                        sibling = previous_sibling(previous);
                    }
                }
                _ => {
                    let mut walk = NODES[current].parent;
                    while let Some(ancestor) = walk {
                        if compound_matches(compound, ancestor, dark, &mut hover) {
                            found = Some(ancestor);
                            break;
                        }
                        walk = NODES[ancestor].parent;
                    }
                    // A native region collapses several Discord wrappers
                    // into one node, so a wrapper compound may match the
                    // same node again.
                    if found.is_none()
                        && collapsed(node.role)
                        && compound_matches(compound, current, dark, &mut hover)
                    {
                        found = Some(current);
                    }
                }
            }
            let Some(next) = found else {
                matched = false;
                break;
            };
            // Other roles need a real relation, which keeps broad
            // descendant selectors from landing on the wrong region.
            if next == current && !collapsed(node.role) {
                matched = false;
                break;
            }
            current = next;
        }
        if matched {
            return Some((node.role, hover, specificity, pseudo));
        }
    }
    None
}

/// Roles whose synthetic node stands for several nested Discord wrappers.
fn collapsed(role: Role) -> bool {
    // `.message` and `.messageContent_` are one item in Discord's DOM, and
    // `html body` is one page; the other regions have real wrappers that a
    // selector's descendant compounds should match instead of collapsing.
    matches!(role, Role::Root | Role::Message)
}

/// A rough CSS specificity: ids, classes/attributes/pseudo-classes, tags.
fn specificity(compounds: &[String]) -> u32 {
    let mut specificity = 0;
    for compound in compounds {
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

/// Whether `child` is below `ancestor` in the synthetic tree.
fn is_descendant(child: usize, ancestor: usize) -> bool {
    let mut current = NODES[child].parent;
    while let Some(parent) = current {
        if parent == ancestor {
            return true;
        }
        current = NODES[parent].parent;
    }
    false
}

/// The node's 1-based position among its siblings, and how many there are.
/// The synthetic page is a root in its own right, so it counts as one.
fn sibling_position(node: usize) -> Option<(usize, usize)> {
    let Some(parent) = NODES[node].parent else {
        return Some((1, 1));
    };
    let siblings: Vec<usize> = (0..NODES.len())
        .filter(|index| NODES[*index].parent == Some(parent))
        .collect();
    let position = siblings.iter().position(|index| *index == node)? + 1;
    Some((position, siblings.len()))
}

/// The node's 1-based position among siblings that share one of its tags.
fn type_position(node: usize) -> Option<(usize, usize)> {
    let Some(parent) = NODES[node].parent else {
        return Some((1, 1));
    };
    let tags = NODES[node].tags;
    let same: Vec<usize> = (0..NODES.len())
        .filter(|index| {
            NODES[*index].parent == Some(parent)
                && tags
                    .iter()
                    .any(|tag| NODES[*index].tags.iter().any(|known| known == tag))
        })
        .collect();
    let position = same.iter().position(|index| *index == node)? + 1;
    Some((position, same.len()))
}

/// Whether a 1-based sibling position matches `odd`, `even`, `n`, `an+b`
/// or an integer, as `:nth-child()` writes it.
fn nth_matches(expression: &str, position: usize) -> bool {
    let expression = expression
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<String>();
    if expression == "odd" {
        return position % 2 == 1;
    }
    if expression == "even" {
        return position.is_multiple_of(2);
    }
    let Some((coefficient, offset)) = expression.split_once('n') else {
        return expression
            .parse::<i64>()
            .is_ok_and(|value| value == position as i64);
    };
    let coefficient = match coefficient {
        "" | "+" => 1,
        "-" => -1,
        value => match value.parse::<i64>() {
            Ok(value) => value,
            Err(_) => return false,
        },
    };
    let offset = match offset {
        "" => 0,
        value => match value.trim_start_matches('+').parse::<i64>() {
            Ok(value) => value,
            Err(_) => return false,
        },
    };
    if coefficient == 0 {
        return offset == position as i64;
    }
    let distance = position as i64 - offset;
    distance % coefficient == 0 && distance / coefficient >= 0
}

/// The previous sibling of a node in the synthetic tree.
fn previous_sibling(node: usize) -> Option<usize> {
    let parent = NODES[node].parent?;
    (0..node)
        .rev()
        .find(|index| NODES[*index].parent == Some(parent))
}

/// The selector split at its top-level combinators, rightmost last. The
/// character on each part is its combinator with the part before it; the
/// first part's is ignored.
fn selector_parts(selector: &str) -> Vec<(char, String)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut pending = ' ';
    let mut depth = 0u32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let characters = selector.chars();
    for character in characters {
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
            '(' | '[' if quote.is_none() => {
                depth += 1;
                current.push(character);
            }
            ')' | ']' if quote.is_none() => {
                depth = depth.saturating_sub(1);
                current.push(character);
            }
            '>' | '+' | '~' if quote.is_none() && depth == 0 => {
                if !current.trim().is_empty() {
                    parts.push((pending, current.trim().to_owned()));
                    current.clear();
                }
                pending = character;
            }
            character if character.is_whitespace() && quote.is_none() && depth == 0 => {
                if !current.trim().is_empty() {
                    parts.push((pending, current.trim().to_owned()));
                    current.clear();
                    pending = ' ';
                }
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        parts.push((pending, current.trim().to_owned()));
    }
    parts
}

fn compound_matches(compound: &str, node: usize, dark: bool, hover: &mut bool) -> bool {
    let mut rest = compound.trim();
    if rest.is_empty() {
        return false;
    }
    if rest == "*" {
        return true;
    }
    let mut saw_anything = false;

    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('.') {
            let (class, after) = take_identifier(after);
            if class.is_empty() {
                return false;
            }
            if !class_matches(node, class, dark) {
                return false;
            }
            rest = after;
            saw_anything = true;
            continue;
        }
        if let Some(after) = rest.strip_prefix('#') {
            let (id, after) = take_identifier(after);
            if NODES[node].id != Some(id) {
                return false;
            }
            rest = after;
            saw_anything = true;
            continue;
        }
        if rest.starts_with('[') {
            let Some(end) = rest.find(']') else {
                return false;
            };
            if !attribute_matches(&rest[1..end], node, dark) {
                return false;
            }
            rest = rest[end + 1..].trim_start();
            saw_anything = true;
            continue;
        }
        if rest.starts_with(':') {
            let after = &rest[1..];
            let (name, after) = take_identifier(after);
            if name.is_empty() {
                return false;
            }
            if let Some(after_paren) = skip_arguments(after) {
                let arguments = after_paren.0;
                rest = after_paren.1;
                match name.to_ascii_lowercase().as_str() {
                    "not" => {
                        let mut ignored = false;
                        let mut any = false;
                        for inner in split_selectors(arguments) {
                            if compound_matches(inner, node, dark, &mut ignored) {
                                any = true;
                                break;
                            }
                        }
                        if any {
                            return false;
                        }
                    }
                    "is" | "where" => {
                        let mut any = false;
                        for inner in split_selectors(arguments) {
                            let mut ignored = false;
                            if compound_matches(inner, node, dark, &mut ignored) {
                                any = true;
                                break;
                            }
                        }
                        if !any {
                            return false;
                        }
                    }
                    "has" => {
                        let mut any = false;
                        for inner in split_selectors(arguments) {
                            let inner = inner.trim().trim_start_matches('>').trim();
                            if inner.is_empty()
                                || inner.contains(' ')
                                || inner.contains('+')
                                || inner.contains('~')
                            {
                                continue;
                            }
                            for descendant in 0..NODES.len() {
                                if descendant == node || !is_descendant(descendant, node) {
                                    continue;
                                }
                                let mut ignored = false;
                                if compound_matches(inner, descendant, dark, &mut ignored) {
                                    any = true;
                                    break;
                                }
                            }
                            if any {
                                break;
                            }
                        }
                        if !any {
                            return false;
                        }
                    }
                    "nth-child" => {
                        if !sibling_position(node)
                            .is_some_and(|(position, _)| nth_matches(arguments, position))
                        {
                            return false;
                        }
                    }
                    "nth-last-child" => {
                        if !sibling_position(node).is_some_and(|(position, total)| {
                            nth_matches(arguments, total + 1 - position)
                        }) {
                            return false;
                        }
                    }
                    "nth-of-type" => {
                        if !type_position(node)
                            .is_some_and(|(position, _)| nth_matches(arguments, position))
                        {
                            return false;
                        }
                    }
                    "nth-last-of-type" => {
                        if !type_position(node).is_some_and(|(position, total)| {
                            nth_matches(arguments, total + 1 - position)
                        }) {
                            return false;
                        }
                    }
                    _ => return false,
                }
                saw_anything = true;
                continue;
            }
            match name.to_ascii_lowercase().as_str() {
                "root" => {
                    if NODES[node].role != Role::Root {
                        return false;
                    }
                }
                "hover" | "focus" | "focus-visible" | "focus-within" | "active" => {
                    *hover = true;
                }
                "first-child" => {
                    if !sibling_position(node).is_some_and(|(position, _)| position == 1) {
                        return false;
                    }
                }
                "last-child" => {
                    if !sibling_position(node).is_some_and(|(position, total)| position == total) {
                        return false;
                    }
                }
                "only-child" => {
                    if !sibling_position(node).is_some_and(|(_, total)| total == 1) {
                        return false;
                    }
                }
                "first-of-type" => {
                    if !type_position(node).is_some_and(|(position, _)| position == 1) {
                        return false;
                    }
                }
                "last-of-type" => {
                    if !type_position(node).is_some_and(|(position, total)| position == total) {
                        return false;
                    }
                }
                "only-of-type" => {
                    if !type_position(node).is_some_and(|(_, total)| total == 1) {
                        return false;
                    }
                }
                "empty" => {
                    if (0..NODES.len()).any(|child| NODES[child].parent == Some(node)) {
                        return false;
                    }
                }
                "link" | "visited" | "disabled" | "checked" => return false,
                _ => return false,
            }
            rest = after.trim_start();
            saw_anything = true;
            continue;
        }
        // A type selector: a tag name at the start of the compound.
        let (tag, after) = take_identifier(rest);
        if tag.is_empty() {
            return false;
        }
        if !NODES[node]
            .tags
            .iter()
            .any(|known| known.eq_ignore_ascii_case(tag))
        {
            return false;
        }
        rest = after.trim_start();
        saw_anything = true;
    }
    saw_anything
}

fn class_matches(node: usize, class: &str, dark: bool) -> bool {
    if let Some(theme) = theme_class(class) {
        return NODES[node].role == Role::Root && theme == dark;
    }
    NODES[node]
        .classes
        .iter()
        .any(|known| class_prefix_matches(known, class))
}

/// Whether a selector class names one of a node's stable class fragments.
/// Discord hashes the suffix per build (`chat_z_gUBg`, `chat-2ZfjoI`) and
/// changes the separator between releases; the stem is what themes keep.
pub(crate) fn class_prefix_matches(known: &str, class: &str) -> bool {
    if class.eq_ignore_ascii_case(known) {
        return true;
    }
    let Some(stem) = known.strip_suffix('_') else {
        return false;
    };
    !stem.is_empty()
        && class.len() > stem.len()
        && class
            .get(..stem.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(stem))
        && matches!(class.as_bytes().get(stem.len()), Some(b'_' | b'-'))
}

fn theme_class(class: &str) -> Option<bool> {
    match class.to_ascii_lowercase().as_str() {
        "theme-dark" | "theme-darker" | "theme-midnight" | "theme-onxy" | "theme-ash" => Some(true),
        "theme-light" => Some(false),
        _ => None,
    }
}

fn attribute_matches(attribute: &str, node: usize, dark: bool) -> bool {
    let attribute = attribute.trim();
    let (name, operator, expected) = split_attribute(attribute);
    let name = name.trim().to_ascii_lowercase();
    let expected = expected.trim().trim_matches(['"', '\'']);
    let (actual, any) = match name.as_str() {
        "class" => (
            if NODES[node].role == Role::Root {
                format!(
                    "app app_ appMount appMount_ theme-{}",
                    if dark { "dark" } else { "light" }
                )
            } else {
                NODES[node].classes.join(" ")
            },
            true,
        ),
        "id" => (NODES[node].id.unwrap_or("").to_owned(), false),
        "role" => (NODES[node].aria_role.unwrap_or("").to_owned(), false),
        "data-theme" => (
            if NODES[node].role == Role::Root {
                if dark { "dark" } else { "light" }.to_owned()
            } else {
                String::new()
            },
            false,
        ),
        _ => return false,
    };
    let values: Vec<&str> = if any {
        actual.split_whitespace().collect()
    } else {
        vec![actual.as_str()]
    };
    match operator {
        "=" => {
            if any {
                actual.eq_ignore_ascii_case(expected)
            } else {
                values
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(expected))
            }
        }
        "~=" => {
            any && values
                .iter()
                .any(|value| value.eq_ignore_ascii_case(expected))
        }
        "|=" => values.iter().any(|value| {
            value.eq_ignore_ascii_case(expected) || value.starts_with(&format!("{expected}-"))
        }),
        "^=" => values.iter().any(|value| {
            value
                .to_ascii_lowercase()
                .starts_with(&expected.to_ascii_lowercase())
        }),
        "$=" => values.iter().any(|value| {
            value
                .to_ascii_lowercase()
                .ends_with(&expected.to_ascii_lowercase())
        }),
        "*=" => values.iter().any(|value| {
            value
                .to_ascii_lowercase()
                .contains(&expected.to_ascii_lowercase())
        }),
        _ => false,
    }
}

fn split_attribute(attribute: &str) -> (&str, &str, &str) {
    for operator in ["~=", "|=", "^=", "$=", "*=", "="] {
        if let Some(index) = attribute.find(operator) {
            return (
                &attribute[..index],
                operator,
                &attribute[index + operator.len()..],
            );
        }
    }
    (attribute, "", "")
}

fn take_identifier(text: &str) -> (&str, &str) {
    let mut end = 0;
    for (index, character) in text.char_indices() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '\\') {
            end = index + character.len_utf8();
        } else {
            break;
        }
    }
    (&text[..end], &text[end..])
}

/// Skips a balanced `(...)` after a pseudo-class and returns its inside and
/// what follows it.
fn skip_arguments(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    let inside = text.strip_prefix('(')?;
    let mut depth = 1u32;
    let mut quote = None;
    let bytes = inside.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        match byte {
            b'"' | b'\'' if quote == Some(byte) => quote = None,
            b'"' | b'\'' if quote.is_none() => quote = Some(byte),
            b'(' if quote.is_none() => depth += 1,
            b')' if quote.is_none() => {
                depth -= 1;
                if depth == 0 {
                    return Some((&inside[..index], &inside[index + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

fn split_selectors(text: &str) -> Vec<&str> {
    color::split_top(text, ',')
        .into_iter()
        .map(str::trim)
        .filter(|selector| !selector.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mode, Options, load_str};

    fn dark(css: &str) -> crate::ThemeFile {
        load_str(css, None, &Options::with_mode(Mode::Dark)).expect("theme")
    }

    #[test]
    fn a_sidebar_gradient_is_kept_for_the_window() {
        let theme = dark(
            r#"
            .theme-dark [class*="sidebarList_"] {
                background: linear-gradient(180deg, #111111, #222222);
            }
            "#,
        );
        let gradient = theme.native.sidebar_gradient.expect("a gradient");
        assert_eq!(gradient.kind, crate::GradientKind::Linear(180.0));
        assert_eq!(gradient.stops.len(), 2);
    }

    #[test]
    fn a_sidebar_rule_reaches_the_native_sidebar() {
        let theme = dark(
            r#"
            :root { --panel: #112233; }
            .theme-dark [class*="sidebarList_"] {
                background: var(--panel);
                color: #abcdef;
                border: 1px solid #123456;
                border-radius: 3px;
            }
            "#,
        );
        assert_eq!(theme.native.sidebar, Some(Rgba::rgb(0x11, 0x22, 0x33)));
        assert_eq!(theme.native.secondary, Some(Rgba::rgb(0xab, 0xcd, 0xef)));
        assert_eq!(theme.native.line, Some(Rgba::rgb(0x12, 0x34, 0x56)));
        assert_eq!(theme.look.radius, Some(3.0));
    }

    #[test]
    fn a_gradient_keeps_its_first_opaque_stop() {
        let theme = dark(
            r#"
            .theme-dark [class*="chat_"] {
                background: linear-gradient(180deg, #112233 0%, #445566 100%);
            }
            "#,
        );
        assert_eq!(theme.native.chat, Some(Rgba::rgb(0x11, 0x22, 0x33)));
    }

    #[test]
    fn has_matches_a_synthetic_descendant() {
        let theme = dark(
            r#"
            .theme-dark:root:has(.guilds_) { background: #102030; }
            "#,
        );
        assert_eq!(theme.native.chat, Some(Rgba::rgb(0x10, 0x20, 0x30)));
    }

    #[test]
    fn the_light_and_dark_themes_choose_their_own_rules() {
        let theme = dark(
            r#"
            .theme-light [class*="guilds_"] { background: #ffffff; }
            .theme-dark [class*="guilds_"] { background: #010203; }
            "#,
        );
        assert_eq!(theme.native.rail, Some(Rgba::rgb(0x01, 0x02, 0x03)));
    }

    #[test]
    fn roles_map_to_the_regions_they_name() {
        let theme = dark(
            r#"
            [class*="guilds_"] { background-color: #010203; }
            [class*="chat_"] { background: #040506; }
            [class*="membersWrap_"] { background: #070809; }
            [class*="channelTextArea_"] { background: #0a0b0c; }
            [class*="message_"]:hover { background: #0d0e0f; }
            "#,
        );
        assert_eq!(theme.native.rail, Some(Rgba::rgb(1, 2, 3)));
        assert_eq!(theme.native.chat, Some(Rgba::rgb(4, 5, 6)));
        assert_eq!(theme.native.panel, Some(Rgba::rgb(7, 8, 9)));
        assert_eq!(theme.native.field, Some(Rgba::rgb(10, 11, 12)));
        assert_eq!(theme.native.row_hover, Some(Rgba::rgb(13, 14, 15)));
    }

    #[test]
    fn specificity_and_source_order_pick_the_winner() {
        let theme = dark(
            r#"
            .theme-dark [class*="sidebar"] { background: #111111; }
            #app-mount [class*="sidebar"] { background: #222222; }
            .theme-dark [class*="sidebar"] { background: #333333; }
            "#,
        );
        assert_eq!(theme.native.sidebar, Some(Rgba::rgb(0x22, 0x22, 0x22)));
    }

    #[test]
    fn hashed_discord_classes_keep_their_stable_prefix() {
        let theme = dark(
            r#"
            .chat_z_gUBg { background: #123456; }
            .bg_Fd0FEH { background: #654321; }
            "#,
        );
        // The page's ground is set first, then the chat's own rule wins.
        assert_eq!(theme.native.chat, Some(Rgba::rgb(0x12, 0x34, 0x56)));
        let background = dark(".bg_Fd0FEH { background: #654321; }");
        assert_eq!(background.native.chat, Some(Rgba::rgb(0x65, 0x43, 0x21)));
        let old = dark(".chat-2ZfjoI { background: #abcdef; }");
        assert_eq!(old.native.chat, Some(Rgba::rgb(0xab, 0xcd, 0xef)));
    }

    #[test]
    fn structural_pseudos_and_siblings_follow_the_synthetic_tree() {
        let second = dark(
            r#"
            [class*="messages_"]:nth-child(2) { background: #111111; }
            "#,
        );
        assert_eq!(second.native.chat, Some(Rgba::rgb(0x11, 0x11, 0x11)));
        let first = dark(
            r#"
            [class*="messages_"]:first-child { background: #222222; }
            "#,
        );
        assert_eq!(first.native.chat, None);
        let adjacent = dark(
            r#"
            [class*="header_"] + [class*="messages_"] { background: #333333; }
            "#,
        );
        assert_eq!(adjacent.native.chat, Some(Rgba::rgb(0x33, 0x33, 0x33)));
        let general = dark(
            r#"
            [class*="header_"] ~ [class*="composer"] { background: #444444; }
            "#,
        );
        assert_eq!(general.native.field, Some(Rgba::rgb(0x44, 0x44, 0x44)));
    }

    #[test]
    fn a_collapsed_message_wrapper_matches_both_compounds() {
        let theme = dark(
            r#"
            .theme-dark .message .messageContent { color: #123456; }
            "#,
        );
        assert_eq!(theme.native.text, Some(Rgba::rgb(0x12, 0x34, 0x56)));
    }

    #[test]
    fn variables_and_extra_properties_resolve() {
        let theme = dark(
            r#"
            :root { --line: #445566; --font: "Pixel Operator", monospace; }
            body {
                font-family: var(--font);
                font-size: 18px;
                letter-spacing: 1px;
            }
            "#,
        );
        assert_eq!(theme.native.font_family.as_deref(), Some("Pixel Operator"));
        assert_eq!(theme.look.font_size, Some(18.0));
        assert_eq!(theme.look.letter_spacing, Some(1.0));
    }

    #[test]
    fn regions_added_since_the_first_mapping_reach_their_surfaces() {
        let theme = dark(
            r#"
            [class*="serverSettings_"] { background: #010101; }
            [class*="threadSidebar_"] { background: #020202; }
            [class*="forum_"] { background: #030303; }
            [class*="forumPost_"] { background: #040404; }
            [class*="stage_"] { background: #050505; }
            [class*="inbox_"] { background: #060606; }
            [class*="profileBanner_"] { background: #070707; }
            [class*="membersGroup_"] { color: #080808; }
            [class*="upload_"] { background: #090909; }
            [class*="searchResults_"] { background: #0a0a0a; }
            [class*="poll_"] { background: #0b0b0b; }
            [role="menu"] { background: #0c0c0c; }
            [role="tooltip"] { background: #0d0d0d; }
            "#,
        );
        assert_eq!(theme.native.raised, Some(Rgba::rgb(0x0c, 0x0c, 0x0c)));
        assert_eq!(theme.native.panel, Some(Rgba::rgb(2, 2, 2)));
        assert_eq!(theme.native.chat, Some(Rgba::rgb(0x0a, 0x0a, 0x0a)));
        assert_eq!(theme.native.stage, Some(Rgba::rgb(5, 5, 5)));
        assert_eq!(theme.native.muted, Some(Rgba::rgb(8, 8, 8)));
        assert_eq!(theme.native.field, None, "uploads are raised, not fields");
        assert_eq!(theme.native.tooltip, Some(Rgba::rgb(13, 13, 13)));
    }

    #[test]
    fn a_forum_post_and_a_poll_are_raised_surfaces() {
        let theme = dark(
            r#"
            [class*="forumPost_"] { background: #111111; }
            "#,
        );
        assert_eq!(theme.native.raised, Some(Rgba::rgb(0x11, 0x11, 0x11)));
        let theme = dark(
            r#"
            [class*="poll_"] { background: #222222; }
            "#,
        );
        assert_eq!(theme.native.raised, Some(Rgba::rgb(0x22, 0x22, 0x22)));
    }

    #[test]
    fn a_search_result_panel_is_the_chat_ground() {
        let theme = dark(
            r#"
            [class*="searchResults_"] { background: #333333; }
            "#,
        );
        assert_eq!(theme.native.chat, Some(Rgba::rgb(0x33, 0x33, 0x33)));
    }

    #[test]
    fn selection_and_placeholder_have_their_own_inks() {
        let theme = dark(
            r#"
            ::selection { background: #112233; }
            input::placeholder { color: #445566; }
            "#,
        );
        assert_eq!(theme.native.selection, Some(Rgba::rgb(0x11, 0x22, 0x33)));
        assert_eq!(theme.native.placeholder, Some(Rgba::rgb(0x44, 0x55, 0x66)));
    }

    #[test]
    fn a_scrollbars_thumb_and_its_hover_are_kept() {
        let theme = dark(
            r#"
            ::-webkit-scrollbar { width: 8px; }
            ::-webkit-scrollbar-thumb { background: #111111; border-radius: 4px; }
            ::-webkit-scrollbar-thumb:hover { background: #222222; }
            ::-webkit-scrollbar-track { background: #333333; }
            "#,
        );
        assert_eq!(theme.native.scrollbar, Some(Rgba::rgb(0x11, 0x11, 0x11)));
        assert_eq!(
            theme.native.scrollbar_hover,
            Some(Rgba::rgb(0x22, 0x22, 0x22))
        );
    }

    #[test]
    fn a_pseudo_elements_overlay_or_thin_bar_becomes_a_native_surface() {
        let theme = dark(
            r#"
            [class*="chat_"]::after {
                content: "";
                position: absolute;
                inset: 0;
                background: #102030;
            }
            [class*="channelTextArea_"]::before {
                content: "";
                height: 2px;
                background: #405060;
            }
            "#,
        );
        assert_eq!(theme.native.chat, Some(Rgba::rgb(0x10, 0x20, 0x30)));
        assert_eq!(theme.native.line, Some(Rgba::rgb(0x40, 0x50, 0x60)));
    }

    #[test]
    fn a_fixed_pseudo_decoration_does_not_become_the_regions_ground() {
        let theme = dark(
            r#"
            [class*="sidebar_"]::before {
                content: "";
                position: absolute;
                left: 0;
                right: 0;
                bottom: 52px;
                height: 220px;
                background: #e2342a;
            }
            [class*="sidebar_"]::after {
                content: "CHANNELS";
                position: absolute;
                inset: 0;
                background: #e2342a;
            }
            "#,
        );
        assert_eq!(theme.native.sidebar, None);
    }

    #[test]
    fn a_pseudo_elements_opacity_fades_its_own_paint() {
        let theme = dark(
            r#"
            [class*="chat_"]::before {
                content: "";
                inset: 0;
                background: #ffffff;
                opacity: 0.25;
            }
            "#,
        );
        let chat = theme.native.chat.expect("the pseudo-element's ground");
        assert_eq!((chat.r, chat.g, chat.b), (0xff, 0xff, 0xff));
        assert!((f32::from(chat.a) / 255.0 - 0.25).abs() < 0.01);
    }

    #[test]
    fn a_regions_opacity_fades_its_ground() {
        let theme = dark(
            r#"
            [class*="sidebar_"] { opacity: 0.5; }
            [class*="membersWrap_"] { opacity: 0.75; }
            "#,
        );
        assert_eq!(theme.native.sidebar_opacity, Some(0.5));
        assert_eq!(theme.native.panel_opacity, Some(0.75));
    }

    #[test]
    fn hostile_numbers_reach_the_window_within_usable_ranges() {
        let theme = dark(
            r#"
            [class*="category_"] { font-size: 0; }
            [class*="membersGroup_"] { font-size: -12px; }
            [class*="header_"] { font-size: 99999px; }
            [class*="sidebar_"] { border-radius: 99999px; }
            [class*="membersWrap_"] { opacity: 2; }
            [class*="menu_"] { box-shadow: 1e9px 4px 1e9px #ff0000; }
            "#,
        );
        // A zero or negative size is not a length CSS accepts, so the window
        // keeps its own; an absurd one is held to the range it draws in.
        assert_eq!(theme.native.category_size, None);
        assert_eq!(theme.native.member_heading_size, None);
        assert_eq!(theme.native.header_size, Some(crate::css::bounds::TEXT.1));
        assert_eq!(theme.native.radius, Some(crate::css::bounds::RADIUS.1));
        // The engine drops an opacity outside 0..1 as invalid, so the region
        // keeps the window's own; a well-formed one is kept.
        assert_eq!(theme.native.sidebar_opacity, None);
        assert_eq!(
            dark(r#"[class*="membersWrap_"] { opacity: 0.9; }"#)
                .native
                .panel_opacity,
            Some(0.9)
        );
        let shadow = theme.native.raised_shadow.expect("the menu's shadow");
        assert!(shadow.offset_x <= crate::css::bounds::SHADOW.1);
        assert!(shadow.blur <= crate::css::bounds::SHADOW.1);
        assert!(shadow.offset_y.abs() <= crate::css::bounds::SHADOW.1);
    }

    #[test]
    fn hostile_root_variables_reach_the_look_within_usable_ranges() {
        let theme = dark(
            r#"
            :root {
                --font-size: -20px;
                --rl-tracking: 9999px;
                --radius-md: -50px;
                --gap: -10px;
            }
            "#,
        );
        assert_eq!(theme.look.font_size, None);
        assert_eq!(
            theme.look.letter_spacing,
            Some(crate::css::bounds::TRACKING.1)
        );
        assert_eq!(theme.look.radius, Some(0.0));
        assert_eq!(theme.look.panel_gap, Some(0.0));

        let huge = dark(
            r#"
            :root { --font-size: 99999px; --rl-gap: 1e30px; }
            body { display: flex; gap: 1e9px; }
            "#,
        );
        assert_eq!(huge.look.font_size, Some(crate::css::bounds::TEXT.1));
        // `1e30px` is not a number an `f32` holds, so it is no length at all,
        // and the page's own gap decides.
        assert_eq!(huge.look.panel_gap, Some(crate::css::bounds::GAP.1));
    }

    #[test]
    fn a_box_shadow_keeps_its_offset_blur_and_colour() {
        let theme = dark(
            r#"
            [class*="menu_"] {
                box-shadow: 0 4px 12px rgba(0, 0, 0, 0.6);
            }
            [class*="userPopout_"] {
                box-shadow: 0 8px 30px #00000080;
            }
            "#,
        );
        let raised = theme.native.raised_shadow.expect("the menu's shadow");
        assert_eq!(raised.offset_y, 4.0);
        assert_eq!(raised.blur, 12.0);
        let card = theme.native.card_shadow.expect("the popout's shadow");
        assert_eq!(card.offset_y, 8.0);
        assert_eq!(card.blur, 30.0);
        assert_eq!(card.color.a, 0x80);
    }

    #[test]
    fn a_popups_transition_sets_the_native_motions_pace() {
        let theme = dark(
            r#"
            [class*="menu_"] { transition: opacity 240ms ease; }
            "#,
        );
        assert_eq!(theme.native.motion_ms, Some(240));
        assert_eq!(theme.native.motion_steps, Some(0), "a continuous curve");

        let theme = dark(
            r#"
            [class*="modal_"] { animation: pop 0.3s steps(6); }
            "#,
        );
        assert_eq!(theme.native.motion_ms, Some(300));
        assert_eq!(theme.native.motion_steps, Some(6));
    }

    #[test]
    fn a_headings_size_and_case_are_kept_for_its_own_line() {
        let theme = dark(
            r#"
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
        );
        assert_eq!(theme.native.category_size, Some(11.0));
        assert_eq!(
            theme.native.category_transform,
            Some(TextTransform::Uppercase)
        );
        assert_eq!(theme.native.member_heading_size, Some(12.0));
        assert_eq!(
            theme.native.member_heading_transform,
            Some(TextTransform::Lowercase)
        );
        assert_eq!(theme.native.header_size, Some(15.0));
        assert_eq!(
            theme.native.header_transform,
            Some(TextTransform::Capitalize)
        );
    }

    #[test]
    fn capitalize_changes_only_the_first_letter_of_each_word() {
        assert_eq!(
            TextTransform::Capitalize.apply("general chat"),
            "General Chat"
        );
        assert_eq!(TextTransform::Uppercase.apply("Gen"), "GEN");
        assert_eq!(TextTransform::None.apply("Gen"), "Gen");
    }
}
