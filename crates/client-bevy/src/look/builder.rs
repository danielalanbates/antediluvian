// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! The character builder: the screen a new account lands on.
//!
//! A panel on the left lists every choice as a line with arrows; the
//! character stands on the right, wearing the look as it changes. The name
//! is typed directly (letters only), Up/Down move between lines, Left/Right
//! change the highlighted line, F5 or the Randomise button rolls a whole
//! look, Enter or the Create button submits. Every line is also clickable.
//!
//! The server is the authority: it re-validates the name with the same
//! `antediluvia_sim::names::validate_name` used here for live feedback and
//! clamps the look. A refusal comes back as `Builder::error`.

use antediluvia_protocol::look::{Look, LOOK_FIELDS};
use antediluvia_protocol::{CharacterCreate, Class, ClientMsg};
use bevy::gltf::GltfAssetLabel;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use super::apply::LookRig;
use super::{
    body_scale, field_label, option_name, swatch, F_BEARD, F_BUILD, F_DYE, F_EYES, F_FACE, F_HAIR,
    F_HAIR_COLOR, F_HEIGHT, F_MANTLE, F_MARKING, F_MARK_COLOR, F_SKIN, OUTFIT, OUTFIT_FILES,
};
use crate::net::NetTx;

/// Character-builder working state.
pub struct Builder {
    pub name: String,
    pub class: Class,
    pub faction: Option<String>,
    /// `[outfit, legacy skin, legacy hair]`; only the outfit is chosen here.
    pub appearance: [u32; 3],
    pub look: Look,
    /// Highlighted line (index into [`ROWS`]).
    pub row: usize,
    /// How many times Randomise has been used (part of the next roll's seed).
    pub rolls: u64,
    pub submitted: bool,
    pub error: Option<String>,
    /// Set by a scripted run to press Create on the next frame.
    pub want_submit: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            name: String::new(),
            class: Class::Warrior,
            faction: None,
            appearance: [0, 0, 0],
            look: Look { skin: 2, build: 2, height: 4, face: 5, ..Look::default() },
            row: 0,
            rolls: 0,
            submitted: false,
            error: None,
            want_submit: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Class,
    Lineage,
    Outfit,
    Field(usize),
}

/// The lines of the builder, top to bottom.
pub const ROWS: [Row; 15] = [
    Row::Class,
    Row::Lineage,
    Row::Outfit,
    Row::Field(F_MANTLE),
    Row::Field(F_DYE),
    Row::Field(F_SKIN),
    Row::Field(F_FACE),
    Row::Field(F_EYES),
    Row::Field(F_HAIR),
    Row::Field(F_HAIR_COLOR),
    Row::Field(F_BEARD),
    Row::Field(F_BUILD),
    Row::Field(F_HEIGHT),
    Row::Field(F_MARKING),
    Row::Field(F_MARK_COLOR),
];

const LINEAGES: [Option<&str>; 3] = [None, Some("sethite"), Some("cainite")];

pub fn row_label(row: Row) -> &'static str {
    match row {
        Row::Class => "Class",
        Row::Lineage => "Lineage",
        Row::Outfit => "Outfit",
        Row::Field(f) => field_label(f),
    }
}

/// (current index, number of options) of a line.
pub fn row_position(b: &Builder, row: Row) -> (usize, usize) {
    match row {
        Row::Class => (Class::ALL.iter().position(|c| *c == b.class).unwrap_or(0), Class::ALL.len()),
        Row::Lineage => (LINEAGES.iter().position(|l| *l == b.faction.as_deref()).unwrap_or(0), LINEAGES.len()),
        Row::Outfit => (b.appearance[0] as usize % OUTFIT.len(), OUTFIT.len()),
        Row::Field(f) => (b.look.to_array()[f] as usize, LOOK_FIELDS[f].count as usize),
    }
}

pub fn row_value(b: &Builder, row: Row) -> String {
    match row {
        Row::Class => antediluvia_sim::abilities::class_blurb(b.class).0.to_string(),
        Row::Lineage => match b.faction.as_deref() {
            None => "Undecided (choose at level 10)".into(),
            Some("sethite") => "Sethite".into(),
            Some(_) => "Cainite".into(),
        },
        Row::Outfit => OUTFIT[b.appearance[0] as usize % OUTFIT.len()].into(),
        Row::Field(f) => option_name(f, b.look.to_array()[f]),
    }
}

/// Move a line one option forward or back, wrapping.
pub fn step(b: &mut Builder, row: Row, delta: i32) {
    let (i, n) = row_position(b, row);
    let j = (i as i32 + delta).rem_euclid(n as i32) as usize;
    match row {
        Row::Class => b.class = Class::ALL[j],
        Row::Lineage => b.faction = LINEAGES[j].map(str::to_string),
        Row::Outfit => b.appearance[0] = j as u32,
        Row::Field(f) => {
            let mut a = b.look.to_array();
            a[f] = j as u8;
            b.look = Look::from_array(a);
        }
    }
}

/// Roll a whole look (and outfit). `entropy` varies the result between
/// sessions; the roll counter varies it between presses.
pub fn randomise(b: &mut Builder, entropy: u64) {
    b.rolls += 1;
    let seed = entropy ^ b.rolls.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    b.look = Look::random(seed);
    b.appearance[0] = ((seed >> 17) % OUTFIT.len() as u64) as u32;
}

/// What the name line shows under the field: `Ok` text or the refusal.
pub fn name_status(name: &str) -> Result<String, &'static str> {
    antediluvia_sim::names::validate_name(name)
}

/// The login message for the current choices, or why it cannot be sent.
pub fn create_message(b: &Builder, apple_id: &str) -> Result<ClientMsg, &'static str> {
    let name = name_status(&b.name)?;
    let look = b.look.clamped();
    Ok(ClientMsg::Login {
        proto: antediluvia_protocol::PROTOCOL_VERSION,
        apple_id: apple_id.to_string(),
        character_name: None,
        create: Some(CharacterCreate {
            name,
            class: b.class,
            faction: b.faction.clone(),
            // Old servers read skin and hair from here; keep them in range.
            appearance: [b.appearance[0] % 4, (look.skin as u32).min(15), (look.hair_color as u32).min(11)],
            look: look.pack(),
        }),
        wire: crate::net::wanted_wire(),
    })
}

/// Scripted control for windowless runs (`ANTEDILUVIA_AUTOCMD="look ..."`):
/// `random`, `name <text>`, `row <n>`, `step <n> <delta>`, `outfit <n>`,
/// `set <field> <value>`, `create`.
pub fn autocmd(b: &mut Builder, arg: &str) {
    let parts: Vec<&str> = arg.split_whitespace().collect();
    match parts.as_slice() {
        ["random"] => randomise(b, 0xA17E),
        ["random", n] => {
            b.rolls = n.parse().unwrap_or(0);
            randomise(b, 0xA17E);
        }
        ["name", n] => b.name = n.to_string(),
        ["row", n] => b.row = n.parse::<usize>().unwrap_or(0) % ROWS.len(),
        ["step", n, d] => {
            let row = ROWS[n.parse::<usize>().unwrap_or(0) % ROWS.len()];
            step(b, row, d.parse().unwrap_or(1));
        }
        ["outfit", n] => b.appearance[0] = n.parse::<u32>().unwrap_or(0) % 4,
        ["set", field, v] => {
            if let Some(f) = LOOK_FIELDS.iter().position(|x| x.name == *field) {
                let mut a = b.look.to_array();
                a[f] = v.parse::<u8>().unwrap_or(0) % LOOK_FIELDS[f].count;
                b.look = Look::from_array(a);
            }
        }
        ["create"] => b.want_submit = true,
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct BuilderUi;

#[derive(Component)]
pub struct BuilderPreview {
    outfit: u32,
}

#[derive(Component, Clone, Copy, PartialEq)]
pub enum Slot {
    Name,
    NameStatus,
    Value(usize),
    Count(usize),
    Blurb,
    Status,
}

#[derive(Component)]
pub struct RowLine(usize);

#[derive(Component)]
pub struct RowSwatch(usize);

#[derive(Component, Clone, Copy)]
pub enum Press {
    Step(usize, i32),
    Select(usize),
    Random,
    Create,
}

const PANEL: Color = Color::srgba(0.07, 0.06, 0.05, 0.98);
const EDGE: Color = Color::srgb(0.72, 0.58, 0.30);
const GOLD: Color = Color::srgb(0.98, 0.90, 0.55);
const TEXT: Color = Color::srgb(0.93, 0.90, 0.80);
const DIM: Color = Color::srgb(0.62, 0.60, 0.54);
const BAD: Color = Color::srgb(0.95, 0.45, 0.35);
const GOOD: Color = Color::srgb(0.55, 0.85, 0.50);
const BTN: Color = Color::srgb(0.17, 0.15, 0.12);
const BTN_HOVER: Color = Color::srgb(0.30, 0.25, 0.16);
const ROW_ON: Color = Color::srgba(0.72, 0.58, 0.30, 0.22);
const ROW_OFF: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);

fn label(p: &mut ChildBuilder, text: &str, size: f32, color: Color, width: Option<f32>) {
    let mut node = Node::default();
    if let Some(w) = width {
        node.width = Val::Px(w);
    }
    p.spawn((Text::new(text), TextFont { font_size: size, ..default() }, TextColor(color), node));
}

fn button(p: &mut ChildBuilder, text: &str, press: Press, size: f32, pad: f32) {
    p.spawn((
        Button,
        Node {
            padding: UiRect::axes(Val::Px(pad), Val::Px(3.0)),
            border: UiRect::all(Val::Px(1.0)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(BTN),
        BorderColor(EDGE),
        BorderRadius::all(Val::Px(4.0)),
        press,
    ))
    .with_children(|b| {
        b.spawn((Text::new(text), TextFont { font_size: size, ..default() }, TextColor(GOLD)));
    });
}

fn spawn_screen(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                // Covers the (empty) unit frame the pre-login HUD still draws.
                left: Val::Px(8.0),
                top: Val::Px(8.0),
                width: Val::Px(560.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                padding: UiRect::all(Val::Px(18.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor(EDGE),
            BorderRadius::all(Val::Px(8.0)),
            // In front of the HUD panels the pre-login screen still draws.
            GlobalZIndex(50),
            BuilderUi,
        ))
        .with_children(|p| {
            label(p, "Create your character", 28.0, GOLD, None);
            label(p, "Type a name.  Up / Down pick a line,  Left / Right change it,  or click the arrows.", 12.0, DIM, None);
            // Name line.
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                margin: UiRect::vertical(Val::Px(6.0)),
                ..default()
            })
            .with_children(|r| {
                label(r, "Name", 17.0, TEXT, Some(128.0));
                r.spawn((
                    Node {
                        width: Val::Px(210.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.02, 0.02, 0.02)),
                    BorderColor(EDGE),
                ))
                .with_children(|bx| {
                    bx.spawn((Text::new("_"), TextFont { font_size: 18.0, ..default() }, TextColor(GOLD), Slot::Name));
                });
                r.spawn((Text::new(""), TextFont { font_size: 12.0, ..default() }, TextColor(DIM), Slot::NameStatus));
            });
            for (i, row) in ROWS.iter().enumerate() {
                p.spawn((
                    Button,
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(8.0),
                        padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(ROW_OFF),
                    BorderRadius::all(Val::Px(4.0)),
                    RowLine(i),
                    Press::Select(i),
                ))
                .with_children(|r| {
                    label(r, row_label(*row), 15.0, TEXT, Some(124.0));
                    button(r, "<", Press::Step(i, -1), 14.0, 9.0);
                    r.spawn((
                        Node { width: Val::Px(16.0), height: Val::Px(16.0), border: UiRect::all(Val::Px(1.0)), ..default() },
                        BackgroundColor(Color::BLACK),
                        BorderColor(Color::srgb(0.0, 0.0, 0.0)),
                        RowSwatch(i),
                    ));
                    r.spawn((
                        Text::new(""),
                        TextFont { font_size: 15.0, ..default() },
                        TextColor(GOLD),
                        Node { width: Val::Px(250.0), ..default() },
                        Slot::Value(i),
                    ));
                    r.spawn((
                        Text::new(""),
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(DIM),
                        Node { width: Val::Px(46.0), ..default() },
                        Slot::Count(i),
                    ));
                    button(r, ">", Press::Step(i, 1), 14.0, 9.0);
                });
            }
            p.spawn((
                Text::new(""),
                TextFont { font_size: 12.0, ..default() },
                TextColor(DIM),
                Node { margin: UiRect::top(Val::Px(6.0)), ..default() },
                Slot::Blurb,
            ));
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                margin: UiRect::top(Val::Px(8.0)),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|r| {
                button(r, "Randomise  (F5)", Press::Random, 15.0, 14.0);
                button(r, "Enter the world  (Enter)", Press::Create, 15.0, 14.0);
            });
            p.spawn((Text::new(""), TextFont { font_size: 14.0, ..default() }, TextColor(BAD), Slot::Status));
            label(
                p,
                &format!(
                    "{} options across {} lines; about {:.1e} different characters.",
                    Look::option_count() + Class::ALL.len() as u32 + 3,
                    ROWS.len(),
                    Look::combinations() as f64
                ),
                11.0,
                DIM,
                None,
            );
        });
}

/// Where the preview stands and where the camera sits while building.
const PREVIEW_AT: Vec3 = Vec3::new(0.0, 0.0, 40.0);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn builder_screen(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut kb_events: EventReader<KeyboardInput>,
    mut session: ResMut<crate::Session>,
    tx: Res<NetTx>,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    ui_root: Query<Entity, With<BuilderUi>>,
    mut texts: Query<(&Slot, &mut Text, &mut TextColor)>,
    mut swatches: Query<(&RowSwatch, &mut BackgroundColor, &mut Visibility)>,
    mut presses: Query<(Ref<Interaction>, &Press, Option<&RowLine>, &mut BackgroundColor), Without<RowSwatch>>,
    mut preview: Query<(Entity, &BuilderPreview, &mut Transform, &mut LookRig), Without<crate::MainCamera>>,
    mut cam: Query<&mut Transform, (With<crate::MainCamera>, Without<BuilderPreview>)>,
) {
    let apple_id = session.apple_id.clone();
    let act = session.act;
    let Some(b) = session.builder.as_mut() else {
        for e in &ui_root {
            commands.entity(e).despawn_recursive();
        }
        for (e, ..) in &preview {
            commands.entity(e).despawn_recursive();
        }
        kb_events.clear();
        return;
    };

    // ── Keyboard ─────────────────────────────────────────────────────────
    let mut typed = false;
    for ev in kb_events.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        match &ev.logical_key {
            Key::Character(c) => {
                for ch in c.chars() {
                    if (ch.is_ascii_alphabetic() || ch == '\'' || ch == '-') && b.name.chars().count() < antediluvia_sim::names::NAME_MAX {
                        b.name.push(ch);
                        typed = true;
                    }
                }
            }
            Key::Backspace => {
                b.name.pop();
                typed = true;
            }
            _ => {}
        }
    }
    if typed {
        b.error = None;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        b.row = (b.row + 1) % ROWS.len();
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        b.row = (b.row + ROWS.len() - 1) % ROWS.len();
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        step(b, ROWS[b.row], 1);
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        step(b, ROWS[b.row], -1);
    }
    let entropy = (time.elapsed_secs_f64() * 1000.0) as u64;
    let mut submit = keys.just_pressed(KeyCode::Enter) || std::mem::take(&mut b.want_submit);
    if keys.just_pressed(KeyCode::F5) {
        randomise(b, entropy);
    }

    // ── Mouse ────────────────────────────────────────────────────────────
    for (interaction, press, line, mut bg) in &mut presses {
        // Lines show the selection; buttons show the pointer.
        let want = match line {
            Some(l) if l.0 == b.row => ROW_ON,
            Some(_) => ROW_OFF,
            None if *interaction == Interaction::None => BTN,
            None => BTN_HOVER,
        };
        if bg.0 != want {
            bg.0 = want;
        }
        if !interaction.is_changed() || *interaction != Interaction::Pressed {
            continue;
        }
        match *press {
            Press::Step(i, d) => {
                b.row = i;
                step(b, ROWS[i], d);
            }
            Press::Select(i) => b.row = i,
            Press::Random => randomise(b, entropy),
            Press::Create => submit = true,
        }
    }

    if submit && !b.submitted {
        match create_message(b, &apple_id) {
            Ok(msg) => {
                b.submitted = true;
                b.error = None;
                tx.send(msg);
            }
            Err(why) => b.error = Some(why.to_string()),
        }
    }

    // ── Panel ────────────────────────────────────────────────────────────
    if ui_root.is_empty() {
        spawn_screen(&mut commands);
    }
    let status = name_status(&b.name);
    for (slot, mut text, mut color) in &mut texts {
        let (s, c) = match *slot {
            Slot::Name => (format!("{}_", b.name), GOLD),
            Slot::NameStatus => match (&status, b.name.is_empty()) {
                (_, true) => ("3 to 16 letters".to_string(), DIM),
                (Ok(n), _) => (format!("will be saved as {n}"), GOOD),
                (Err(why), _) => (why.to_string(), BAD),
            },
            Slot::Value(i) => (row_value(b, ROWS[i]), if i == b.row { GOLD } else { TEXT }),
            Slot::Count(i) => {
                let (at, n) = row_position(b, ROWS[i]);
                (format!("{}/{}", at + 1, n), DIM)
            }
            Slot::Blurb => {
                let (_, pitch) = antediluvia_sim::abilities::class_blurb(b.class);
                let kit: Vec<&str> = crate::class_bar(b.class).iter().map(|a| a.name).collect();
                (format!("{pitch}\n{}", kit.join(", ")), DIM)
            }
            Slot::Status => match (&b.error, b.submitted) {
                (Some(e), _) => (format!("Cannot create: {e}"), BAD),
                (None, true) => ("Creating...".to_string(), GOOD),
                _ => (String::new(), BAD),
            },
        };
        if **text != s {
            **text = s;
        }
        if color.0 != c {
            color.0 = c;
        }
    }
    for (sw, mut bg, mut vis) in &mut swatches {
        let colour = match ROWS[sw.0] {
            Row::Field(f) => swatch(f, b.look.to_array()[f]),
            _ => None,
        };
        match colour {
            Some(c) => {
                bg.0 = Color::srgb(c[0], c[1], c[2]);
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }

    // ── Live preview ─────────────────────────────────────────────────────
    let outfit = b.appearance[0] % 4;
    let ground = crate::terrain::terrain_height(act, PREVIEW_AT.x, PREVIEW_AT.z);
    let at = Vec3::new(PREVIEW_AT.x, ground, PREVIEW_AT.z);
    // A slow turn from side to side: the face is always in view.
    let yaw = 0.15 + (time.elapsed_secs() * 0.55).sin() * 0.75;
    let mut have = false;
    for (e, p, mut t, mut rig) in &mut preview {
        if p.outfit != outfit {
            commands.entity(e).despawn_recursive();
            continue;
        }
        have = true;
        if rig.look != b.look {
            rig.look = b.look;
        }
        *t = Transform::from_translation(at)
            .with_rotation(Quat::from_rotation_y(yaw))
            .with_scale(Vec3::splat(crate::CHAR_SCALE) * body_scale(b.look));
    }
    if !have {
        let file = OUTFIT_FILES[outfit as usize];
        let clip = |i: usize| asset_server.load(GltfAssetLabel::Animation(i).from_asset(file));
        commands.spawn((
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(file))),
            Transform::from_translation(at).with_scale(Vec3::splat(crate::CHAR_SCALE) * body_scale(b.look)),
            BuilderPreview { outfit },
            LookRig { look: b.look },
            // Idle animation and hidden weapons, like a real player.
            crate::RigClips { idle: clip(36), run: clip(48), attack: clip(1), death: clip(23) },
            crate::equipment::Loadout::default(),
        ));
    }
    // Close camera, character right of centre so the panel never covers it.
    if let Ok(mut c) = cam.get_single_mut() {
        let target = at + Vec3::new(-34.0, 36.0, 0.0);
        *c = Transform::from_translation(at + Vec3::new(-34.0, 44.0, 150.0)).looking_at(target, Vec3::Y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_look_field_and_the_outfit_have_a_line() {
        for f in 0..LOOK_FIELDS.len() {
            assert!(ROWS.contains(&Row::Field(f)), "{} has no line in the builder", LOOK_FIELDS[f].name);
        }
        assert!(ROWS.contains(&Row::Outfit) && ROWS.contains(&Row::Class) && ROWS.contains(&Row::Lineage));
        let labels: std::collections::HashSet<_> = ROWS.iter().map(|r| row_label(*r)).collect();
        assert_eq!(labels.len(), ROWS.len());
    }

    #[test]
    fn stepping_walks_every_option_and_wraps() {
        for row in ROWS {
            let mut b = Builder::default();
            let (start, n) = row_position(&b, row);
            let mut seen = std::collections::HashSet::new();
            for _ in 0..n {
                assert!(seen.insert(row_value(&b, row)), "{row:?} repeats a value name");
                step(&mut b, row, 1);
            }
            assert_eq!(row_position(&b, row).0, start, "{row:?} does not wrap forward");
            step(&mut b, row, -1);
            assert_eq!(row_position(&b, row).0, (start + n - 1) % n, "{row:?} does not wrap back");
            assert_eq!(b.look, b.look.clamped());
        }
    }

    #[test]
    fn randomise_changes_the_look_each_press_and_stays_valid() {
        let mut b = Builder::default();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            randomise(&mut b, 12345);
            assert_eq!(b.look, b.look.clamped());
            assert!(b.appearance[0] < 4);
            seen.insert((b.appearance[0], b.look.pack()));
        }
        assert!(seen.len() >= 199);
        // Name, class and lineage are the player's own choices: untouched.
        assert_eq!(b.class, Class::Warrior);
        assert!(b.name.is_empty() && b.faction.is_none());
    }

    #[test]
    fn create_needs_a_valid_name_and_sends_a_clamped_look() {
        let mut b = Builder::default();
        assert!(create_message(&b, "apple-1").is_err());
        b.name = "x1".into();
        assert!(create_message(&b, "apple-1").is_err());
        b.name = "eNOCH".into();
        b.look = Look { hair: 200, ..b.look };
        match create_message(&b, "apple-1") {
            Ok(ClientMsg::Login { create: Some(c), apple_id, .. }) => {
                assert_eq!(c.name, "Enoch");
                assert_eq!(apple_id, "apple-1");
                assert_eq!(Look::unpack(c.look), Look::unpack(c.look).clamped());
                assert!(c.appearance[1] <= 15 && c.appearance[2] <= 11);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn scripted_commands_drive_the_builder() {
        let mut b = Builder::default();
        autocmd(&mut b, "name Tubal");
        autocmd(&mut b, "outfit 2");
        autocmd(&mut b, "set hair 12");
        autocmd(&mut b, "set dye 99");
        autocmd(&mut b, "row 5");
        autocmd(&mut b, "step 0 1");
        assert_eq!((b.name.as_str(), b.appearance[0], b.look.hair, b.row), ("Tubal", 2, 12, 5));
        assert!(b.look.dye < LOOK_FIELDS[11].count);
        assert_eq!(b.class, Class::ALL[1]);
        let before = b.look;
        autocmd(&mut b, "random 7");
        assert_ne!(b.look, before);
        assert!(!b.want_submit);
        autocmd(&mut b, "create");
        assert!(b.want_submit);
    }
}
