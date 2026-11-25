//! The layers the view stacks: the window itself, then whatever is open
//! over it (the profile card, mention suggestions, the emoji picker, a
//! menu, the modal), each with a backdrop that takes a click outside; and
//! the theme it's drawn in.

use iced::{
    Background, Border, ContentFit, Element, Length, Point, Rectangle, Size,
    widget::{Space, column, container, image, row, stack, text},
};

use crate::{
    controls::outside,
    fonts,
    picker::{self, Target},
    theme::Theme,
    widgets::{Anchor, Place},
};

use super::{
    App, Message,
    layers::{Layer, Popup, Popup::Layer as L},
};

/// The zoom level's note, this far below the window's top or its bar.
const ZOOM_NOTE_TOP: f32 = 12.0;

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        // Whatever this returns, what it no longer shows goes at its end.
        let _viewing = self.win.pictures.view();
        crate::widgets::motion::set_moving(self.themes.motion() != crate::themes::Motion::Off);
        let pace = self
            .themes
            .motion_pace()
            .unwrap_or((crate::widgets::motion::DURATION, 4));
        crate::widgets::motion::set_pace(pace.0, pace.1);
        if !crate::signin::workspace(self) {
            return self.frame_layers(crate::signin::view(self.ctx()));
        }
        // The window's own background is the theme's chat (`window_style`).
        // The rail and the sidebar on the rail's ground, the account's card
        // across the foot of both.
        let rail = self.theme.rail;
        let sidebar = self.theme.sidebar;
        let navigation_gradient = self.theme.navigation_gradient;
        let mut foot = column![];
        if let Some(dock) = crate::call::dock::view(self.ctx()) {
            foot = foot.push(dock);
        }
        // The server rail keeps its own ground; the channel list and the
        // account panel keep the sidebar's, which is where a theme's
        // navigation gradient belongs.
        let rail = container(crate::rail::view(&self.rail, self.ctx()))
            .width(crate::rail::WIDTH)
            .height(Length::Fill)
            .style(move |_| crate::style::fill(rail));
        let navigation = container(column![
            row![rail, crate::sidebar::view(&self.sidebar, self.ctx())].height(Length::Fill),
            foot.push(crate::account::view(self.ctx())),
        ])
        .width(crate::conversation::navigation(&self.layout))
        .height(Length::Fill)
        .style(move |_| {
            let mut ground = crate::style::fill(sidebar);
            if let Some(gradient) = navigation_gradient {
                ground.background = Some(gradient.into());
            }
            ground
        });
        // The sidebar's edge drags it wider or narrower.
        let splitter = crate::widgets::Splitter::new(
            iced::Color::TRANSPARENT,
            self.theme.line,
            self.theme.faint,
            |by| Message::Side(crate::sidebar::Event::Resize(by)),
            Message::Side(crate::sidebar::Event::Resized),
            Message::Side(crate::sidebar::Event::ResetWidth),
        );
        let navigation = stack![
            navigation,
            container(splitter)
                .width(crate::conversation::navigation(&self.layout))
                .align_x(iced::Alignment::End)
        ];
        let chat = self.theme.chat;
        let chat_gradient = self.theme.chat_gradient;
        let conversation = crate::conversation::conversation(self);
        // Ryoku, Midnight and their kin separate the rail, the chat and the
        // member list into rounded panels with a gap between them. A theme
        // that names no gap or radius keeps ReCore's full-bleed layout.
        let panel_gap = self.theme.panel_gap;
        let panel_radius = self.theme.panel_radius;
        let base: Element<'_, Message> = if panel_gap > 0.0 || panel_radius > 0.0 {
            let frame = self.theme.frame;
            let line = self.theme.line;
            let navigation = container(navigation)
                .clip(true)
                .style(move |_| Self::panel_style(None, line, panel_radius));
            let conversation = container(conversation).clip(true).style(move |_| {
                let mut ground = crate::style::fill(chat);
                if let Some(gradient) = chat_gradient {
                    ground.background = Some(gradient.into());
                }
                ground.border = Border {
                    color: line,
                    width: 1.0,
                    radius: panel_radius.into(),
                };
                ground
            });
            container(
                row![navigation, conversation]
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .spacing(panel_gap),
            )
            .padding(panel_gap)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| crate::style::fill(frame))
            .into()
        } else {
            container(row![navigation, conversation])
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| {
                    let mut ground = crate::style::fill(chat);
                    if let Some(gradient) = chat_gradient {
                        ground.background = Some(gradient.into());
                    }
                    ground
                })
                .into()
        };
        // A background picture sits under the panels; a GIF, APNG, WebP or
        // video plays through the window's picture players, its first frame
        // standing in until the first one falls due. A theme that keeps the
        // panels opaque hides it, as ReCord's panel opacity does.
        let base: Element<'_, Message> = match &self.themes.background_picture {
            Some(picture) => {
                // A moving background is the most expensive thing the window
                // draws. It rests while the window is unfocused, as the CRT
                // beam and flicker do; the first frame remains, so returning
                // to the window starts the motion where it left off.
                let animate = self.themes.recore.background_animate
                    && self.shell.focused
                    && self.themes.motion() != crate::themes::Motion::Off
                    && !self.themes.low_power;
                let fit = self.themes.background_fit();
                let content_fit = match fit {
                    recore_css::BackgroundFit::Cover => ContentFit::Cover,
                    recore_css::BackgroundFit::Contain => ContentFit::Contain,
                    recore_css::BackgroundFit::Fill => ContentFit::Fill,
                };
                let picture: Element<'_, Message> = if animate {
                    let size = Size::new(self.window.width, self.window.height);
                    let at = Rectangle::with_size(size);
                    self.win
                        .pictures
                        .animated(picture, size, at, fit != recore_css::BackgroundFit::Contain)
                        .still(self.themes.background.clone())
                        .grade(self.themes.background_grade(&self.theme))
                        // The renderer scales the decoded frame to the
                        // window; fitting it here copied every pixel on the
                        // processor for every frame.
                        .stretch(fit != recore_css::BackgroundFit::Contain)
                        .into()
                } else {
                    match &self.themes.background {
                        Some(still) => image(still.clone())
                            .content_fit(content_fit)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .into(),
                        None => Space::new().into(),
                    }
                };
                // The dim is part of the grade now, so no flat black
                // rectangle covers the picture.
                stack![picture, base]
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
            None => base,
        };
        // A CSS theme's page paint sits under the native panels; the panels
        // and the frame's gaps decide how much of it shows.
        let base: Element<'_, Message> = match &self.themes.paint {
            Some(paint) if self.themes.background_picture.is_none() => stack![
                image(paint.clone())
                    .content_fit(ContentFit::Fill)
                    .width(Length::Fill)
                    .height(Length::Fill),
                base
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
            _ => base,
        };
        // After the boot signal, the screen unfolds from its power-on line.
        let base: Element<'_, Message> = match self.reveal {
            Some(start) if self.themes.motion() == crate::themes::Motion::Full => {
                crate::widgets::motion::power_on(base, start)
            }
            _ => base,
        };
        // Always a stack, so the window's widget state stays as overlays
        // come and go: the card, the suggestions, the picker, a menu, the
        // modal, each over the last.
        let mut layers = stack![base];
        if let Some(toast) = crate::toast::view(self) {
            layers = layers.push(toast);
        }
        // ReCord's screen-in when the open room changes: over the room,
        // under the overlays and the title bar. It joins the window's one
        // stack rather than wrapping the room, which would give the rail,
        // the sidebar and the timeline a new tree and lose where they
        // were scrolled to.
        if let Some(switch) = self.switch_view() {
            layers = layers.push(switch);
        }
        // A role's card, under the card of someone on it.
        if let Some(role) = crate::info_cards::role_layers(&self.cards, &self.profiles, self.ctx())
        {
            layers = layers.extend(
                role.into_iter()
                    .map(|layer| self.popup(L(Layer::RoleCard), layer)),
            );
        }
        if let Some(card) = crate::profile::card_view(self.ctx()) {
            layers = layers
                .push(outside(
                    Message::Profile(crate::profile::Event::Close),
                    false,
                ))
                .push(self.popup(L(Layer::Card), card));
        }
        // The picker for a dialog's emoji opens over the dialog.
        let mut over_dialogs = None;
        if let Some(picker) = self.win.picker.get() {
            let shown = picker::view(picker, self.ctx());
            let size = picker.size();
            let place = match &picker.target {
                // Over the emoji button, its right edge level with the button's.
                Target::Composer => {
                    let right = crate::conversation::chat_right(self) - 8.0 - 4.0;
                    let bottom = crate::composer::composer_buttons_top(self) - 8.0;
                    Place::new(shown, Point::new(right - size.width, bottom), 8.0)
                        .anchor(Anchor::Bottom)
                }
                // Over the edit's field, at the timeline's right.
                Target::Edit => {
                    let right = crate::conversation::chat_right(self) - 16.0;
                    let bottom = crate::edit::edit_top(self)
                        .unwrap_or(crate::composer::composer_top(self))
                        - 8.0;
                    Place::new(shown, Point::new(right - size.width, bottom), 8.0)
                        .anchor(Anchor::Bottom)
                }
                Target::React(_, at) | Target::Status(at) | Target::Poll(_, at) => {
                    Place::new(shown, *at, 8.0).anchor(Anchor::Pointer)
                }
            };
            if matches!(picker.target, Target::Status(_) | Target::Poll(..)) {
                over_dialogs = Some(place);
            } else {
                layers = layers
                    .push(outside(Message::Picker(picker::Event::Close), false))
                    .push(place);
            }
        }
        for layer in crate::call::dm::call_layers(self) {
            layers = layers.push(layer);
        }
        if let Some(viewer) = crate::call::corner::clip_viewer(self) {
            layers = layers.push(viewer);
        }
        if let Some(popup) = crate::call::popups::call_popup_view(self.ctx()) {
            layers = layers
                .push(outside(Message::Call(crate::call::Event::Close), false))
                .push(popup);
        }
        // Open, or closed and drawn going, as the tag's card below.
        if let Some(settings) = self.win.settings.get().or(self.win.settings.leaving()) {
            layers = layers.push(self.popup(
                L(Layer::Settings),
                crate::settings::view(settings, self.ctx()),
            ));
        }
        if let Some(settings) = self
            .server_screen
            .get()
            .and_then(|screen| crate::server_settings::view(screen, self.ctx()))
        {
            layers = layers.push(self.popup(L(Layer::ServerSettings), settings));
        }
        if let Some(settings) = self
            .channel_screen
            .get()
            .and_then(|screen| crate::channel_settings::view(screen, self.ctx()))
        {
            layers = layers.push(self.popup(L(Layer::ChannelSettings), settings));
        }
        // Over the settings, as a member's from Members.
        if let Some(full) = crate::profile::full_profile_view(self.ctx()) {
            layers = layers.push(self.popup(Popup::FullProfile, full));
        }
        if let Some(editor) = crate::profile::editor::profile_editor_view(self.ctx()) {
            layers = layers.push(self.popup(L(Layer::ProfileEditor), editor));
        }
        // A mutual server's, over the profile that lists it.
        if let Some(server) =
            crate::info_cards::server_layers(&self.cards, &self.profiles, self.ctx())
        {
            layers = layers.extend(
                server
                    .into_iter()
                    .map(|layer| self.popup(L(Layer::ServerCard), layer)),
            );
        }
        if let Some(expression) =
            crate::info_cards::expression_layers(&self.cards, &self.profiles, self.ctx())
        {
            layers = layers.extend(
                expression
                    .into_iter()
                    .map(|layer| self.popup(L(Layer::ExpressionCard), layer)),
            );
        }
        // A server tag's, over the profile or the messages it's on.
        if let Some(card) = self.win.tag_card.get().or(self.win.tag_card.leaving()) {
            layers = layers.extend(
                crate::tag_card::layers(card, self.ctx())
                    .into_iter()
                    .map(|layer| self.popup(L(Layer::TagCard), layer)),
            );
        }
        if let Some(popover) = crate::status::popover_view(&self.status, self.ctx()) {
            layers = layers
                .push(outside(Message::Status(crate::status::Event::Close), false))
                .push(popover);
        }
        if let Some(dialog) = crate::status::dialog_view(&self.status, self.ctx()) {
            layers = layers.push(self.popup(L(Layer::StatusEditor), dialog));
        }
        for (layer, dialog) in [
            (
                Layer::NewDm,
                crate::switcher::new_dm_view(&self.quick, self.ctx()),
            ),
            (
                Layer::Poll,
                crate::switcher::poll_view(&self.quick, self.ctx()),
            ),
            (
                Layer::Switcher,
                crate::switcher::switcher_view(&self.quick, self.ctx()),
            ),
            (
                Layer::ServerDialog,
                crate::server_dialogs::view(&self.server_dialogs, self.ctx()),
            ),
            (
                Layer::LinkDialog,
                crate::link_dialogs::view(&self.link_dialogs, self.ctx()),
            ),
        ] {
            if let Some(dialog) = dialog {
                layers = layers.push(self.popup(L(layer), dialog));
            }
        }
        if let Some(place) = over_dialogs {
            layers = layers
                .push(outside(Message::Picker(picker::Event::Close), false))
                .push(place);
        }
        if let Some(popout) = crate::search_popout::view(self.ctx()) {
            layers = layers.push(popout);
        }
        if let Some(dialog) = crate::search_filters::view(self.ctx()) {
            layers = layers.push(self.popup(L(Layer::SearchFilters), dialog));
        }
        if let Some(popover) =
            crate::header_popovers::view(&self.popovers, &self.header.name, self.ctx())
        {
            layers = layers.push(self.popup(L(Layer::HeaderPopover), popover));
        }
        if let Some(modal) = crate::bot_modal::view(&self.modal, self.ctx()) {
            layers = layers.push(modal);
        }
        if let Some(translation) = crate::translate::view(self) {
            layers = layers.push(translation);
        }
        if let Some(viewer) = crate::image_viewer::view(&self.viewer, self.ctx()) {
            layers = layers.push(viewer);
        }
        if let Some(menu) = self
            .win
            .menu
            .as_ref()
            .and_then(|menu| crate::menus::view(menu, self.ctx()))
        {
            layers = layers
                .push(outside(Message::Menu(crate::menus::Event::Close), false))
                .push(menu);
        }
        if let Some(window) = crate::reactions::view(&self.reactions, self.ctx()) {
            layers = layers.push(self.popup(L(Layer::Reactions), window));
        }
        if let Some(modal) = crate::dialogs::delete_view(self.confirm_focus, self.ctx()) {
            layers = layers.push(modal);
        }
        if let Some(report) = crate::dialogs::report_view(self.ctx()) {
            layers = layers.push(report);
        }
        if let Some(bulk) = crate::dialogs::bulk_delete_view(self.ctx()) {
            layers = layers.push(bulk);
        }
        if let Some(folder) = crate::dialogs::folder_view(self.ctx()) {
            layers = layers.push(folder);
        }
        if let Some(dialog) = crate::person_menu::view(&self.person, self.ctx()) {
            layers = layers.push(self.popup(L(Layer::PersonDialog), dialog));
        }
        if let Some(dialog) = crate::friends::add_friend_view(self.ctx()) {
            layers = layers.push(self.popup(L(Layer::AddFriend), dialog));
        }
        if let Some(forward) = crate::forward::view(&self.forward, self.ctx()) {
            layers = layers.push(self.popup(L(Layer::Forwarding), forward));
        }
        if let Some(leaving) = crate::dialogs::leaving_view(self.leaving.as_deref(), self.ctx()) {
            layers = layers.push(self.popup(L(Layer::Leaving), leaving));
        }
        if let Some(drop) = crate::uploads::drop_view(self.ctx()) {
            layers = layers.push(drop);
        }
        if let Some(prompt) = crate::mfa::view(&self.mfa, self.ctx()) {
            layers = layers.push(self.popup(L(Layer::Mfa), prompt));
        }
        if let Some(sheet) = crate::shortcuts::view(&self.shortcuts, self.ctx()) {
            layers = layers.push(sheet);
        }
        // While the keys are on the messages a press says so, for one off
        // the list to take them (`timeline::keys::follow_press`).
        let content = crate::widgets::RightPress::spot(
            layers.width(Length::Fill).height(Length::Fill),
            &self.pressed,
        )
        .telling(self.keyed.is_some().then_some(Message::Nothing));
        // A middle click scrolls the list under it towards the pointer.
        let theme = &self.theme;
        let arrow = |icon| {
            self.look()
                .icon(icon, 10, crate::theme::rgb(theme.secondary))
        };
        let content = crate::widgets::Autoscroll::new(
            content,
            &[crate::timeline::TIMELINE, crate::thread::LIST],
            crate::widgets::AutoscrollLook {
                ground: theme.raised,
                edge: theme.line,
                dot: theme.secondary,
                up: arrow(crate::pictures::Icon::ChevronUp),
                down: arrow(crate::pictures::Icon::ChevronDown),
            },
        );
        self.frame_layers(content)
    }

    /// The window's content under the title bar, when ReCore draws one,
    /// where it takes the window's place (`self.window` is its size), and
    /// over it the zoom level's note, the inbox, the bar and the edges that
    /// resize the window. Ctrl and the wheel zoom over all of it.
    fn frame_layers<'a>(
        &'a self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let mut layers = stack![crate::widgets::Below::new(
            content,
            crate::window_bar::bar_height(self)
        )];
        if let Some(note) = self.zoom_note_view() {
            layers = layers.push(note);
        }
        if let Some(inbox) = crate::window_bar::inbox_view(self) {
            layers = layers.push(inbox);
        }
        if let Some(bar) = crate::window_bar::window_bar(self) {
            layers = layers.push(column![bar, Space::new().height(Length::Fill)]);
        }
        if let Some(edges) = crate::window_bar::resize_edges(self) {
            layers = layers.push(edges);
        }
        // ReCord's boot signal: the mark fills in, the bar steps across,
        // and the tip's underscore blinks over everything while the
        // window comes up.
        if let Some(boot) = self.boot_view() {
            layers = layers.push(boot);
        }
        if let Some(reveal) = self.reveal_view() {
            layers = layers.push(reveal);
        }
        // A CSS theme's root/body animation: a quiet moving wash over the
        // window. Like the CRT layer it takes no pointer events.
        if self.shell.focused
            && !self.themes.low_power
            && self.themes.motion() == crate::themes::Motion::Full
            && self.themes.has_animations()
        {
            layers = layers.push(
                iced::widget::Canvas::new(crate::css_animation::Ground {
                    animations: self.themes.animations().to_vec(),
                    started: self.css_clock,
                })
                .width(Length::Fill)
                .height(Length::Fill),
            );
        }
        // ReCord's CRT layer over the whole window. Its canvas takes no
        // pointer events, so everything under it behaves as if it weren't
        // there.
        if let Some(mut shade) = self.themes.shade(&self.theme) {
            // The beam and flicker rest while the window is not the one
            // in use, as the background picture does: an inactive window
            // draws nothing.
            if !self.shell.focused {
                shade.beam = 0.0;
                shade.flicker = 0.0;
            }
            layers = layers.push(
                iced::widget::Canvas::new(shade)
                    .width(Length::Fill)
                    .height(Length::Fill),
            );
        }
        let frame =
            crate::widgets::CtrlWheel::new(layers, zoom_turn, recore_core::zoom::Wheel::reset);
        // What the overlays are placed by is the size iced lays out, which a
        // display change can alter without a window resize event.
        iced::widget::sensor(frame)
            .on_resize(|size| Message::Shell(crate::shell::Event::Resized(size)))
            .into()
    }

    /// ReCord's `rl-screen-in`: the new room's content fades back from the
    /// theme's ground in five steps.
    fn switch_view(&self) -> Option<Element<'_, Message>> {
        let start = self.switched?;
        let elapsed = start.elapsed();
        if elapsed >= std::time::Duration::from_millis(260)
            || self.themes.motion() != crate::themes::Motion::Full
        {
            return None;
        }
        let t = elapsed.as_secs_f32() / 0.260;
        let step = (t * 5.0).floor() / 5.0;
        let alpha = if step < 0.3 {
            1.0 - step / 0.3
        } else if step < 0.6 {
            0.14
        } else {
            0.0
        };
        let ground = self.theme.chat;
        Some(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| crate::style::fill(iced::Color { a: alpha, ..ground }))
                .into(),
        )
    }

    /// ReCord's boot screen: a centered mark growing in four steps, a
    /// stepped loading bar, a tip and its blinking line.
    fn boot_view(&self) -> Option<Element<'_, Message>> {
        let start = self.boot?;
        let elapsed = start.elapsed();
        if elapsed >= crate::app::BOOT {
            return None;
        }
        let theme = &self.theme;
        let whole = crate::app::BOOT.as_secs_f32();
        let progress = (elapsed.as_secs_f32() / (whole * 0.86)).clamp(0.0, 1.0);
        let stepped = (progress * 12.0).floor() / 12.0;
        let mark = 72.0 + 56.0 * ((progress * 4.0).floor() / 4.0).min(1.0);
        let caret = (elapsed.as_millis() / 450) % 2 == 1;
        let dim = theme.sunken;
        let accent = theme.accent;
        let bone = theme.text;
        let secondary = theme.secondary;
        let track = container(Space::new())
            .width(220)
            .height(8)
            .style(move |_| container::Style {
                background: Some(dim.into()),
                border: iced::Border {
                    color: accent,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            });
        let filled = container(Space::new())
            .width(Length::Fixed(220.0 * stepped))
            .height(8)
            .style(move |_| crate::style::fill(accent));
        let tip = if progress < 0.45 {
            "CONNECTING THE TERMINAL"
        } else if progress < 0.86 {
            "LOADING THE RECORD"
        } else {
            "SIGNAL LOCKED"
        };
        Some(
            container(
                column![
                    container(self.look().glyph(
                        crate::pictures::Icon::Logo,
                        mark.round() as u32,
                        accent,
                    ))
                    .center(128),
                    Space::new().height(30),
                    stack![track, filled],
                    Space::new().height(26),
                    text(tip).font(fonts::BOLD()).size(12).color(accent),
                    Space::new().height(10),
                    row![
                        text("// RECORE").font(fonts::BOLD()).size(12).color(bone),
                        text(if caret { "_" } else { " " })
                            .font(fonts::BOLD())
                            .size(12)
                            .color(accent),
                    ],
                    Space::new().height(34),
                    text(format!("v{}", self.build.version))
                        .font(fonts::MEDIUM())
                        .size(11)
                        .color(secondary),
                ]
                .align_x(iced::Alignment::Center),
            )
            .center(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| crate::style::fill(theme.chat))
            .into(),
        )
    }

    /// A rounded panel with no ground of its own, so its child's rail or
    /// chat colour shows through, clipped to the theme's corners.
    fn panel_style(
        background: Option<Background>,
        line: iced::Color,
        radius: f32,
    ) -> iced::widget::container::Style {
        iced::widget::container::Style {
            background,
            border: Border {
                color: line,
                width: 1.0,
                radius: radius.into(),
            },
            ..Default::default()
        }
    }

    /// The screen-in after the boot signal: black over the window, stepping
    /// off in five ReCord frames.
    fn reveal_view(&self) -> Option<Element<'_, Message>> {
        let start = self.reveal?;
        let elapsed = start.elapsed();
        if elapsed >= crate::app::REVEAL || self.themes.motion() != crate::themes::Motion::Full {
            return None;
        }
        let t = elapsed.as_secs_f32() / crate::app::REVEAL.as_secs_f32();
        let alpha = (1.0 - (t * 5.0).floor() / 5.0).clamp(0.0, 1.0);
        Some(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| crate::style::fill(iced::Color::from_rgba(0.0, 0.0, 0.0, alpha)))
                .into(),
        )
    }

    /// The zoom level, a moment after it changes.
    fn zoom_note_view(&self) -> Option<Element<'_, Message>> {
        self.shell.zoom_note?;
        let theme = &self.theme;
        let tip = theme.tooltip;
        let note = container(
            iced::widget::text(format!("{:.0}%", self.shell.zoom * 100.0))
                .size(15)
                .font(fonts::BOLD())
                .color(theme.text),
        )
        .padding([0, 14])
        .height(36)
        .align_y(iced::Alignment::Center)
        .style(move |_| iced::widget::container::Style {
            shadow: crate::style::raised_shadow(&self.theme),
            ..crate::style::rounded(tip, 18.0)
        });
        Some(
            column![
                Space::new().height(crate::window_bar::bar_height(self) + ZOOM_NOTE_TOP),
                container(note).center_x(Length::Fill)
            ]
            .into(),
        )
    }

    pub fn window_style(&self, _: &iced::Theme) -> iced::theme::Style {
        iced::theme::Style {
            background_color: self.theme.chat,
            text_color: self.theme.text,
        }
    }

    pub fn theme(&self) -> iced::Theme {
        self.theme.iced()
    }

    /// Shows the window in another theme, built in or the person's own, and
    /// keeps the choice, as Appearance does. Returns whether there's such a
    /// theme. Pictures stay as they are, save avatars that a new grade may
    /// colour; what the client drew in the old colours (icons, dots, the
    /// card's name) is drawn again, and the member list's styled names take
    /// the theme's palette.
    pub fn choose_theme(&mut self, id: &str) -> bool {
        let Some(theme) = self.themes.find(id) else {
            return false;
        };
        self.themes.choose(self.client.root(), &theme.id);
        self.show_theme(theme);
        true
    }

    /// Shows the window in `theme` without keeping it, as the theme
    /// editor shows what's edited.
    pub fn show_theme(&mut self, theme: Theme) {
        self.layout.theme_sidebar_width = theme.sidebar_width;
        self.layout.theme_members_width = theme.members_width;
        self.win.pictures.clear_filtered();
        if theme == self.theme {
            return;
        }
        let old = std::mem::replace(&mut self.theme, theme);
        self.win.pictures.marks.borrow_mut().clear();
        self.win.icons.forget();
        self.show_roster(false);
        // Text laid out in the old theme's inks: the rows' again now, the
        // rest as each is next taken in.
        crate::timeline::recolour(self, &old);
        self.header.topic = None;
        self.popovers.forget_laid();
        crate::search::forget_found(self);
        crate::forum::forget_guidelines(self);
        self.drafts.starter = None;
    }
}

/// What a turn of the wheel with Ctrl held comes to: the zoom's step,
/// as `zoom::Wheel` makes it of a notch or 60 px of a touchpad.
fn zoom_turn(
    wheel: &mut recore_core::zoom::Wheel,
    delta: iced::mouse::ScrollDelta,
) -> Option<Message> {
    let step = match delta {
        iced::mouse::ScrollDelta::Lines { y, .. } => wheel.lines(f64::from(y)),
        iced::mouse::ScrollDelta::Pixels { y, .. } => wheel.pixels(f64::from(y)),
    };
    step.map(|step| Message::Shell(crate::shell::Event::ZoomStep(step)))
}
