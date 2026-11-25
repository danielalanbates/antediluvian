// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Open beta: the tag every player sees, and the status notice that replaces
//! a silent, empty world when the client cannot get in.
//!
//! Without the notice a player could sit in front of an empty field with no
//! explanation when: their browser runs a cached client older than the server
//! (the server refuses its protocol version), the server cannot be reached,
//! the connection drops (a restart for an update), or a login is refused for
//! another reason (the character is already online in another tab).

use bevy::prelude::*;

use crate::net::{self, Link};
use crate::Session;

/// The beta's one promise, shown under the tag on the splash page
/// (`web/index.html`), in the character builder and in the notice below.
pub const NOTE: &str = "Open beta: progress may be reset. Report bugs to help@batesai.org";
pub const NOTE_COLOR: Color = Color::srgb(0.80, 0.76, 0.64);
const TAG_FG: Color = Color::srgb(0.08, 0.07, 0.05);
const TAG_BG: Color = Color::srgba(0.98, 0.78, 0.30, 0.92);
const PANEL: Color = Color::srgba(0.07, 0.06, 0.05, 0.96);
const EDGE: Color = Color::srgb(0.72, 0.58, 0.30);
const GOLD: Color = Color::srgb(0.98, 0.90, 0.55);
const TEXT: Color = Color::srgb(0.93, 0.90, 0.80);
const BTN: Color = Color::srgb(0.17, 0.15, 0.12);

/// Seconds without an answer before "Connecting" shows, and before it turns
/// into "still connecting".
const SHOW_CONNECTING: f32 = 3.0;
const SLOW_CONNECTING: f32 = 20.0;

/// A small "OPEN BETA" chip.
pub fn tag(p: &mut ChildBuilder, size: f32) {
    p.spawn((
        Node { padding: UiRect::axes(Val::Px(7.0), Val::Px(2.0)), ..default() },
        BackgroundColor(TAG_BG),
        BorderRadius::all(Val::Px(4.0)),
    ))
    .with_children(|c| {
        c.spawn((Text::new("OPEN BETA"), TextFont { font_size: size, ..default() }, TextColor(TAG_FG)));
    });
}

/// What the notice says.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Hidden,
    Connecting,
    SlowConnecting,
    /// The server speaks a newer protocol: this client is out of date.
    NewVersion,
    /// Never connected. (Not "Unreachable": the gate greps logs for "unreachable" panics.)
    NoServer,
    Disconnected,
    Rejected(String),
}

/// Pure decision, so it can be tested without an app.
pub fn status(link: Link, in_world: bool, building: bool, rejected: Option<&str>, waited: f32) -> Status {
    if rejected.is_some_and(|r| r.contains("protocol version mismatch")) {
        return Status::NewVersion;
    }
    match link {
        Link::Failed => return Status::NoServer,
        Link::Closed => return Status::Disconnected,
        Link::Connecting | Link::Open => {}
    }
    if in_world || building {
        return Status::Hidden;
    }
    if let Some(r) = rejected {
        return Status::Rejected(r.to_string());
    }
    if waited >= SLOW_CONNECTING {
        Status::SlowConnecting
    } else if waited >= SHOW_CONNECTING {
        Status::Connecting
    } else {
        Status::Hidden
    }
}

#[cfg(target_arch = "wasm32")]
const REFRESH: &str = "Refresh the page";
#[cfg(not(target_arch = "wasm32"))]
const REFRESH: &str = "Restart the game";

/// (title, body, offer the refresh button).
pub fn words(s: &Status) -> (String, String, bool) {
    match s {
        Status::Hidden => (String::new(), String::new(), false),
        Status::Connecting => ("Connecting to the game server...".into(), String::new(), false),
        Status::SlowConnecting => (
            "Still connecting...".into(),
            format!("The server may be restarting for an update. {REFRESH} in a minute if this does not clear."),
            true,
        ),
        Status::NewVersion => (
            "A new version of Antediluvia is out".into(),
            if cfg!(target_arch = "wasm32") {
                "This page is running an older copy of the game. Refresh the page to load the new one \
                 (Ctrl+Shift+R, or Cmd+Shift+R on a Mac)."
                    .into()
            } else {
                "This copy of the game is older than the server. Update the app to keep playing.".into()
            },
            true,
        ),
        Status::NoServer => (
            "Cannot reach the game server".into(),
            format!("Check your connection. The server may be down for an update: {REFRESH} in a few minutes."),
            true,
        ),
        Status::Disconnected => (
            "Disconnected from the game server".into(),
            format!("The server may be restarting for an update. {REFRESH} to reconnect."),
            true,
        ),
        Status::Rejected(r) => ("Could not log in".into(), r.clone(), false),
    }
}

/// The in-world chip; hidden while the builder (which has its own) is up.
#[derive(Component)]
struct HudTag;
#[derive(Component)]
struct StatusRoot;
#[derive(Component)]
struct StatusTitle;
#[derive(Component)]
struct StatusBody;
#[derive(Component)]
struct RefreshButton;

pub struct BetaPlugin;

impl Plugin for BetaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(Update, (update_status, refresh_click, hud_tag));
    }
}

fn spawn(mut commands: Commands) {
    // The in-world tag: top centre, between the unit frame and the minimap.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(4.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            },
            // Under every window; plain nodes take no clicks.
            GlobalZIndex(-1),
            HudTag,
        ))
        .with_children(|r| {
            tag(r, 11.0);
            r.spawn((
                Text::new("bugs: help@batesai.org"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(NOTE_COLOR.with_alpha(0.8)),
            ));
        });

    // The notice, hidden until needed. Above the builder (z 50).
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(34.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(60),
            Visibility::Hidden,
            StatusRoot,
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(10.0),
                    max_width: Val::Px(560.0),
                    padding: UiRect::all(Val::Px(20.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor(EDGE),
                BorderRadius::all(Val::Px(8.0)),
            ))
            .with_children(|p| {
                tag(p, 12.0);
                p.spawn((Text::new(""), TextFont { font_size: 24.0, ..default() }, TextColor(GOLD), StatusTitle));
                p.spawn((
                    Text::new(""),
                    TextFont { font_size: 15.0, ..default() },
                    TextColor(TEXT),
                    TextLayout::new_with_justify(JustifyText::Center),
                    StatusBody,
                ));
                p.spawn((
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(5.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(BTN),
                    BorderColor(EDGE),
                    BorderRadius::all(Val::Px(4.0)),
                    Visibility::Hidden,
                    RefreshButton,
                ))
                .with_children(|b| {
                    b.spawn((Text::new(REFRESH), TextFont { font_size: 15.0, ..default() }, TextColor(GOLD)));
                });
                p.spawn((Text::new(NOTE), TextFont { font_size: 11.0, ..default() }, TextColor(NOTE_COLOR)));
            });
        });
}

#[allow(clippy::type_complexity)]
fn update_status(
    session: Res<Session>,
    time: Res<Time>,
    mut shown: Local<Option<Status>>,
    mut root: Query<&mut Visibility, (With<StatusRoot>, Without<RefreshButton>)>,
    mut title: Query<&mut Text, (With<StatusTitle>, Without<StatusBody>)>,
    mut body: Query<&mut Text, (With<StatusBody>, Without<StatusTitle>)>,
    mut button: Query<&mut Visibility, (With<RefreshButton>, Without<StatusRoot>)>,
) {
    let st = status(
        net::link(),
        session.sheet.is_some(),
        session.builder.is_some(),
        session.login_error.as_deref(),
        time.elapsed_secs(),
    );
    if shown.as_ref() == Some(&st) {
        return;
    }
    // One console line per change: the browser gate and bug reports read it.
    info!("[beta] status: {st:?}");
    let (t, b, offer) = words(&st);
    for mut v in &mut root {
        *v = if st == Status::Hidden { Visibility::Hidden } else { Visibility::Inherited };
    }
    for mut x in &mut title {
        **x = t.clone();
    }
    for mut x in &mut body {
        **x = b.clone();
    }
    // Native has nothing to reload; the words say what to do instead.
    for mut v in &mut button {
        *v = if offer && cfg!(target_arch = "wasm32") { Visibility::Inherited } else { Visibility::Hidden };
    }
    *shown = Some(st);
}

fn hud_tag(session: Res<Session>, mut q: Query<&mut Visibility, With<HudTag>>) {
    let want = if session.builder.is_some() { Visibility::Hidden } else { Visibility::Inherited };
    for mut v in &mut q {
        if *v != want {
            *v = want;
        }
    }
}

fn refresh_click(q: Query<&Interaction, (Changed<Interaction>, With<RefreshButton>)>) {
    for i in &q {
        if *i == Interaction::Pressed {
            #[cfg(target_arch = "wasm32")]
            if let Some(w) = web_sys::window() {
                // index.html is revalidated on every load and names the
                // engine by build stamp, so a reload picks up a new build.
                let _ = w.location().reload();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_client_is_told_to_refresh_not_left_hanging() {
        let s = status(Link::Open, false, false, Some("protocol version mismatch - a new version is out"), 1.0);
        assert_eq!(s, Status::NewVersion);
        let (title, body, offer) = words(&s);
        assert!(title.contains("new version"), "{title}");
        assert!(!body.is_empty() && offer);
    }

    #[test]
    fn the_notice_stays_out_of_a_normal_session() {
        // Logging in: nothing for the first seconds, then "Connecting".
        assert_eq!(status(Link::Connecting, false, false, None, 1.0), Status::Hidden);
        assert_eq!(status(Link::Open, false, false, None, 4.0), Status::Connecting);
        assert_eq!(status(Link::Open, false, false, None, 25.0), Status::SlowConnecting);
        // In the builder or in the world: hidden.
        assert_eq!(status(Link::Open, false, true, None, 60.0), Status::Hidden);
        assert_eq!(status(Link::Open, true, false, None, 600.0), Status::Hidden);
    }

    #[test]
    fn a_dropped_or_refused_connection_says_so() {
        assert_eq!(status(Link::Closed, true, false, None, 900.0), Status::Disconnected);
        assert_eq!(status(Link::Failed, false, false, None, 2.0), Status::NoServer);
        assert_eq!(
            status(Link::Open, false, false, Some("that character is already online"), 2.0),
            Status::Rejected("that character is already online".into())
        );
    }
}
