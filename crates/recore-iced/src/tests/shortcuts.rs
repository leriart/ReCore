//! Help's card: the app's shortcuts, which Escape, its close button or a
//! click outside closes.

use iced::{
    Event, Point,
    keyboard::{self, key},
    mouse,
};

use super::{interact, preview, shows};
use crate::app::Message;

/// The Help button pressed.
fn help(app: &mut crate::app::App) {
    let _ = app.update(Message::Bar(crate::window_bar::Event::Help));
}

#[test]
fn help_opens_the_shortcuts_and_escape_closes_them() {
    let (mut app, _folder) = preview();
    help(&mut app);
    assert!(app.shortcuts.open);
    assert!(shows(&app, "Keyboard Shortcuts"));
    assert!(shows(&app, "Quick switcher"));

    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        keyboard::Modifiers::default(),
    ));
    assert!(!app.shortcuts.open);
    assert!(!shows(&app, "Keyboard Shortcuts"));
}

#[test]
fn a_click_outside_the_shortcut_card_closes_it() {
    let (mut app, _folder) = preview();
    help(&mut app);
    interact(&mut app, |ui| {
        // Left of the card, which is 600 px wide in the window's middle.
        ui.point_at(Point::new(10.0, 200.0));
        ui.simulate([
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
    });
    assert!(!app.shortcuts.open);
}

#[test]
fn help_again_closes_the_shortcut_card() {
    let (mut app, _folder) = preview();
    help(&mut app);
    help(&mut app);
    assert!(!app.shortcuts.open);
}
