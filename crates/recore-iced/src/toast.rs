//! A notification's toast inside the window: who wrote where and what, over
//! the top right corner for a few seconds after the sound plays, so a
//! notification heard from another room can be read without leaving what's
//! in front. A press opens its room; another toast replaces it.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use iced::{
    Alignment, Border, Element, Length, Padding, Task,
    widget::{button, column, container},
};
use recore_core::chat::Alert;

use crate::app::{App, Message};

/// How wide it is, and how long it stays.
const WIDTH: f32 = 360.0;
const LINGER: Duration = Duration::from_secs(6);

/// The count that tells an old toast's timer from the toast now showing.
static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct Toast {
    id: u64,
    /// Where a press goes.
    action: Action,
    title: String,
    body: String,
    mention: bool,
}

enum Action {
    /// A conversation of the core's, by room id.
    Room(String),
    /// Settings' About, where an update is installed.
    About,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// Its time ran out; the count says which toast.
    Done(u64),
    /// A press on it: its room opens.
    Open(u64),
}

/// Shows `alert` over the window and arms its timer. The body follows
/// Messages' notification text switch, as the desktop's notification does.
pub fn show(app: &mut App, alert: Alert) -> Task<Message> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let body = if app.layout.prefs.notification_text.unwrap_or(true) {
        alert.body
    } else {
        "New message".to_owned()
    };
    app.shell.toast = Some(Toast {
        id,
        action: Action::Room(alert.room),
        title: alert.title,
        body,
        mention: alert.mention,
    });
    crate::after(LINGER, Message::Toast(Event::Done(id)))
}

/// Shows a newer release's toast: a press opens About, where it's installed.
pub fn update_available(app: &mut App, tag: &str) -> Task<Message> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    app.shell.toast = Some(Toast {
        id,
        action: Action::About,
        title: format!("ReCore {tag} is available"),
        body: "Open About to install it.".into(),
        mention: true,
    });
    crate::after(LINGER, Message::Toast(Event::Done(id)))
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Done(id) => {
            if app.shell.toast.as_ref().is_some_and(|toast| toast.id == id) {
                app.shell.toast = None;
            }
            Task::none()
        }
        Event::Open(id) => {
            let Some(toast) = app.shell.toast.take_if(|toast| toast.id == id) else {
                return Task::none();
            };
            match toast.action {
                Action::Room(room) => {
                    app.client
                        .dispatch(recore_core::application::Action::SelectRoom(room));
                    app.sync()
                }
                Action::About => app.open_settings(crate::settings::Section::About),
            }
        }
    }
}

/// The toast over the window's top right corner, while one shows.
pub fn view(app: &App) -> Option<Element<'_, Message>> {
    let toast = app.shell.toast.as_ref()?;
    let theme = &app.theme;
    let stripe = if toast.mention {
        theme.accent
    } else {
        theme.line
    };
    let mut body = column![
        crate::elide::elide(toast.title.as_str())
            .size(14)
            .font(crate::fonts::BOLD())
            .color(theme.text)
    ];
    if !toast.body.is_empty() {
        body = body.push(
            crate::elide::elide(toast.body.as_str())
                .size(13)
                .color(theme.secondary),
        );
    }
    let card = button(
        container(
            iced::widget::row![
                container(iced::widget::Space::new())
                    .width(3)
                    .height(Length::Fill)
                    .style(move |_| crate::style::fill(stripe)),
                container(body)
                    .width(Length::Fill)
                    .padding(Padding {
                        top: 10.0,
                        bottom: 10.0,
                        left: 12.0,
                        right: 12.0,
                    })
                    .clip(true),
            ]
            .height(Length::Shrink),
        )
        .width(WIDTH),
    )
    .padding(0)
    .width(WIDTH)
    .on_press(Message::Toast(Event::Open(toast.id)))
    .style(move |_, status| button::Style {
        background: Some(
            if crate::controls::lights(status) {
                theme.wash(0.06)
            } else {
                theme.raised
            }
            .into(),
        ),
        border: Border {
            color: theme.line,
            width: 1.0,
            radius: if theme.square { 0.0 } else { 8.0 }.into(),
        },
        shadow: iced::Shadow {
            color: crate::media::scrim(theme, 0.55),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 18.0,
        },
        ..button::Style::default()
    });
    Some(
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding {
                top: 12.0,
                right: 12.0,
                ..Padding::ZERO
            })
            .align_x(Alignment::End)
            .align_y(Alignment::Start)
            .into(),
    )
}
