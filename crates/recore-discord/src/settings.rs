//! The settings Discord keeps for the account: the status and the game it
//! shares, how messages draw, the theme, the language, which reactions
//! notify, and the server list's order and folders.
//! The settings are a protobuf message, `PreloadedUserSettings`, in base64;
//! ReCore reads those fields and writes the status, the text and image
//! switches, the theme and the language by hand, always whole: Discord resets
//! what a write of a field leaves out. `status` also says whether the account
//! shares the game it plays.
//!
//! Each session tells the gateway its presence, so ReCore sends the status the
//! settings hold once connected, and again when they change.

use base64::Engine;
use serde_json::{Value, json};

/// `PreloadedUserSettings.status`.
const STATUS: u64 = 11;
/// Voice and video settings, and in them the AFK timeout.
const VOICE_AND_VIDEO: u64 = 5;
const VOICE_AFK_TIMEOUT: u64 = 6;
/// `PreloadedUserSettings.notifications`, and in it `reaction_notifications`,
/// an enum: 0 every reaction notifies, 1 only in DMs and 2 none.
const NOTIFICATIONS: u64 = 7;
const REACTION_NOTIFICATIONS: u64 = 7;
/// `PreloadedUserSettings.text_and_images`, and in it the switches that
/// change how a message draws, each a `BoolValue`, and `view_nsfw_commands`:
/// age-restricted commands are offered in DMs.
const TEXT_AND_IMAGES: u64 = 6;
const INLINE_ATTACHMENT_MEDIA: u64 = 9;
const INLINE_EMBED_MEDIA: u64 = 10;
const GIF_AUTO_PLAY: u64 = 11;
const RENDER_EMBEDS: u64 = 12;
const RENDER_REACTIONS: u64 = 13;
const MESSAGE_DISPLAY_COMPACT: u64 = 17;
const CONVERT_EMOTICONS: u64 = 21;
const VIEW_NSFW_COMMANDS: u64 = 23;
/// `PreloadedUserSettings.appearance`, and in it the theme, an enum: 1 dark,
/// 2 light, 3 darker and 4 midnight, 0 being dark as well; and developer mode.
const APPEARANCE: u64 = 13;
const THEME: u64 = 1;
const DEVELOPER_MODE: u64 = 2;
/// `PreloadedUserSettings.localization`, and in it the locale, a
/// `StringValue`, such as `en-US`.
const LOCALIZATION: u64 = 12;
const LOCALE: u64 = 1;
/// `StatusSettings.status`, a `StringValue`, and `StatusSettings.custom_status`.
const STATUS_STATUS: u64 = 1;
const STATUS_CUSTOM: u64 = 2;
/// `StatusSettings.show_current_game`, a `BoolValue`: whether the account
/// shares the game it plays and the Rich Presence games send.
const STATUS_SHOW_GAME: u64 = 3;
/// `StatusSettings.status_expires_at_ms`, a `fixed64`, and
/// `status_created_at_ms`, a `UInt64Value`: when a status set for a while
/// ends, and when it was set.
const STATUS_EXPIRES: u64 = 4;
const STATUS_CREATED: u64 = 5;
/// `CustomStatus`'s fields.
const CUSTOM_TEXT: u64 = 1;
const CUSTOM_EMOJI_ID: u64 = 2;
const CUSTOM_EMOJI_NAME: u64 = 3;
const CUSTOM_EXPIRES: u64 = 4;
/// `CustomStatus.created_at_ms`, a `fixed64`: when it was set.
const CUSTOM_CREATED: u64 = 5;
/// `PreloadedUserSettings.guild_folders`, a `GuildFolders`: its `folders`, in
/// the order the server list shows them, and `guild_positions`, the order
/// older clients kept.
const GUILD_FOLDERS: u64 = 14;
const FOLDERS: u64 = 1;
const GUILD_POSITIONS: u64 = 2;
/// `GuildFolder.guild_ids`: one server outside a folder, or a folder's.
const FOLDER_GUILDS: u64 = 1;
/// `GuildFolder`'s `id`, an `Int64Value` that only a folder has; its `name`,
/// a `StringValue`; and its `color`, a `UInt64Value` holding RGB.
const FOLDER_ID: u64 = 2;
const FOLDER_NAME: u64 = 3;
const FOLDER_COLOR: u64 = 4;

pub(crate) const VARINT: u64 = 0;
const FIXED64: u64 = 1;
const BYTES: u64 = 2;
const FIXED32: u64 = 5;

/// A custom status: its text, its emoji, and when it ends.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CustomStatus {
    pub text: String,
    /// A Unicode emoji, or a custom emoji's name.
    pub emoji_name: Option<String>,
    /// A custom emoji's id.
    pub emoji_id: Option<String>,
    /// Unix milliseconds when it clears itself; none when it stays.
    pub expires_at_ms: Option<u64>,
}

impl CustomStatus {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.emoji_name.is_none() && self.emoji_id.is_none()
    }

    /// Whether it has ended by `now_ms`.
    pub fn expired(&self, now_ms: u64) -> bool {
        self.expires_at_ms.is_some_and(|expires| expires <= now_ms)
    }
}

/// The theme Discord's client shows, as `appearance.theme` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
    Darker,
    Midnight,
}

impl Theme {
    /// The enum's value on the wire.
    fn number(self) -> u64 {
        match self {
            Self::Dark => 1,
            Self::Light => 2,
            Self::Darker => 3,
            Self::Midnight => 4,
        }
    }

    /// The theme a value names; 0 is Discord's unset, which it treats as dark.
    fn of(value: u64) -> Option<Self> {
        match value {
            0 | 1 => Some(Self::Dark),
            2 => Some(Self::Light),
            3 => Some(Self::Darker),
            4 => Some(Self::Midnight),
            _ => None,
        }
    }
}

/// Which reactions notify the account, as `notifications.reaction_notifications`
/// names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactionNotifications {
    /// Every reaction notifies.
    All,
    /// Only reactions in DMs notify.
    OnlyDirectMessages,
    /// No reaction notifies.
    Disabled,
}

impl ReactionNotifications {
    /// The setting a value names.
    fn of(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::All),
            1 => Some(Self::OnlyDirectMessages),
            2 => Some(Self::Disabled),
            _ => None,
        }
    }
}

/// A switch in `text_and_images` that changes how a message draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAndImage {
    /// Attachments show where they were uploaded.
    InlineAttachmentMedia,
    /// Images and videos from links show in place.
    InlineEmbedMedia,
    /// GIFs play as they show.
    GifAutoPlay,
    /// Messages' embeds, such as link previews, render.
    RenderEmbeds,
    /// Reactions render under messages.
    RenderReactions,
    /// Messages draw compactly.
    MessageDisplayCompact,
    /// Emoticons become emoji.
    ConvertEmoticons,
}

impl TextAndImage {
    /// The switch's field in `TextAndImagesSettings`.
    fn number(self) -> u64 {
        match self {
            Self::InlineAttachmentMedia => INLINE_ATTACHMENT_MEDIA,
            Self::InlineEmbedMedia => INLINE_EMBED_MEDIA,
            Self::GifAutoPlay => GIF_AUTO_PLAY,
            Self::RenderEmbeds => RENDER_EMBEDS,
            Self::RenderReactions => RENDER_REACTIONS,
            Self::MessageDisplayCompact => MESSAGE_DISPLAY_COMPACT,
            Self::ConvertEmoticons => CONVERT_EMOTICONS,
        }
    }
}

/// A top-level field of `PreloadedUserSettings` that ReCore writes whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsSection {
    TextAndImages,
    Appearance,
    Localization,
}

impl SettingsSection {
    /// The section's field in `PreloadedUserSettings`.
    fn number(self) -> u64 {
        match self {
            Self::TextAndImages => TEXT_AND_IMAGES,
            Self::Appearance => APPEARANCE,
            Self::Localization => LOCALIZATION,
        }
    }
}

/// What the settings say of the account. `None` where they don't say: a
/// partial update leaves the rest as it was.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusSettings {
    /// `online`, `idle`, `dnd` or `invisible`.
    pub status: Option<String>,
    /// The custom status, `Some(None)` when it's cleared.
    pub custom: Option<Option<CustomStatus>>,
    /// How long, in seconds, nobody uses a client before it tells Discord
    /// it's away from the keyboard.
    pub afk_timeout: Option<u64>,
    /// Whether the account shares the game it plays.
    pub show_game: Option<bool>,
    /// Whether apps' age-restricted commands are offered in DMs.
    pub nsfw_commands_in_dms: Option<bool>,
    /// Whether attachments show where they were uploaded.
    pub inline_attachment_media: Option<bool>,
    /// Whether images and videos from links show in place.
    pub inline_embed_media: Option<bool>,
    /// Whether GIFs play as they show.
    pub gif_auto_play: Option<bool>,
    /// Whether messages' embeds, such as link previews, render.
    pub render_embeds: Option<bool>,
    /// Whether reactions render under messages.
    pub render_reactions: Option<bool>,
    /// Whether messages draw compactly.
    pub message_display_compact: Option<bool>,
    /// Whether emoticons become emoji.
    pub convert_emoticons: Option<bool>,
    /// The theme Discord's client shows.
    pub theme: Option<Theme>,
    /// Whether Discord's client offers its developer tools.
    pub developer_mode: Option<bool>,
    /// The language Discord's client shows, such as `en-US`.
    pub locale: Option<String>,
    /// Which reactions notify the account.
    pub reaction_notifications: Option<ReactionNotifications>,
}

/// The settings in base64, as READY's `user_settings_proto` or a
/// USER_SETTINGS_PROTO_UPDATE carries them.
pub fn read(encoded: &str) -> StatusSettings {
    let Ok(data) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
        return StatusSettings::default();
    };
    // A UInt32Value: the number in its field 1, 0 when it's left out.
    let mut settings = StatusSettings {
        afk_timeout: message(&data, VOICE_AND_VIDEO)
            .and_then(|voice| message(voice, VOICE_AFK_TIMEOUT))
            .map(wrapped_varint),
        ..StatusSettings::default()
    };
    if let Some(text) = message(&data, TEXT_AND_IMAGES) {
        let switch = |number| message(text, number).map(bool_value);
        settings.inline_attachment_media = switch(INLINE_ATTACHMENT_MEDIA);
        settings.inline_embed_media = switch(INLINE_EMBED_MEDIA);
        settings.gif_auto_play = switch(GIF_AUTO_PLAY);
        settings.render_embeds = switch(RENDER_EMBEDS);
        settings.render_reactions = switch(RENDER_REACTIONS);
        settings.message_display_compact = switch(MESSAGE_DISPLAY_COMPACT);
        settings.convert_emoticons = switch(CONVERT_EMOTICONS);
        settings.nsfw_commands_in_dms = switch(VIEW_NSFW_COMMANDS);
    }
    if let Some(appearance) = message(&data, APPEARANCE) {
        settings.theme = appearance_theme(appearance);
        settings.developer_mode = Some(fields(appearance).any(|(number, value)| {
            number == DEVELOPER_MODE && matches!(value, Field::Varint(on) if on != 0)
        }));
    }
    if let Some(localization) = message(&data, LOCALIZATION) {
        settings.locale = message(localization, LOCALE)
            .map(string_value)
            .filter(|locale| !locale.is_empty());
    }
    if let Some(notifications) = message(&data, NOTIFICATIONS) {
        // An enum leaves out its 0: a section without the field says
        // every reaction notifies.
        settings.reaction_notifications = match fields(notifications)
            .filter(|(number, _)| *number == REACTION_NOTIFICATIONS)
            .last()
        {
            None => Some(ReactionNotifications::All),
            Some((_, Field::Varint(level))) => ReactionNotifications::of(level),
            Some(_) => None,
        };
    }
    let Some(status) = message(&data, STATUS) else {
        return settings;
    };
    // `status` comes whole: one without a custom status has none, as
    // when another device clears it or it runs out there.
    settings.custom = Some(None);
    for (number, value) in fields(status) {
        match (number, value) {
            (STATUS_STATUS, Field::Bytes(wrapper)) => {
                settings.status = fields(wrapper).find_map(|(number, value)| match value {
                    Field::Bytes(text) if number == 1 => {
                        Some(String::from_utf8_lossy(text).into_owned())
                    }
                    _ => None,
                });
            }
            (STATUS_CUSTOM, Field::Bytes(custom)) => {
                let mut status = CustomStatus::default();
                for (number, value) in fields(custom) {
                    match (number, value) {
                        (CUSTOM_TEXT, Field::Bytes(text)) => {
                            status.text = String::from_utf8_lossy(text).into_owned();
                        }
                        (CUSTOM_EMOJI_ID, Field::Fixed64(id)) if id != 0 => {
                            status.emoji_id = Some(id.to_string());
                        }
                        (CUSTOM_EMOJI_NAME, Field::Bytes(name)) if !name.is_empty() => {
                            status.emoji_name = Some(String::from_utf8_lossy(name).into_owned());
                        }
                        (CUSTOM_EXPIRES, Field::Fixed64(at)) if at != 0 => {
                            status.expires_at_ms = Some(at);
                        }
                        _ => {}
                    }
                }
                settings.custom = Some((!status.is_empty()).then_some(status));
            }
            (STATUS_SHOW_GAME, Field::Bytes(wrapper)) => {
                settings.show_game = Some(bool_value(wrapper));
            }
            _ => {}
        }
    }
    settings
}

/// The theme `appearance` names, dark when it leaves it out, as its 0 does.
fn appearance_theme(appearance: &[u8]) -> Option<Theme> {
    match fields(appearance)
        .filter(|(number, _)| *number == THEME)
        .last()
    {
        None => Some(Theme::Dark),
        Some((_, Field::Varint(theme))) => Theme::of(theme),
        Some(_) => None,
    }
}

/// A `StringValue`'s text, empty when it leaves it out.
fn string_value(wrapper: &[u8]) -> String {
    fields(wrapper)
        .find_map(|(number, value)| match value {
            Field::Bytes(text) if number == 1 => Some(String::from_utf8_lossy(text).into_owned()),
            _ => None,
        })
        .unwrap_or_default()
}

/// A `BoolValue`'s value, which leaves out a false one.
fn bool_value(wrapper: &[u8]) -> bool {
    fields(wrapper)
        .any(|(number, value)| number == 1 && matches!(value, Field::Varint(on) if on != 0))
}

/// The last field `number` of `data` that holds a message.
pub(crate) fn message(data: &[u8], number: u64) -> Option<&[u8]> {
    fields(data)
        .filter_map(|(field, value)| match value {
            Field::Bytes(bytes) if field == number => Some(bytes),
            _ => None,
        })
        .last()
}

/// A folder in the account's server list.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Folder {
    /// Discord's id for it.
    pub id: String,
    /// Its name; none when it has none, and the rail names it by its servers.
    pub name: Option<String>,
    /// Its colour as RGB; none for Discord's default.
    pub color: Option<u32>,
    /// Its servers' ids, in order.
    pub servers: Vec<String>,
}

/// The account's server list, as its settings keep it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerList {
    /// Every server it places, in order, each folder's in its place.
    pub order: Vec<String>,
    pub folders: Vec<Folder>,
}

/// The account's server list. `None` when the settings don't say: a partial
/// update leaves it as it was.
pub fn server_list(encoded: &str) -> Option<ServerList> {
    let data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let folders = fields(&data).find_map(|(number, value)| match value {
        Field::Bytes(bytes) if number == GUILD_FOLDERS => Some(bytes),
        _ => None,
    })?;
    let mut list = ServerList::default();
    let mut positions = Vec::new();
    for (number, value) in fields(folders) {
        match (number, value) {
            (FOLDERS, Field::Bytes(entry)) => {
                let mut folder = Folder::default();
                let mut id = None;
                for (number, value) in fields(entry) {
                    match (number, value) {
                        (FOLDER_GUILDS, value) => folder
                            .servers
                            .extend(fixed64s(value).into_iter().map(|id| id.to_string())),
                        (FOLDER_ID, Field::Bytes(wrapper)) => id = Some(wrapped_varint(wrapper)),
                        (FOLDER_NAME, Field::Bytes(wrapper)) => {
                            folder.name = fields(wrapper)
                                .find_map(|(number, value)| match value {
                                    Field::Bytes(text) if number == 1 => {
                                        Some(String::from_utf8_lossy(text).into_owned())
                                    }
                                    _ => None,
                                })
                                .filter(|name| !name.trim().is_empty());
                        }
                        (FOLDER_COLOR, Field::Bytes(wrapper)) => {
                            folder.color = u32::try_from(wrapped_varint(wrapper)).ok();
                        }
                        _ => {}
                    }
                }
                list.order.extend(folder.servers.iter().cloned());
                // An entry without an id is one server outside any folder.
                if let Some(id) = id
                    && !folder.servers.is_empty()
                {
                    folder.id = id.to_string();
                    list.folders.push(folder);
                }
            }
            (GUILD_POSITIONS, value) => positions.extend(fixed64s(value)),
            _ => {}
        }
    }
    if list.order.is_empty() {
        list.order = positions.into_iter().map(|id| id.to_string()).collect();
    }
    Some(list)
}

/// The value of a wrapper such as `Int64Value`, whose field 1 is a varint;
/// 0, protobuf's default, when it's left out.
fn wrapped_varint(wrapper: &[u8]) -> u64 {
    fields(wrapper)
        .find_map(|(number, value)| match value {
            Field::Varint(value) if number == 1 => Some(value),
            _ => None,
        })
        .unwrap_or_default()
}

/// Base64 settings holding only a server order: each folder's servers, a
/// server outside a folder as a folder of one.
#[cfg(test)]
pub(crate) fn write_server_order(folders: &[&[u64]]) -> String {
    let entries: Vec<_> = folders
        .iter()
        .map(|servers| Folder {
            servers: servers.iter().map(u64::to_string).collect(),
            ..Folder::default()
        })
        .collect();
    write_server_list(&entries)
}

/// Base64 settings holding only a server list. An entry with an empty `id`
/// is a server outside any folder.
pub fn write_server_list(entries: &[Folder]) -> String {
    let mut message = Vec::new();
    for entry in entries {
        let packed: Vec<u8> = entry
            .servers
            .iter()
            .filter_map(|id| id.parse::<u64>().ok())
            .flat_map(u64::to_le_bytes)
            .collect();
        let mut folder = Vec::new();
        put_bytes(&mut folder, FOLDER_GUILDS, &packed);
        if let Ok(id) = entry.id.parse::<u64>() {
            let mut wrapper = Vec::new();
            put_varint(&mut wrapper, 1 << 3 | VARINT);
            put_varint(&mut wrapper, id);
            put_bytes(&mut folder, FOLDER_ID, &wrapper);
        }
        if let Some(name) = &entry.name {
            let mut wrapper = Vec::new();
            put_bytes(&mut wrapper, 1, name.as_bytes());
            put_bytes(&mut folder, FOLDER_NAME, &wrapper);
        }
        if let Some(color) = entry.color {
            let mut wrapper = Vec::new();
            put_varint(&mut wrapper, 1 << 3 | VARINT);
            put_varint(&mut wrapper, u64::from(color));
            put_bytes(&mut folder, FOLDER_COLOR, &wrapper);
        }
        put_bytes(&mut message, FOLDERS, &folder);
    }
    let mut settings = Vec::new();
    put_bytes(&mut settings, GUILD_FOLDERS, &message);
    base64::engine::general_purpose::STANDARD.encode(settings)
}

/// The values of a repeated `fixed64` field: packed, or one by one.
fn fixed64s(value: Field<'_>) -> Vec<u64> {
    match value {
        Field::Fixed64(value) => vec![value],
        Field::Bytes(packed) => packed
            .as_chunks::<8>()
            .0
            .iter()
            .map(|bytes| u64::from_le_bytes(*bytes))
            .collect(),
        _ => Vec::new(),
    }
}

/// The settings' `status` as they hold it, its bytes, when they have one:
/// what a write of it starts from.
pub fn status_field(encoded: &str) -> Option<Vec<u8>> {
    submessage(encoded, STATUS)
}

/// Base64 settings holding `status` alone, as a PATCH sends them: Discord
/// puts each top-level field it carries in place of the old one, and resets
/// what that field leaves out, so `status` always goes whole.
pub fn encode_status(status: &[u8]) -> String {
    encode_message(STATUS, status)
}

/// The settings' field `section` as they hold it, its bytes, when they have
/// it: what a write of it starts from.
pub fn section_field(encoded: &str, section: SettingsSection) -> Option<Vec<u8>> {
    submessage(encoded, section.number())
}

/// Base64 settings holding `section` alone, as a PATCH sends them: it goes
/// whole, as [`encode_status`] does.
pub fn encode_section(section: SettingsSection, bytes: &[u8]) -> String {
    encode_message(section.number(), bytes)
}

/// The settings' field `number` as they hold it, its bytes, when they have
/// it.
fn submessage(encoded: &str, number: u64) -> Option<Vec<u8>> {
    let data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    message(&data, number).map(<[u8]>::to_vec)
}

/// Base64 settings holding only the field `number`, a message.
fn encode_message(number: u64, bytes: &[u8]) -> String {
    let mut settings = Vec::new();
    put_bytes(&mut settings, number, bytes);
    base64::engine::general_purpose::STANDARD.encode(settings)
}

/// `current`, a message of the settings such as `status`, with its field
/// `number` set to `value`, or left out with `None`, and the fields in
/// `stale` taken out; the rest as it was, unknown fields too, as Discord's
/// client keeps them.
fn with_field(current: &[u8], number: u64, value: Option<Field<'_>>, stale: &[u64]) -> Vec<u8> {
    let mut message = Vec::new();
    for (field, kept) in fields(current) {
        if field != number && !stale.contains(&field) {
            put_field(&mut message, field, &kept);
        }
    }
    if let Some(value) = value {
        put_field(&mut message, number, &value);
    }
    message
}

/// `current`, the settings' `status`, with the custom status set, or left
/// out to clear it with `None`, as Discord's client clears it. A new one
/// says when it was set, `now_ms`, as the client's does.
pub fn write_custom(current: &[u8], custom: Option<&CustomStatus>, now_ms: u64) -> Vec<u8> {
    let message = custom.map(|custom| {
        let mut message = Vec::new();
        put_bytes(&mut message, CUSTOM_TEXT, custom.text.as_bytes());
        if let Some(id) = custom
            .emoji_id
            .as_deref()
            .and_then(|id| id.parse::<u64>().ok())
        {
            put_fixed64(&mut message, CUSTOM_EMOJI_ID, id);
        }
        if let Some(name) = &custom.emoji_name {
            put_bytes(&mut message, CUSTOM_EMOJI_NAME, name.as_bytes());
        }
        if let Some(at) = custom.expires_at_ms {
            put_fixed64(&mut message, CUSTOM_EXPIRES, at);
        }
        put_fixed64(&mut message, CUSTOM_CREATED, now_ms);
        message
    });
    with_field(
        current,
        STATUS_CUSTOM,
        message.as_deref().map(Field::Bytes),
        &[],
    )
}

/// `current`, the settings' `status`, with the status set: `online`,
/// `idle`, `dnd` or `invisible`. As in Discord's client it stays until
/// changed, the old one's end going with it, and says when it was set,
/// `now_ms`, or keeps when the same status was.
pub fn write_status(current: &[u8], status: &str, now_ms: u64) -> Vec<u8> {
    let mut wrapper = Vec::new();
    put_bytes(&mut wrapper, 1, status.as_bytes());
    let created = fields(current).find_map(|(number, value)| match value {
        Field::Bytes(created) if number == STATUS_CREATED => Some(created.to_vec()),
        _ => None,
    });
    let same = status_of(current).as_deref() == Some(status);
    let created = match created.filter(|_| same) {
        Some(created) => created,
        None => {
            let mut created = Vec::new();
            put_varint(&mut created, 1 << 3 | VARINT);
            put_varint(&mut created, now_ms);
            created
        }
    };
    let status = with_field(
        current,
        STATUS_STATUS,
        Some(Field::Bytes(&wrapper)),
        &[STATUS_EXPIRES, STATUS_CREATED],
    );
    with_field(&status, STATUS_CREATED, Some(Field::Bytes(&created)), &[])
}

/// The status `status`, the settings' field, holds, if any.
fn status_of(status: &[u8]) -> Option<String> {
    fields(status).find_map(|(number, value)| match value {
        Field::Bytes(wrapper) if number == STATUS_STATUS => {
            fields(wrapper).find_map(|(number, value)| match value {
                Field::Bytes(text) if number == 1 => {
                    Some(String::from_utf8_lossy(text).into_owned())
                }
                _ => None,
            })
        }
        _ => None,
    })
}

/// `current`, the settings' `status`, sharing the game the account plays,
/// or not.
pub fn write_show_game(current: &[u8], shown: bool) -> Vec<u8> {
    let mut wrapper = Vec::new();
    if shown {
        put_varint(&mut wrapper, 1 << 3 | VARINT);
        put_varint(&mut wrapper, 1);
    }
    with_field(current, STATUS_SHOW_GAME, Some(Field::Bytes(&wrapper)), &[])
}

/// `current`, the settings' `text_and_images`, with a switch that changes
/// how messages draw set to `on`. The rest of the section stays as it was.
pub fn write_text_and_image(current: &[u8], switch: TextAndImage, on: bool) -> Vec<u8> {
    let mut wrapper = Vec::new();
    if on {
        put_varint(&mut wrapper, 1 << 3 | VARINT);
        put_varint(&mut wrapper, 1);
    }
    with_field(current, switch.number(), Some(Field::Bytes(&wrapper)), &[])
}

/// `current`, the settings' `appearance`, with the theme set. The rest of
/// the section stays as it was.
pub fn write_theme(current: &[u8], theme: Theme) -> Vec<u8> {
    with_field(current, THEME, Some(Field::Varint(theme.number())), &[])
}

/// `current`, the settings' `localization`, with the locale set, such as
/// `en-US`. The rest of the section stays as it was.
pub fn write_locale(current: &[u8], locale: &str) -> Vec<u8> {
    let mut wrapper = Vec::new();
    put_bytes(&mut wrapper, 1, locale.as_bytes());
    with_field(current, LOCALE, Some(Field::Bytes(&wrapper)), &[])
}

/// The presence a session sends the gateway, in IDENTIFY or opcode 3: the
/// status, and the custom status as an activity unless it has ended.
/// `away` marks the session AFK, so Discord sends this account's
/// notifications to its phone, and may show an online status as idle.
pub fn presence(status: &str, custom: Option<&CustomStatus>, now_ms: u64, away: Away) -> Value {
    let activities: Vec<Value> = custom
        .filter(|custom| !custom.is_empty() && !custom.expired(now_ms))
        .map(|custom| {
            let mut activity = json!({"type": 4, "name": "Custom Status"});
            if !custom.text.is_empty() {
                activity["state"] = json!(custom.text);
            }
            if custom.emoji_name.is_some() || custom.emoji_id.is_some() {
                activity["emoji"] = json!({
                    "name": custom.emoji_name,
                    "id": custom.emoji_id,
                    "animated": false,
                });
            }
            activity
        })
        .into_iter()
        .collect();
    let status = match away.idle_since {
        Some(_) if status == "online" => "idle",
        _ => status,
    };
    json!({
        "status": status,
        "since": away.idle_since.unwrap_or(0),
        "activities": activities,
        "afk": away.afk,
    })
}

/// Nobody has used ReCore for a while: as the web client decides it, from
/// [`Away::after`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Away {
    /// Away from the keyboard: Discord sends notifications to the phone.
    pub afk: bool,
    /// Shown idle since then, in Unix milliseconds.
    pub idle_since: Option<u64>,
}

/// The web client shows its account idle after 10 minutes without input.
const IDLE_AFTER_MS: u64 = 10 * 60 * 1000;
/// The AFK timeout the settings hold when they say none, in seconds.
pub const AFK_TIMEOUT: u64 = 60;

impl Away {
    /// How a session is away when nobody has used ReCore since
    /// `inactive_since`, in Unix milliseconds: AFK after the account's AFK
    /// timeout, at most 10 minutes, and idle after 10 minutes unless
    /// `show_idle` is off. Also returns when that next changes.
    pub fn after(
        inactive_since: Option<u64>,
        afk_timeout: u64,
        show_idle: bool,
        now_ms: u64,
    ) -> (Self, Option<u64>) {
        let Some(since) = inactive_since else {
            return (Self::default(), None);
        };
        let afk_at = since + afk_timeout.saturating_mul(1000).min(IDLE_AFTER_MS);
        let idle_at = since + IDLE_AFTER_MS;
        let away = Self {
            afk: now_ms >= afk_at,
            idle_since: (show_idle && now_ms >= idle_at).then_some(since),
        };
        let next = [afk_at, idle_at]
            .into_iter()
            .filter(|&at| at > now_ms && (show_idle || at == afk_at))
            .min();
        (away, next)
    }
}

pub(crate) enum Field<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
}

/// A protobuf message's fields, as far as they parse.
pub(crate) fn fields(mut data: &[u8]) -> impl Iterator<Item = (u64, Field<'_>)> {
    std::iter::from_fn(move || {
        let key = varint(&mut data)?;
        let value = match key & 7 {
            VARINT => Field::Varint(varint(&mut data)?),
            FIXED64 => {
                let (bytes, rest) = data.split_first_chunk::<8>()?;
                data = rest;
                Field::Fixed64(u64::from_le_bytes(*bytes))
            }
            BYTES => {
                let length = usize::try_from(varint(&mut data)?).ok()?;
                let bytes = data.get(..length)?;
                data = &data[length..];
                Field::Bytes(bytes)
            }
            FIXED32 => {
                let (bytes, rest) = data.split_first_chunk::<4>()?;
                data = rest;
                Field::Fixed32(u32::from_le_bytes(*bytes))
            }
            _ => return None,
        };
        Some((key >> 3, value))
    })
}

pub(crate) fn varint(data: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = data.split_first()?;
        *data = rest;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

pub(crate) fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

pub(crate) fn put_bytes(out: &mut Vec<u8>, number: u64, bytes: &[u8]) {
    put_varint(out, number << 3 | BYTES);
    put_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

pub(crate) fn put_fixed64(out: &mut Vec<u8>, number: u64, value: u64) {
    put_varint(out, number << 3 | FIXED64);
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_fixed32(out: &mut Vec<u8>, number: u64, value: u32) {
    put_varint(out, number << 3 | FIXED32);
    out.extend_from_slice(&value.to_le_bytes());
}

/// Writes a field as [`fields`] read it, to keep it as it was.
pub(crate) fn put_field(out: &mut Vec<u8>, number: u64, field: &Field<'_>) {
    match *field {
        Field::Varint(value) => {
            put_varint(out, number << 3 | VARINT);
            put_varint(out, value);
        }
        Field::Fixed64(value) => put_fixed64(out, number, value),
        Field::Bytes(bytes) => put_bytes(out, number, bytes),
        Field::Fixed32(value) => put_fixed32(out, number, value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_server_list_with_its_folders() {
        let folder = |id: &str, name: Option<&str>, color, servers: &[&str]| Folder {
            id: id.into(),
            name: name.map(Into::into),
            color,
            servers: servers.iter().map(|&id| id.into()).collect(),
        };
        let written = write_server_list(&[
            folder("", None, None, &["30"]),
            folder("7", Some("Games"), Some(0x5865f2), &["10", "40"]),
            folder("8", Some(" "), None, &["20"]),
        ]);
        let list = server_list(&written).unwrap();
        assert_eq!(list.order, ["30", "10", "40", "20"]);
        assert_eq!(
            list.folders,
            [
                folder("7", Some("Games"), Some(0x5865f2), &["10", "40"]),
                // A blank name counts as none; a folder of one is still a folder.
                folder("8", None, None, &["20"]),
            ]
        );
        // Settings that leave the list out don't empty it.
        assert_eq!(server_list(&status_only("idle")), None);
    }

    /// Settings holding a status written over none.
    fn status_only(status: &str) -> String {
        encode_status(&write_status(&[], status, 1))
    }

    fn custom_only(custom: Option<&CustomStatus>) -> String {
        encode_status(&write_custom(&[], custom, 1))
    }

    #[test]
    fn reads_and_writes_the_custom_status() {
        let status = CustomStatus {
            text: "Fixing the bridge".into(),
            emoji_name: Some("pog".into()),
            emoji_id: Some("1400000000000000300".into()),
            expires_at_ms: Some(1_791_300_000_000),
        };
        let written = custom_only(Some(&status));
        assert_eq!(
            read(&written),
            StatusSettings {
                status: None,
                custom: Some(Some(status)),
                ..StatusSettings::default()
            }
        );
        assert_eq!(read(&custom_only(None)).custom, Some(None));
        assert_eq!(
            read(&status_only("idle")),
            StatusSettings {
                status: Some("idle".into()),
                custom: Some(None),
                ..StatusSettings::default()
            },
            "the status comes whole: none in it is none"
        );
        // Other settings around it, and a status, are read past and read.
        let mut settings = Vec::new();
        put_bytes(&mut settings, 1, b"\x08\x01");
        put_varint(&mut settings, 6 << 3 | VARINT);
        put_varint(&mut settings, 300);
        let mut inner = Vec::new();
        let mut wrapper = Vec::new();
        put_bytes(&mut wrapper, 1, b"dnd");
        put_bytes(&mut inner, STATUS_STATUS, &wrapper);
        put_fixed64(&mut inner, 4, 9);
        put_bytes(&mut settings, STATUS, &inner);
        let read = read(&base64::engine::general_purpose::STANDARD.encode(settings));
        assert_eq!(read.status.as_deref(), Some("dnd"));
        assert_eq!(
            read.custom,
            Some(None),
            "a status without a custom status clears it, as another device's clearing does"
        );
        assert_eq!(super::read("not base64!"), StatusSettings::default());
    }

    #[test]
    fn reads_and_writes_whether_the_game_shows() {
        let game = |shown| encode_status(&write_show_game(&[], shown));
        assert_eq!(read(&game(false)).show_game, Some(false));
        assert_eq!(read(&game(true)).show_game, Some(true));
        assert_eq!(read(&status_only("idle")).show_game, None);
    }

    #[test]
    fn each_write_keeps_the_rest_of_the_status() {
        let custom = CustomStatus {
            text: "away for lunch".into(),
            ..CustomStatus::default()
        };
        // The account's status: Do Not Disturb, set at 5 for a while, and a
        // field ReCore doesn't know.
        let mut current = Vec::new();
        put_bytes(&mut current, STATUS_STATUS, b"\x0a\x03dnd");
        put_fixed64(&mut current, STATUS_EXPIRES, 9);
        put_bytes(&mut current, STATUS_CREATED, b"\x08\x05");
        put_bytes(&mut current, 9, b"kept");
        let has = |status: &[u8], number: u64| fields(status).any(|(field, _)| field == number);

        let written = write_custom(&current, Some(&custom), 100);
        let expected = StatusSettings {
            status: Some("dnd".into()),
            custom: Some(Some(custom)),
            ..StatusSettings::default()
        };
        assert_eq!(read(&encode_status(&written)), expected);
        let data = base64::engine::general_purpose::STANDARD
            .decode(encode_status(&written))
            .unwrap();
        assert_eq!(
            fields(&data).map(|(number, _)| number).collect::<Vec<_>>(),
            [STATUS],
            "only the status goes, the rest stay as they are"
        );
        assert!(
            fields(&written)
                .any(|(number, value)| number == STATUS_EXPIRES
                    && matches!(value, Field::Fixed64(9))),
            "the status still ends when it would"
        );
        assert!(
            has(&written, 9),
            "unknown fields stay, as the client keeps them"
        );
        assert!(
            fields(message(&written, STATUS_CUSTOM).unwrap())
                .any(|(number, value)| number == CUSTOM_CREATED
                    && matches!(value, Field::Fixed64(100))),
            "a custom status says when it was set"
        );

        let written = write_show_game(&written, false);
        assert_eq!(
            read(&encode_status(&written)),
            StatusSettings {
                show_game: Some(false),
                ..expected.clone()
            }
        );

        // The same status again keeps when it was set; another is set now
        // and stays until changed.
        let same = write_status(&written, "dnd", 200);
        assert_eq!(message(&same, STATUS_CREATED), Some(&b"\x08\x05"[..]));
        let written = write_status(&written, "idle", 200);
        assert_eq!(
            read(&encode_status(&written)),
            StatusSettings {
                status: Some("idle".into()),
                show_game: Some(false),
                ..expected
            }
        );
        assert!(
            !has(&written, STATUS_EXPIRES),
            "a new status stays until changed"
        );
        assert_eq!(
            message(&written, STATUS_CREATED),
            Some(&b"\x08\xc8\x01"[..]),
            "and says when it was set"
        );

        // A cleared custom status is left out, as the client clears it.
        let cleared = write_custom(&written, None, 300);
        assert!(!has(&cleared, STATUS_CUSTOM));
        assert_eq!(read(&encode_status(&cleared)).custom, Some(None));
    }

    #[test]
    fn reads_whether_age_restricted_commands_are_offered_in_dms() {
        let encoded = |on: bool| {
            let mut wrapper = Vec::new();
            if on {
                put_varint(&mut wrapper, 1 << 3 | VARINT);
                put_varint(&mut wrapper, 1);
            }
            let mut text = Vec::new();
            put_bytes(&mut text, VIEW_NSFW_COMMANDS, &wrapper);
            let mut settings = Vec::new();
            put_bytes(&mut settings, TEXT_AND_IMAGES, &text);
            base64::engine::general_purpose::STANDARD.encode(settings)
        };
        assert_eq!(read(&encoded(true)).nsfw_commands_in_dms, Some(true));
        assert_eq!(read(&encoded(false)).nsfw_commands_in_dms, Some(false));
        assert_eq!(read(&status_only("idle")).nsfw_commands_in_dms, None);
    }

    #[test]
    fn reads_the_server_order_with_folders_in_place() {
        let packed = |ids: &[u64]| {
            ids.iter()
                .flat_map(|id| id.to_le_bytes())
                .collect::<Vec<_>>()
        };
        let mut folders = Vec::new();
        // A server on its own, a folder of two, then one written unpacked.
        let mut folder = Vec::new();
        put_bytes(&mut folder, FOLDER_GUILDS, &packed(&[30]));
        put_bytes(&mut folders, FOLDERS, &folder);
        let mut folder = Vec::new();
        put_bytes(&mut folder, FOLDER_GUILDS, &packed(&[10, 20]));
        put_bytes(&mut folder, 3, b"\x0a\x05Games");
        put_bytes(&mut folders, FOLDERS, &folder);
        let mut folder = Vec::new();
        put_fixed64(&mut folder, FOLDER_GUILDS, 5);
        put_bytes(&mut folders, FOLDERS, &folder);
        put_bytes(&mut folders, GUILD_POSITIONS, &packed(&[1, 2]));
        let mut settings = Vec::new();
        put_bytes(&mut settings, STATUS, b"");
        put_bytes(&mut settings, GUILD_FOLDERS, &folders);
        let encoded = base64::engine::general_purpose::STANDARD.encode(&settings);
        assert_eq!(
            server_list(&encoded).unwrap().order,
            ["30", "10", "20", "5"]
        );

        // Older settings keep only the flat order.
        let mut folders = Vec::new();
        put_bytes(&mut folders, GUILD_POSITIONS, &packed(&[2, 1]));
        let mut settings = Vec::new();
        put_bytes(&mut settings, GUILD_FOLDERS, &folders);
        let encoded = base64::engine::general_purpose::STANDARD.encode(&settings);
        assert_eq!(server_list(&encoded).unwrap().order, ["2", "1"]);
        assert_eq!(server_list(&custom_only(None)), None);
    }

    #[test]
    fn presence_carries_the_custom_status_until_it_ends() {
        let status = CustomStatus {
            text: "Away".into(),
            emoji_name: Some("🌙".into()),
            emoji_id: None,
            expires_at_ms: Some(2_000),
        };
        let shown = presence("idle", Some(&status), 1_000, Away::default());
        assert_eq!(shown["status"], "idle");
        assert_eq!(
            shown["activities"],
            json!([{"type": 4, "name": "Custom Status", "state": "Away",
                "emoji": {"name": "🌙", "id": null, "animated": false}}])
        );
        assert_eq!(
            presence("online", Some(&status), 2_000, Away::default())["activities"],
            json!([])
        );
    }

    #[test]
    fn a_session_nobody_uses_goes_afk_then_idle_as_the_web_client_does() {
        const MINUTE: u64 = 60_000;
        let since = 1_000_000;
        let at = |minutes: u64, show_idle| {
            Away::after(Some(since), 60, show_idle, since + minutes * MINUTE)
        };
        assert_eq!(Away::after(None, 60, true, since), (Away::default(), None));
        assert_eq!(at(0, true), (Away::default(), Some(since + MINUTE)));
        let afk = Away {
            afk: true,
            idle_since: None,
        };
        assert_eq!(at(1, true), (afk, Some(since + 10 * MINUTE)));
        let idle = Away {
            afk: true,
            idle_since: Some(since),
        };
        assert_eq!(at(10, true), (idle, None));
        assert_eq!(at(10, false), (afk, None), "never idle when that's off");
        // A long AFK timeout still goes AFK at 10 minutes.
        assert_eq!(
            Away::after(Some(since), 3600, true, since + 10 * MINUTE).0,
            idle
        );

        let shown = presence("online", None, 0, idle);
        assert_eq!(
            (&shown["status"], &shown["since"], &shown["afk"]),
            (&json!("idle"), &json!(since), &json!(true))
        );
        let shown = presence("online", None, 0, afk);
        assert_eq!(
            (&shown["status"], &shown["since"]),
            (&json!("online"), &json!(0))
        );
        assert_eq!(presence("dnd", None, 0, idle)["status"], "dnd");
    }

    #[test]
    fn reads_the_afk_timeout() {
        // voice_and_video { afk_timeout { value: 300 } }
        let mut wrapper = Vec::new();
        wrapper.extend([1 << 3, 0xac, 0x02]);
        let mut voice = Vec::new();
        put_bytes(&mut voice, VOICE_AFK_TIMEOUT, &wrapper);
        let mut settings = Vec::new();
        put_bytes(&mut settings, VOICE_AND_VIDEO, &voice);
        let encoded = base64::engine::general_purpose::STANDARD.encode(settings);
        assert_eq!(read(&encoded).afk_timeout, Some(300));
        assert_eq!(read(&status_only("dnd")).afk_timeout, None);
    }

    /// A `BoolValue` holding `on`; false goes as an empty one, as Discord
    /// writes it.
    fn bool_wrapper(on: bool) -> Vec<u8> {
        let mut wrapper = Vec::new();
        if on {
            put_varint(&mut wrapper, 1 << 3 | VARINT);
            put_varint(&mut wrapper, 1);
        }
        wrapper
    }

    #[test]
    fn reads_and_writes_the_text_and_image_switches() {
        const SWITCHES: [TextAndImage; 7] = [
            TextAndImage::InlineAttachmentMedia,
            TextAndImage::InlineEmbedMedia,
            TextAndImage::GifAutoPlay,
            TextAndImage::RenderEmbeds,
            TextAndImage::RenderReactions,
            TextAndImage::MessageDisplayCompact,
            TextAndImage::ConvertEmoticons,
        ];
        let value = |settings: &StatusSettings, switch| match switch {
            TextAndImage::InlineAttachmentMedia => settings.inline_attachment_media,
            TextAndImage::InlineEmbedMedia => settings.inline_embed_media,
            TextAndImage::GifAutoPlay => settings.gif_auto_play,
            TextAndImage::RenderEmbeds => settings.render_embeds,
            TextAndImage::RenderReactions => settings.render_reactions,
            TextAndImage::MessageDisplayCompact => settings.message_display_compact,
            TextAndImage::ConvertEmoticons => settings.convert_emoticons,
        };
        let encode = |switch, on| {
            encode_section(
                SettingsSection::TextAndImages,
                &write_text_and_image(&[], switch, on),
            )
        };
        for switch in SWITCHES {
            assert_eq!(value(&read(&encode(switch, true)), switch), Some(true));
            assert_eq!(value(&read(&encode(switch, false)), switch), Some(false));
        }
        let untouched = read(&status_only("idle"));
        for switch in SWITCHES {
            assert_eq!(value(&untouched, switch), None, "the settings don't say");
        }
    }

    #[test]
    fn a_text_and_image_write_keeps_the_rest_of_the_section() {
        let mut current = Vec::new();
        put_bytes(&mut current, INLINE_EMBED_MEDIA, &bool_wrapper(true));
        put_bytes(&mut current, 30, b"kept");
        let written = write_text_and_image(&current, TextAndImage::RenderReactions, false);
        let settings = read(&encode_section(SettingsSection::TextAndImages, &written));
        assert_eq!(settings.inline_embed_media, Some(true));
        assert_eq!(settings.render_reactions, Some(false));
        assert!(
            fields(&written).any(|(number, _)| number == 30),
            "unknown fields stay, as the client keeps them"
        );
    }

    #[test]
    fn reads_and_writes_the_theme_and_developer_mode() {
        for theme in [Theme::Dark, Theme::Light, Theme::Darker, Theme::Midnight] {
            let encoded = encode_section(SettingsSection::Appearance, &write_theme(&[], theme));
            assert_eq!(read(&encoded).theme, Some(theme));
        }
        let mut appearance = Vec::new();
        put_varint(&mut appearance, THEME << 3 | VARINT);
        put_varint(&mut appearance, 3);
        put_varint(&mut appearance, DEVELOPER_MODE << 3 | VARINT);
        put_varint(&mut appearance, 1);
        let settings = read(&encode_section(SettingsSection::Appearance, &appearance));
        assert_eq!(settings.theme, Some(Theme::Darker));
        assert_eq!(settings.developer_mode, Some(true));

        // A section without a theme is dark, as its 0 is; an appearance
        // without developer mode has it off.
        let mut appearance = Vec::new();
        put_varint(&mut appearance, DEVELOPER_MODE << 3 | VARINT);
        put_varint(&mut appearance, 0);
        let settings = read(&encode_section(SettingsSection::Appearance, &appearance));
        assert_eq!(settings.theme, Some(Theme::Dark));
        assert_eq!(settings.developer_mode, Some(false));
        assert_eq!(read(&status_only("idle")).theme, None);

        // A write keeps the fields it doesn't set.
        let written = write_theme(&appearance, Theme::Midnight);
        let settings = read(&encode_section(SettingsSection::Appearance, &written));
        assert_eq!(settings.theme, Some(Theme::Midnight));
        assert_eq!(settings.developer_mode, Some(false));
    }

    #[test]
    fn reads_and_writes_the_locale() {
        let encoded = encode_section(SettingsSection::Localization, &write_locale(&[], "de-DE"));
        assert_eq!(read(&encoded).locale.as_deref(), Some("de-DE"));

        let mut localization = write_locale(&[], "en-US");
        put_bytes(&mut localization, 9, b"kept");
        let written = write_locale(&localization, "fr");
        assert_eq!(
            read(&encode_section(SettingsSection::Localization, &written))
                .locale
                .as_deref(),
            Some("fr")
        );
        assert!(fields(&written).any(|(number, _)| number == 9));
        assert_eq!(read(&status_only("idle")).locale, None);
    }

    #[test]
    fn reads_which_reactions_notify() {
        let encoded = |level: Option<u64>| {
            let mut notifications = Vec::new();
            if let Some(level) = level {
                put_varint(&mut notifications, REACTION_NOTIFICATIONS << 3 | VARINT);
                put_varint(&mut notifications, level);
            }
            let mut settings = Vec::new();
            put_bytes(&mut settings, NOTIFICATIONS, &notifications);
            base64::engine::general_purpose::STANDARD.encode(settings)
        };
        assert_eq!(
            read(&encoded(Some(0))).reaction_notifications,
            Some(ReactionNotifications::All)
        );
        assert_eq!(
            read(&encoded(Some(1))).reaction_notifications,
            Some(ReactionNotifications::OnlyDirectMessages)
        );
        assert_eq!(
            read(&encoded(Some(2))).reaction_notifications,
            Some(ReactionNotifications::Disabled)
        );
        assert_eq!(
            read(&encoded(None)).reaction_notifications,
            Some(ReactionNotifications::All),
            "an enum leaves out its 0"
        );
        assert_eq!(read(&status_only("idle")).reaction_notifications, None);
    }

    #[test]
    fn reads_the_settings_whole() {
        let mut text = Vec::new();
        for switch in [
            INLINE_ATTACHMENT_MEDIA,
            INLINE_EMBED_MEDIA,
            GIF_AUTO_PLAY,
            RENDER_EMBEDS,
            RENDER_REACTIONS,
            MESSAGE_DISPLAY_COMPACT,
            CONVERT_EMOTICONS,
            VIEW_NSFW_COMMANDS,
        ] {
            put_bytes(
                &mut text,
                switch,
                &bool_wrapper(switch != CONVERT_EMOTICONS),
            );
        }
        let mut appearance = Vec::new();
        put_varint(&mut appearance, THEME << 3 | VARINT);
        put_varint(&mut appearance, 4);
        put_varint(&mut appearance, DEVELOPER_MODE << 3 | VARINT);
        put_varint(&mut appearance, 1);
        let mut localization = Vec::new();
        put_bytes(&mut localization, LOCALE, b"\x0a\x05en-GB");
        let mut notifications = Vec::new();
        put_varint(&mut notifications, REACTION_NOTIFICATIONS << 3 | VARINT);
        put_varint(&mut notifications, 2);
        let mut voice = Vec::new();
        let mut timeout = Vec::new();
        put_varint(&mut timeout, 1 << 3 | VARINT);
        put_varint(&mut timeout, 120);
        put_bytes(&mut voice, VOICE_AFK_TIMEOUT, &timeout);
        let mut settings = Vec::new();
        put_bytes(&mut settings, TEXT_AND_IMAGES, &text);
        put_bytes(&mut settings, APPEARANCE, &appearance);
        put_bytes(&mut settings, LOCALIZATION, &localization);
        put_bytes(&mut settings, NOTIFICATIONS, &notifications);
        put_bytes(&mut settings, VOICE_AND_VIDEO, &voice);
        let read = read(&base64::engine::general_purpose::STANDARD.encode(settings));
        assert_eq!(
            read,
            StatusSettings {
                inline_attachment_media: Some(true),
                inline_embed_media: Some(true),
                gif_auto_play: Some(true),
                render_embeds: Some(true),
                render_reactions: Some(true),
                message_display_compact: Some(true),
                convert_emoticons: Some(false),
                nsfw_commands_in_dms: Some(true),
                theme: Some(Theme::Midnight),
                developer_mode: Some(true),
                locale: Some("en-GB".into()),
                reaction_notifications: Some(ReactionNotifications::Disabled),
                afk_timeout: Some(120),
                ..StatusSettings::default()
            }
        );
    }

    #[test]
    fn reads_the_documented_field_numbers() {
        // The numbers come from Discord's PreloadedUserSettings.proto; this
        // holds the constants to them, so a wrong one reads another setting.
        let mut text = Vec::new();
        put_bytes(&mut text, 9, &bool_wrapper(true)); // inline_attachment_media
        put_bytes(&mut text, 10, &bool_wrapper(true)); // inline_embed_media
        put_bytes(&mut text, 11, &bool_wrapper(true)); // gif_auto_play
        put_bytes(&mut text, 12, &bool_wrapper(true)); // render_embeds
        put_bytes(&mut text, 13, &bool_wrapper(true)); // render_reactions
        put_bytes(&mut text, 17, &bool_wrapper(true)); // message_display_compact
        put_bytes(&mut text, 21, &bool_wrapper(false)); // convert_emoticons
        put_bytes(&mut text, 23, &bool_wrapper(true)); // view_nsfw_commands
        let mut appearance = Vec::new();
        put_varint(&mut appearance, 1 << 3 | VARINT); // theme
        put_varint(&mut appearance, 2); // light
        put_varint(&mut appearance, 2 << 3 | VARINT); // developer_mode
        put_varint(&mut appearance, 1);
        let mut localization = Vec::new();
        put_bytes(&mut localization, 1, b"\x0a\x05en-US"); // locale
        let mut notifications = Vec::new();
        put_varint(&mut notifications, 7 << 3 | VARINT); // reaction_notifications
        put_varint(&mut notifications, 1); // only DMs
        let mut settings = Vec::new();
        put_bytes(&mut settings, 6, &text); // text_and_images
        put_bytes(&mut settings, 13, &appearance); // appearance
        put_bytes(&mut settings, 12, &localization); // localization
        put_bytes(&mut settings, 7, &notifications); // notifications
        let read = read(&base64::engine::general_purpose::STANDARD.encode(settings));
        assert_eq!(
            read,
            StatusSettings {
                inline_attachment_media: Some(true),
                inline_embed_media: Some(true),
                gif_auto_play: Some(true),
                render_embeds: Some(true),
                render_reactions: Some(true),
                message_display_compact: Some(true),
                convert_emoticons: Some(false),
                nsfw_commands_in_dms: Some(true),
                theme: Some(Theme::Light),
                developer_mode: Some(true),
                locale: Some("en-US".into()),
                reaction_notifications: Some(ReactionNotifications::OnlyDirectMessages),
                ..StatusSettings::default()
            }
        );
    }

    #[test]
    fn a_section_field_is_what_a_write_of_it_starts_from() {
        let mut settings = Vec::new();
        put_bytes(&mut settings, TEXT_AND_IMAGES, b"\x0a\x01x");
        put_bytes(&mut settings, APPEARANCE, b"\x08\x01");
        let encoded = base64::engine::general_purpose::STANDARD.encode(settings);
        assert_eq!(
            section_field(&encoded, SettingsSection::TextAndImages),
            Some(b"\x0a\x01x".to_vec())
        );
        assert_eq!(
            section_field(&encoded, SettingsSection::Appearance),
            Some(b"\x08\x01".to_vec())
        );
        assert_eq!(section_field(&encoded, SettingsSection::Localization), None);

        // A PATCH carries the one section, whole.
        let patch = encode_section(SettingsSection::Localization, b"\x0a\x05en-GB");
        let data = base64::engine::general_purpose::STANDARD
            .decode(patch)
            .unwrap();
        assert_eq!(
            fields(&data).map(|(number, _)| number).collect::<Vec<_>>(),
            [LOCALIZATION]
        );
    }

    #[test]
    fn a_server_list_with_a_bad_id_writes_what_it_can() {
        // A folder's and a server's ids come from Discord, and none may
        // panic a write.
        let written = write_server_list(&[
            Folder {
                id: "not a number".into(),
                servers: vec!["10".into(), "also not".into()],
                ..Folder::default()
            },
            Folder {
                id: "7".into(),
                servers: vec!["20".into()],
                ..Folder::default()
            },
        ]);
        let list = server_list(&written).unwrap();
        assert_eq!(list.order, ["10", "20"]);
        assert_eq!(list.folders.len(), 1);
        assert_eq!(list.folders[0].id, "7");
    }
}
