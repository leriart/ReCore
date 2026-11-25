//! Webhooks: a server's webhooks, each copied by its URL or deleted at
//! once, and Create Webhook, which names one and chooses its channel.

use iced::{
    Alignment, Element, Length,
    widget::{column, row, text},
};
use recore_core::application::{ServerSettings, WEBHOOK_NAME_LIMIT, WebhookInput, WebhooksPage};

use super::{Drop, Event, Screen, View, message};
use crate::{
    app::Message,
    controls::{Kind, action},
    dialog, form,
};

const DIALOG: f32 = 440.0;
pub(super) const NAME: &str = "webhook-name";

impl<'a> View<'a> {
    /// Webhooks: Create Webhook, then the server's webhooks with their
    /// channel, who made them and their buttons.
    pub(super) fn webhooks_page(
        self,
        core: &'a ServerSettings,
        page: &'a WebhooksPage,
        _screen: &'a Screen,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let app = &self.cx.app;
        let saving = page.draft.as_ref().is_some_and(|draft| draft.saving);
        let channels = app.settings_channels(&core.server, false);
        let channel_name = |id: Option<&str>| {
            id.and_then(|id| {
                channels
                    .iter()
                    .find(|(channel, _)| channel == id)
                    .map(|(_, name)| name.clone())
            })
            .map(|name| format!("#{name}"))
            .unwrap_or_else(|| "an unknown channel".into())
        };
        let mut list = column![].spacing(6).width(Length::Fill);
        if page.webhooks.is_empty() {
            list = list.push(form::note(theme, "This server has no webhooks."));
        }
        for webhook in &page.webhooks {
            let mut under = vec![channel_name(webhook.channel_id.as_deref())];
            if let Some(creator) = &webhook.creator {
                under.push(format!("by {creator}"));
            }
            let kind = match webhook.kind {
                1 => "Incoming",
                2 => "Channel Follower",
                3 => "Application",
                _ => "Webhook",
            };
            under.push(kind.into());
            let url = WebhooksPage::address(webhook);
            let mut buttons = row![].spacing(form::TIGHT);
            if let Some(url) = url {
                buttons = buttons.push(action(
                    theme,
                    "Copy URL",
                    Kind::Plain,
                    Some(message(Event::CopyWebhook(url))),
                ));
            }
            buttons = buttons.push(action(
                theme,
                "Delete",
                Kind::Danger,
                (!saving)
                    .then(|| message(Event::Webhooks(WebhookInput::Delete(webhook.id.clone())))),
            ));
            let ground = theme.wash(0.03);
            list = list.push(
                iced::widget::container(
                    row![
                        column![
                            text(webhook.name.as_str()).size(15).color(theme.text),
                            text(under.join(" · ")).size(13).color(theme.muted),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        buttons,
                    ]
                    .spacing(form::GAP)
                    .align_y(Alignment::Center),
                )
                .padding(10)
                .width(Length::Fill)
                .style(move |_| crate::style::rounded(ground, 8)),
            );
        }
        column![
            row![
                column![
                    form::title(theme, "Webhooks"),
                    form::note(
                        theme,
                        "Webhooks let other apps post messages to a channel without a bot.",
                    ),
                ]
                .spacing(form::TIGHT)
                .width(Length::Fill),
                action(
                    theme,
                    "Create Webhook",
                    Kind::Primary,
                    (!saving).then(|| message(Event::Webhooks(WebhookInput::Create))),
                ),
            ]
            .spacing(form::GAP)
            .align_y(Alignment::Center),
            form::rule(theme),
            list,
        ]
        .spacing(form::GAP)
        .width(Length::Fill)
        .into()
    }

    /// The dialog that names a new webhook and chooses its channel.
    pub(super) fn webhook_dialog(
        self,
        page: &'a WebhooksPage,
        screen: &'a Screen,
    ) -> Option<Element<'a, Message>> {
        let draft = page.draft.as_ref()?;
        let theme = self.cx.look.theme;
        let core = self.cx.state.server_settings.as_ref()?;
        let channels = self.cx.app.settings_channels(&core.server, false);
        let names: Vec<String> = channels
            .iter()
            .map(|(_, name)| format!("#{name}"))
            .collect();
        let ids: Vec<String> = channels.into_iter().map(|(id, _)| id).collect();
        let chosen = draft
            .channel
            .as_ref()
            .and_then(|channel| ids.iter().position(|id| id == channel));
        let ready = draft.ready();
        let mut body = column![
            text("Create Webhook")
                .size(20)
                .font(crate::fonts::BOLD())
                .color(theme.text),
            form::group([
                form::heading(theme, "NAME"),
                form::entry(theme, NAME, "Webhook name", draft.name.as_str())
                    .on_input(|text| message(Event::Webhooks(WebhookInput::Name(text))))
                    .into(),
            ]),
            form::group([
                form::heading(theme, "CHANNEL"),
                form::dropdown(
                    names,
                    chosen,
                    screen.open == Some(Drop::WebhookChannel),
                    message(Event::Drop(Drop::WebhookChannel)),
                    move |index| {
                        message(Event::Webhooks(WebhookInput::Channel(
                            ids[index].clone(),
                        )))
                    },
                )
                .view(self.cx.look),
            ]),
            form::note(
                theme,
                format!(
                    "Up to {WEBHOOK_NAME_LIMIT} characters. The webhook posts to the channel you choose."
                ),
            ),
        ]
        .spacing(14)
        .padding(16);
        if let Some(error) = &draft.error {
            body = body.push(form::error(theme, error.as_str()));
        }
        body = body.push(
            row![
                iced::widget::Space::new().width(Length::Fill),
                action(
                    theme,
                    "Cancel",
                    Kind::Ghost,
                    (!draft.saving).then(|| { message(Event::Webhooks(WebhookInput::Cancel)) }),
                ),
                action(
                    theme,
                    if draft.saving { "Saving..." } else { "Create" },
                    Kind::Primary,
                    ready.then(|| message(Event::Webhooks(WebhookInput::Save))),
                ),
            ]
            .spacing(form::TIGHT),
        );
        Some(dialog::panel(
            theme,
            body.into(),
            DIALOG,
            None,
            message(Event::Webhooks(WebhookInput::Cancel)),
        ))
    }
}

pub(super) fn dialog_field(page: &WebhooksPage) -> Option<&'static str> {
    page.draft.as_ref().map(|_| NAME)
}
