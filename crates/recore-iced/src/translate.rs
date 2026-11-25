//! A message's text in another language: the Translate row of its menu,
//! Google Translate's answer, and the card that shows it.
//!
//! The request goes from the window, not the core: translation is not
//! Discord's, and a message's text is sent only when someone asks for it.
//! Google's endpoint is the public one Discord's web client uses, without
//! an account or a key; a refused or unreachable request says so in the
//! card and is never retried on its own.

use std::time::Duration;

use iced::{
    Element, Length, Padding, Task,
    widget::{column, container, row, text},
};
use serde_json::Value;

use crate::{
    app::{App, Message},
    dialog::dialog,
    fonts::BOLD,
    pictures::Icon,
    widgets::scroll_view,
};

/// The languages Settings offers, and the codes Google takes: a name for
/// the picker, and the card's heading.
pub const LANGUAGES: [(&str, &str); 12] = [
    ("en", "English"),
    ("es", "Spanish"),
    ("pt", "Portuguese"),
    ("fr", "French"),
    ("de", "German"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("zh-CN", "Chinese (Simplified)"),
    ("ru", "Russian"),
    ("ar", "Arabic"),
];

/// The card's width, and the most of a translation it shows before it
/// scrolls.
const WIDTH: f32 = 460.0;
const LONGEST: f32 = 420.0;
const WAIT: Duration = Duration::from_secs(15);

/// The translation shown, and the count that tells an answer for a card
/// that has since closed or changed.
#[derive(Default)]
pub struct Translate {
    pub shown: Option<Shown>,
    request: u64,
    /// The send waiting for its text to be translated, by its request.
    sending: Option<u64>,
    /// The card's language list is open.
    pub open: bool,
}

/// One message's translation, from the request to its answer.
pub struct Shown {
    /// The message translated, so another language can be asked for it.
    pub item: String,
    pub target: String,
    pub source: Option<String>,
    pub state: State,
    request: u64,
}

pub enum State {
    Waiting,
    Ready(String),
    Failed,
}

#[derive(Clone, Debug)]
pub enum Event {
    Close,
    /// The card's language list, opened or closed, and the language chosen.
    TogglePicker,
    PickTarget(usize),
    Done(u64, Result<Fetched, ()>),
    /// A draft came back translated, or not: it's sent either way.
    SendDone(u64, Result<Fetched, ()>, String),
}

#[derive(Clone, Debug)]
pub struct Fetched {
    pub text: String,
    pub source: Option<String>,
}

/// The name of a language code, or the code itself when Google names one
/// the picker doesn't.
pub fn language_name(code: &str) -> String {
    LANGUAGES
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(code))
        .map_or_else(|| code.to_owned(), |(_, name)| (*name).to_owned())
}

/// Asks Google Translate for `item`'s text in the target Settings kept.
/// A deleted or empty message asks nothing.
pub fn open(app: &mut App, item: &str) -> Task<Message> {
    let body = app
        .client
        .state()
        .message(item)
        .map(|message| message.body.clone())
        .unwrap_or_default();
    if body.trim().is_empty() {
        return Task::none();
    }
    let target = app
        .layout
        .prefs
        .translate_target
        .clone()
        .unwrap_or_else(|| "es".to_owned());
    app.translate.open = false;
    app.translate.request = app.translate.request.wrapping_add(1);
    let request = app.translate.request;
    app.translate.shown = Some(Shown {
        item: item.to_owned(),
        target: target.clone(),
        source: None,
        state: State::Waiting,
        request,
    });
    Task::perform(fetch(body, None, target), move |answer| {
        Message::Translate(Event::Done(request, answer))
    })
}

/// Translates `text` from the language Settings says it's written in (or
/// lets Google tell it) into the one messages are sent in, then sends it.
/// A failed translation sends the text as it was written, so a message is
/// never lost; a press while a send is translating waits for it.
pub fn send_out(app: &mut App, text: String) -> Task<Message> {
    if app.translate.sending.is_some() {
        return Task::none();
    }
    let target = app
        .layout
        .prefs
        .translate_target
        .clone()
        .unwrap_or_else(|| "es".to_owned());
    let source = app
        .layout
        .prefs
        .translate_source
        .clone()
        .filter(|code| !code.is_empty());
    app.translate.request = app.translate.request.wrapping_add(1);
    let request = app.translate.request;
    app.translate.sending = Some(request);
    // Enter's draft leaves the field at once, as a sent message does;
    // what's typed while the answer comes is a new message.
    app.client
        .dispatch(recore_core::application::Action::SetDraft(String::new()));
    app.composer.suggest.clear(&mut app.client);
    let original = text.clone();
    Task::batch([
        app.sync(),
        Task::perform(fetch(text, source, target), move |answer| {
            Message::Translate(Event::SendDone(request, answer, original))
        }),
    ])
}

pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Close => {
            app.translate.shown = None;
            app.translate.open = false;
        }
        Event::TogglePicker => app.translate.open = !app.translate.open,
        Event::PickTarget(index) => {
            app.translate.open = false;
            let Some((code, _)) = LANGUAGES.get(index).copied() else {
                return Task::none();
            };
            app.layout.prefs.translate_target = Some(code.to_owned());
            crate::layout::store(app.client.root(), "translate_target", code);
            let Some(item) = app.translate.shown.as_ref().map(|shown| shown.item.clone()) else {
                return Task::none();
            };
            return open(app, &item);
        }
        Event::Done(request, answer) => {
            let Some(shown) = app.translate.shown.as_mut() else {
                return Task::none();
            };
            if shown.request != request {
                return Task::none();
            }
            match answer {
                Ok(fetched) => {
                    shown.source = fetched.source;
                    shown.state = State::Ready(fetched.text);
                }
                Err(()) => shown.state = State::Failed,
            }
        }
        Event::SendDone(request, answer, original) => {
            if app.translate.sending != Some(request) {
                return Task::none();
            }
            app.translate.sending = None;
            // A translation that failed sends the words as they were
            // written: the message matters more than the language.
            let text = answer.map_or(original, |fetched| fetched.text);
            return crate::composer::send_text(app, text);
        }
    }
    Task::none()
}

async fn fetch(text: String, source: Option<String>, target: String) -> Result<Fetched, ()> {
    let source = source
        .as_deref()
        .filter(|code| !code.is_empty())
        .unwrap_or("auto");
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl={}&tl={}&dt=t&q={}",
        source,
        target,
        escaped(&text)
    );
    let client = reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/131.0 Safari/537.36",
        )
        .timeout(WAIT)
        .build()
        .map_err(|_| ())?;
    let response = client.get(url).send().await.map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }
    let bytes = response.bytes().await.map_err(|_| ())?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| ())?;
    parse(&value).ok_or(())
}

/// The translation in Google's answer: each segment's first string, then
/// the source language it names, when it does.
fn parse(value: &Value) -> Option<Fetched> {
    let segments = value.as_array()?.first()?.as_array()?;
    let mut text = String::new();
    for segment in segments {
        if let Some(part) = segment
            .as_array()
            .and_then(|parts| parts.first())
            .and_then(Value::as_str)
        {
            text.push_str(part);
        }
    }
    let text = text.trim().to_owned();
    if text.is_empty() {
        return None;
    }
    Some(Fetched {
        text,
        source: value.get(2).and_then(Value::as_str).map(str::to_owned),
    })
}

/// `text` as a query value: only the characters this endpoint may read
/// unescaped stay as they are.
fn escaped(text: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            write!(out, "%{byte:02X}").expect("a String writes");
        }
    }
    out
}

/// The translation card, over the window, while one is asked for or shown.
pub fn view(app: &App) -> Option<Element<'_, Message>> {
    let shown = app.translate.shown.as_ref()?;
    let theme = &app.theme;
    let mut body = column![
        row![
            text("Translate")
                .size(18)
                .font(BOLD())
                .color(theme.text)
                .width(Length::Fill),
            app.ctx()
                .look
                .icon_button(Icon::X, "Close", Message::Translate(Event::Close))
                .below(),
        ]
        .align_y(iced::Alignment::Center),
    ]
    .spacing(12)
    .padding(20);
    let target = LANGUAGES
        .iter()
        .position(|(code, _)| code.eq_ignore_ascii_case(&shown.target))
        .unwrap_or(1);
    body = body.push(
        crate::form::dropdown(
            LANGUAGES
                .iter()
                .map(|(_, name)| (*name).to_owned())
                .collect(),
            Some(target),
            app.translate.open,
            Message::Translate(Event::TogglePicker),
            |index| Message::Translate(Event::PickTarget(index)),
        )
        .view(app.ctx().look),
    );
    if let Some(source) = shown.source.as_deref() {
        body = body.push(
            text(format!("Detected: {}", language_name(source)))
                .size(12)
                .color(theme.muted),
        );
    }
    match &shown.state {
        State::Waiting => {
            body = body.push(text("Translating...").size(15).color(theme.secondary));
        }
        State::Ready(translated) => {
            let words = container(text(translated.as_str()).size(15).color(theme.text))
                .padding(Padding::ZERO.right(8));
            // As tall as the words need, up to a card that scrolls.
            let lines = (translated.chars().count() as f32 / 52.0).ceil();
            let height = (lines * 21.0 + 8.0).clamp(36.0, LONGEST);
            body = body.push(
                scroll_view(words)
                    .height(Length::Fixed(height))
                    .style(crate::style::scrollbar(theme)),
            );
        }
        State::Failed => {
            body = body.push(
                text("Could not translate this message. Check the connection and try again.")
                    .size(14)
                    .color(theme.secondary),
            );
        }
    }
    Some(dialog(
        theme,
        body.into(),
        WIDTH,
        Message::Translate(Event::Close),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_answer_is_each_segment_in_turn() {
        let answer = json!([
            [
                ["Hola", "Hello", null, null],
                [" mundo", " world", null, null]
            ],
            null,
            "en"
        ]);
        let fetched = parse(&answer).unwrap();
        assert_eq!(fetched.text, "Hola mundo");
        assert_eq!(fetched.source.as_deref(), Some("en"));
        assert!(parse(&json!([[]])).is_none());
    }

    #[test]
    fn the_text_is_a_query_value() {
        assert_eq!(escaped("hi there & you?"), "hi%20there%20%26%20you%3F");
    }

    #[test]
    fn a_language_the_picker_knows_has_its_name() {
        assert_eq!(language_name("es"), "Spanish");
        assert_eq!(language_name("xx"), "xx");
    }
}
