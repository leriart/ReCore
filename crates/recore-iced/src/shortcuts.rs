//! The keyboard's map, from Help: a card over the window with the app's
//! shortcuts in sections. Escape, its close button or a click outside
//! closes it, and nothing under it takes a key while it is open.

use iced::{
    Alignment, Border, Element, Length, Task,
    widget::{column, container, row, text},
};

use crate::{
    app::{App, Message},
    ctx::Ctx,
    dialog::dialog,
    fonts::{BOLD, MEDIUM},
    pictures::Icon,
    style::scrollbar,
    widgets::scroll_view,
};

/// The card's width, and the room each of its two columns gets.
const WIDTH: f32 = 600.0;
/// The width of a shortcut's keys, so its words line up.
const KEYS: f32 = 112.0;

const NAVIGATION: &[(&[&str], &str)] = &[
    (&["Ctrl", "K"], "Quick switcher"),
    (&["Alt", "←"], "Back"),
    (&["Alt", "→"], "Forward"),
];
const SEARCH: &[(&[&str], &str)] = &[
    (&["Ctrl", "F"], "Search here"),
    (&["Ctrl", "Shift", "F"], "Search the server"),
];
const MESSAGES: &[(&[&str], &str)] = &[
    (&["Alt", "↑"], "Step into messages"),
    (&["↑", "↓"], "Move between them"),
    (&["R"], "Reply"),
    (&["E"], "Edit"),
    (&["Delete"], "Delete"),
    (&["Esc"], "Back to the composer"),
];
const WRITING: &[(&[&str], &str)] = &[
    (&["Enter"], "Send"),
    (&["Shift", "Enter"], "New line"),
    (&["↑"], "Edit last message"),
    (&["Ctrl", "E"], "Emoji picker"),
    (&["Ctrl", "G"], "GIF picker"),
];
const WINDOW: &[(&[&str], &str)] = &[
    (&["Ctrl", "+"], "Zoom in"),
    (&["Ctrl", "−"], "Zoom out"),
    (&["Ctrl", "0"], "Reset the zoom"),
    (&["Ctrl", "wheel"], "Zoom"),
];

/// Whether the card is open.
#[derive(Default)]
pub struct Shortcuts {
    pub open: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum Event {
    Close,
}

/// The Help button's: the card opens, or goes if it is open.
pub fn toggle(app: &mut App) -> Task<Message> {
    app.shortcuts.open = !app.shortcuts.open;
    if app.shortcuts.open {
        app.inbox = None;
    }
    Task::none()
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Close => app.shortcuts.open = false,
    }
    Task::none()
}

/// Escape's, and the card's close button's.
pub fn close(app: &mut App) -> Task<Message> {
    app.shortcuts.open = false;
    Task::none()
}

/// The card, while it is open.
pub fn view<'a>(shortcuts: &Shortcuts, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    if !shortcuts.open {
        return None;
    }
    let theme = cx.look.theme;
    let close = Message::Shortcuts(Event::Close);
    let head = row![
        container(cx.look.glyph(Icon::Keyboard, 20, theme.accent)).center(28),
        text("Keyboard Shortcuts")
            .size(18)
            .font(BOLD())
            .color(theme.text)
            .wrapping(text::Wrapping::None)
            .width(Length::Fill),
        cx.look.icon_button(Icon::X, "Close", close.clone()).below(),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let subtitle = text("Every shortcut the window answers to, by what it does.")
        .size(13)
        .color(theme.secondary)
        .wrapping(text::Wrapping::None);
    let left = column![
        section(theme, "Navigation", NAVIGATION),
        section(theme, "Search", SEARCH),
        section(theme, "Messages", MESSAGES),
    ]
    .spacing(18)
    .width(Length::Fill);
    let right = column![
        section(theme, "Writing", WRITING),
        section(theme, "Window", WINDOW),
    ]
    .spacing(18)
    .width(Length::Fill);
    let columns = row![left, right]
        .spacing(24)
        .align_y(Alignment::Start)
        .width(Length::Fill);
    // A short window scrolls the sections instead of letting the card
    // grow past its edges.
    let columns = scroll_view(columns)
        .height(Length::Fixed((cx.look.window.height - 220.0).max(200.0)))
        .style(scrollbar(theme));
    let more = text(
        "While stepping through messages: Enter opens the menu, + reacts, C copies, \
         F forwards, and Alt+Enter marks unread.",
    )
    .size(12)
    .color(theme.muted)
    .wrapping(text::Wrapping::Word);
    let body = column![head, subtitle, columns, more]
        .spacing(14)
        .padding(20);
    Some(dialog(theme, body.into(), WIDTH, close))
}

/// A section: its name, then its shortcuts.
fn section<'a>(
    theme: &'a crate::theme::Theme,
    name: &'a str,
    rows: &'static [(&'static [&'static str], &'static str)],
) -> Element<'a, Message> {
    let mut list = column![text(name).size(11).font(BOLD()).color(theme.muted)].spacing(7);
    for &(keys, what) in rows {
        list = list.push(shortcut(theme, keys, what));
    }
    list.into()
}

/// A shortcut's keys, then what they do.
fn shortcut<'a>(
    theme: &'a crate::theme::Theme,
    keys: &'static [&'static str],
    what: &'static str,
) -> Element<'a, Message> {
    let mut chords = row![].spacing(4);
    for &key in keys {
        chords = chords.push(keycap(theme, key));
    }
    row![
        chords.width(Length::Fixed(KEYS)),
        text(what)
            .size(13)
            .color(theme.text)
            .wrapping(text::Wrapping::None),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// One key of a shortcut.
fn keycap<'a>(theme: &'a crate::theme::Theme, key: &'static str) -> Element<'a, Message> {
    let ground = theme.sunken;
    let line = theme.edge;
    container(
        text(key)
            .size(11)
            .font(MEDIUM())
            .color(theme.text)
            .wrapping(text::Wrapping::None),
    )
    .padding([2, 6])
    .style(move |_| container::Style {
        background: Some(ground.into()),
        border: Border {
            color: line,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    })
    .into()
}
