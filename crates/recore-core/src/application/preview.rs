//! The preview, which shows ReCore with sample rooms and no account.

use super::*;

impl Application {
    pub(super) fn start_preview(&mut self) -> Vec<Effect> {
        if self.state.busy || self.state.account.is_some() || self.state.preview {
            return vec![];
        }
        self.state.preview = true;
        // Sample avatars are drawn here, so the preview shows faces without
        // a network or an account.
        for name in ["You", "Sam", "Riley", "Morgan", "Friday crew"] {
            self.state.pictures.insert(
                preview_avatar_url(name),
                std::sync::Arc::new(preview_avatar(name)),
            );
        }
        self.state.servers = [
            ChatServer {
                id: "friends".into(),
                name: "Friends".into(),
                channels: vec![
                    "general".into(),
                    "games".into(),
                    "weekend".into(),
                    "lounge".into(),
                ],
                discord: false,
                muted: false,
                unavailable: false,
                icon: None,
                folder: None,
                hide_muted: false,
                community: false,
                pinned: Vec::new(),
                ..Default::default()
            },
            ChatServer {
                id: "coop".into(),
                name: "Co-op crew".into(),
                channels: vec![
                    "announcements".into(),
                    "looking-for-group".into(),
                    "party-voice".into(),
                ],
                discord: false,
                muted: false,
                unavailable: false,
                icon: None,
                folder: None,
                hide_muted: false,
                community: false,
                pinned: Vec::new(),
                ..Default::default()
            },
        ]
        .into_iter()
        .map(Arc::new)
        .collect();
        self.state.navigation = Navigation::Server("friends".into());
        self.state.rooms = [
            preview_room("general", "general", false, 4),
            preview_room("games", "games", false, 4),
            preview_room("weekend", "weekend plans", false, 4),
            preview_room("announcements", "announcements", false, 3),
            preview_room("looking-for-group", "looking-for-group", false, 3),
            ChatRoom {
                voice: true,
                people: preview_voice_people(&["Sam", "Riley", "Morgan"]),
                ..preview_room("lounge", "Lounge", false, 4)
            },
            ChatRoom {
                voice: true,
                people: preview_voice_people(&["Riley", "Sam"]),
                ..preview_room("party-voice", "Party", false, 3)
            },
            preview_room("sam", "Sam", true, 2),
            preview_room("riley", "Riley", true, 2),
            preview_room("party", "Friday crew", true, 3),
        ]
        .into_iter()
        .map(Arc::new)
        .collect();
        self.state.list_changes = self.state.list_changes.wrapping_add(1);
        for (room, lines) in [
            (
                "announcements",
                vec![(
                    "Riley",
                    "Welcome to Co-op crew. This server has its own channels.",
                )],
            ),
            (
                "looking-for-group",
                vec![("Sam", "Looking for one more for co-op tonight.")],
            ),
            (
                "general",
                vec![
                    ("Sam", "Anyone around tonight?"),
                    (
                        "Sam",
                        "Thinking **co-op** after nine, if people are up for it.",
                    ),
                    ("Riley", "Yep. I should be free after dinner."),
                    ("You", "Nice, let's get a game going."),
                    (
                        "Sam",
                        "This is the local preview. Pick a conversation on the left, or send a message below to try it out.",
                    ),
                ],
            ),
            (
                "games",
                vec![
                    ("Riley", "What are we playing next?"),
                    ("Sam", "Something co-op. I'm open to suggestions."),
                ],
            ),
            (
                "sam",
                vec![(
                    "Sam",
                    "Hey! This is a sample DM. Your preview messages stay on this computer.",
                )],
            ),
            (
                "party",
                vec![
                    ("Riley", "Who's joining on Friday?"),
                    ("Sam", "Count me in."),
                ],
            ),
        ] {
            self.samples.insert(
                room.into(),
                lines
                    .into_iter()
                    .enumerate()
                    .map(|(i, (author, body))| {
                        Arc::new(ChatMessage {
                            id: format!("{room}-{i}"),
                            item: format!("{room}-{i}"),
                            author: author.into(),
                            sender: author.into(),
                            formatted: has_markdown(body),
                            body: body.into(),
                            time: format!("19:{:02}", 20 + i),
                            own: author == "You",
                            can_edit: author == "You",
                            can_delete: author == "You",
                            avatar: Some(preview_avatar_url(author)),
                            ..sample_actions()
                        })
                    })
                    .collect(),
            );
        }
        self.select("general".into())
    }

    /// The preview's devices as listed for it; nothing else about voice is real.
    pub fn preview_devices(&mut self, devices: Result<voice::Devices, String>) {
        if !self.state.preview {
            return;
        }
        match devices {
            Ok(devices) => {
                self.state.voice.microphones = devices.microphones;
                self.state.voice.speakers = devices.speakers;
                self.state.voice.cameras = devices.cameras;
            }
            Err(error) => self.state.voice.error = Some(error),
        }
    }

    /// A preview call's starting state, keeping the devices listed and chosen so far.
    pub(super) fn preview_voice(&self) -> voice::State {
        let voice = &self.state.voice;
        voice::State {
            microphones: voice.microphones.clone(),
            speakers: voice.speakers.clone(),
            cameras: voice.cameras.clone(),
            microphone: voice.microphone.clone(),
            speaker: voice.speaker.clone(),
            camera: voice.camera.clone(),
            ..Default::default()
        }
    }
}

pub(super) fn preview_room(id: &str, name: &str, direct: bool, members: u64) -> ChatRoom {
    ChatRoom {
        id: id.into(),
        network: Network::of(id),
        capabilities: Network::of(id).capabilities(),
        name: name.into(),
        direct,
        voice: false,
        encrypted: true,
        members,
        unread: 0,
        mentions: 0,
        muted: false,
        last_read: None,
        locked: false,
        join_locked: false,
        read_only: false,
        permissions: None,
        reacts: Reacts::Any,
        slowed: false,
        timed_out_until: None,
        category: String::new(),
        category_id: None,
        request: false,
        person: (direct && members <= 2).then(|| format!("@{id}:preview.local")),
        people: vec![],
        ringing: vec![],
        incoming: false,
        avatar: direct.then(|| preview_avatar_url(name)),
        decoration: None,
        nameplate: None,
        style: None,
        upload_limit: 50 * 1024 * 1024,
        presence: None,
        activity: None,
        activities: Default::default(),
        parent: None,
        forum: false,
        status: None,
        mark: Default::default(),
        unlisted: false,
        tag: None,
        official: false,
        empty: false,
        preview: None,
        voice_started: None,
    }
}

/// Sample members whose names match the preview's messages.
pub(super) fn preview_members(room: &str) -> Vec<ChatMember> {
    use Role::{Admin, Member, Moderator};
    let people: &[(&str, Role)] = match room {
        "announcements" | "looking-for-group" => {
            &[("Riley", Admin), ("Sam", Member), ("You", Member)]
        }
        "party" => &[("Riley", Member), ("Sam", Member), ("You", Member)],
        "sam" => &[("Sam", Member), ("You", Member)],
        "riley" => &[("Riley", Member), ("You", Member)],
        _ => &[
            ("You", Admin),
            ("Sam", Moderator),
            ("Riley", Member),
            ("Morgan", Member),
        ],
    };
    let mut members: Vec<_> = people
        .iter()
        .map(|&(name, role)| ChatMember {
            bot: false,
            id: format!("@{}:preview.local", name.to_lowercase()),
            name: name.into(),
            username: None,
            role,
            avatar: Some(preview_avatar_url(name)),
            decoration: None,
            nameplate: None,
            style: None,
            group: None,
            presence: None,
            color: None,
            activity: None,
            activities: Default::default(),
            roles: Default::default(),
            timed_out_until: None,
            tag: None,
            group_icon: None,
            role_icon: None,
            boosting_since: None,
        })
        .collect();
    chat::sort_members(&mut members);
    members
}

fn preview_voice_people(names: &[&str]) -> Vec<chat::VoicePerson> {
    names
        .iter()
        .map(|name| chat::VoicePerson {
            id: format!("@{}:preview.local", name.to_ascii_lowercase()),
            name: (*name).into(),
            avatar: Some(preview_avatar_url(name)),
            ..Default::default()
        })
        .collect()
}

fn preview_avatar_url(name: &str) -> String {
    format!("preview:{}", name.to_ascii_lowercase().replace(' ', "-"))
}

/// A locally drawn sample face: a soft two-colour gradient with a mirrored
/// five-by-five mark, so every preview person has a different picture.
fn preview_avatar(name: &str) -> crate::chat::Picture {
    const SIZE: u32 = 96;
    let hash = name.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    // Eight evenly spaced hues keep neighbouring sample people apart,
    // where a raw hash can land two of them on nearly the same green.
    let hue = ((hash % 8) * 45) as f32;
    let top = preview_hsv(hue, 0.62, 0.88);
    let bottom = preview_hsv((hue + 28.0) % 360.0, 0.68, 0.62);
    let mut rgba = vec![0; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        let mix = y as f32 / (SIZE - 1) as f32;
        for x in 0..SIZE {
            let at = ((y * SIZE + x) * 4) as usize;
            let (r, g, b) = preview_mix(top, bottom, mix);
            rgba[at..at + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    let cell = 12;
    let origin = (SIZE - 5 * cell) / 2;
    for row in 0..5 {
        for column in 0..3 {
            if hash >> (row * 3 + column) & 1 == 0 {
                continue;
            }
            for x in [column, 4 - column] {
                let left = origin + x * cell + 2;
                let top = origin + row * cell + 2;
                for y in top..top + cell - 4 {
                    for x in left..left + cell - 4 {
                        let at = ((y * SIZE + x) * 4) as usize;
                        rgba[at..at + 4].copy_from_slice(&[242, 238, 232, 255]);
                    }
                }
            }
        }
    }
    crate::chat::Picture {
        width: SIZE,
        height: SIZE,
        rgba: crate::chat::Pixels::new(SIZE, SIZE, rgba),
        animation: None,
    }
}

fn preview_hsv(hue: f32, saturation: f32, value: f32) -> (u8, u8, u8) {
    let sector = (hue / 60.0) % 6.0;
    let chroma = value * saturation;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = value - chroma;
    let channel = |channel: f32| ((channel + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (channel(r), channel(g), channel(b))
}

fn preview_mix(from: (u8, u8, u8), to: (u8, u8, u8), amount: f32) -> (u8, u8, u8) {
    let channel = |a: u8, b: u8| {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
    )
}

/// Preview messages support every local action; sent ones are also editable.
pub(super) fn sample_actions() -> ChatMessage {
    ChatMessage {
        can_reply: true,
        can_edit: true,
        can_delete: true,
        can_react: true,
        can_forward: true,
        can_crosspost: false,
        can_pin: true,
        ..Default::default()
    }
}
