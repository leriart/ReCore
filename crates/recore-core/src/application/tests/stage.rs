//! Stage channels: their topic, who speaks and who listens, raising a hand
//! to speak, and a moderator moving someone between the two.

use super::*;

use std::sync::RwLock;

use recore_discord::{
    marked::{MarkedServer, ServerMarks},
    message::Names,
    ready::{self, permission},
    service::{SharedNames, Snapshot, StageInstance, Status, VoiceMember},
};
use serde_json::json;

/// Alice, "1", is Staff in a server with a stage channel, "15", whose stage
/// instance she listens to beside Cara, "3", who speaks. Alice may mute
/// members, and so move them between the stage's audience and speakers.
fn stage_app() -> Application {
    let staff = permission::MUTE_MEMBERS | permission::MOVE_MEMBERS;
    let server = ready::server(&json!({
        "id": "10", "owner_id": "99",
        "properties": {"name": "Test"},
        "roles": [
            {"id": "10", "name": "@everyone", "position": 0, "permissions": (
                permission::VIEW_CHANNEL | permission::CONNECT).to_string()},
            {"id": "30", "name": "Staff", "position": 1, "permissions": staff.to_string()},
        ],
        "channels": [
            {"id": "11", "type": 0, "name": "general", "position": 0},
            {"id": "15", "type": 13, "name": "Stage", "position": 1},
        ],
    }))
    .unwrap();
    let marks = ServerMarks {
        roles: vec!["30".into()],
        ..Default::default()
    };
    let mut names = Names::default();
    for (user, name) in [("1", "Alice"), ("3", "Cara"), ("4", "Dave")] {
        names.learn_member(
            "10",
            &json!({
                "user": {"id": user, "username": name.to_lowercase(), "global_name": name},
                "roles": if user == "1" { vec!["30"] } else { vec![] },
            }),
        );
    }
    let person = |user: &str, name: &str, suppressed: bool, requested: bool| VoiceMember {
        user_id: user.into(),
        name: name.into(),
        avatar: String::new(),
        muted: false,
        deafened: false,
        server_muted: false,
        server_deafened: false,
        streaming: false,
        video: false,
        elsewhere: false,
        suppressed,
        requested,
    };
    let mut app = Application::default();
    app.apply(Event::Discord(Arc::new(Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        servers: vec![MarkedServer {
            server: Arc::new(server),
            marks: Arc::new(marks),
        }],
        stage_instances: Arc::new(
            [(
                "15".to_owned(),
                StageInstance {
                    id: "16".into(),
                    channel_id: "15".into(),
                    topic: "Launch day".into(),
                    privacy_level: 2,
                },
            )]
            .into(),
        ),
        voice: [(
            "15".to_owned(),
            vec![
                person("1", "Alice", true, false),
                person("3", "Cara", false, false),
                person("4", "Dave", true, true),
            ],
        )]
        .into(),
        names: SharedNames::new(Arc::new(RwLock::new(names))),
        ..Default::default()
    })));
    app.dispatch(Action::Navigate(Navigation::Server("discord:10".into())));
    // Opening the stage would join its call, which this test doesn't need.
    app.state.selected = Some("discord:10:15".into());
    app
}

#[test]
fn a_stage_shows_its_topic_and_who_speaks() {
    let app = stage_app();
    let room = app.state.selected_room().unwrap();
    assert_eq!(room.mark.kind, chat::MarkKind::Stage);
    assert_eq!(
        app.state.stage_topics["discord:10:15"].as_ref(),
        "Launch day"
    );
    let (speakers, audience): (Vec<_>, Vec<_>) =
        room.people.iter().partition(|person| !person.suppressed);
    assert_eq!(
        speakers
            .iter()
            .map(|person| person.name.as_str())
            .collect::<Vec<_>>(),
        ["Cara"]
    );
    assert_eq!(
        audience
            .iter()
            .map(|person| person.name.as_str())
            .collect::<Vec<_>>(),
        ["Alice", "Dave"]
    );
    assert!(audience[1].requested, "Dave's hand is raised");
}

#[test]
fn asking_to_speak_is_a_stage_command() {
    let mut app = stage_app();
    let room = "discord:10:15".to_owned();
    assert!(matches!(
        app.dispatch(Action::RequestToSpeak {
            room: room.clone(),
            requested: true,
        })
        .as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::RequestToSpeak {
            room: asked,
            requested: true,
        }))] if *asked == room
    ));
    app.dispatch(Action::RequestToSpeak {
        room: room.clone(),
        requested: false,
    });
    // A failure says why where the call's errors show.
    app.apply(Event::SpeakRequested {
        room,
        requested: true,
        result: Err("Discord said no".into()),
    });
    assert!(
        app.state
            .voice
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Discord said no"))
    );
}

#[test]
fn a_moderator_moves_people_between_speakers_and_audience() {
    let mut app = stage_app();
    let room = "discord:10:15";
    assert!(
        app.voice_person_choices(room, "4")
            .contains(&MenuChoice::MoveToSpeakers),
        "Dave listens, so he may be moved to speak"
    );
    assert!(
        app.voice_person_choices(room, "3")
            .contains(&MenuChoice::MoveToAudience),
        "Cara speaks, so she may be moved to the audience"
    );
    assert!(matches!(
        app.dispatch(Action::ChangeMember {
            place: room.into(),
            user: "4".into(),
            change: chat::MemberChange::Suppress(false),
        })
        .as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::Member {
            user,
            change: chat::MemberChange::Suppress(false),
            ..
        }))] if user == "4"
    ));
}

#[test]
fn sets_a_stages_topic() {
    let mut app = stage_app();
    assert!(matches!(
        app.dispatch(Action::SetStageTopic {
            room: "discord:10:15".into(),
            topic: "  A new talk  ".into(),
        })
        .as_slice(),
        [Effect::Backend(Command::Discord(DiscordCommand::SetStageTopic {
            topic,
            ..
        }))] if topic == "A new talk"
    ));
    app.apply(Event::StageTopicSet {
        room: "discord:10:15".into(),
        result: Err("Discord said no".into()),
    });
    assert!(
        app.state
            .voice
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Discord said no"))
    );
}
