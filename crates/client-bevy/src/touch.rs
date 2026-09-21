//! Mobile touch controls (client-side only).
//!
//! Everything here is inert on desktop: [`TouchMode`] flips on only after the
//! first real [`TouchInput`](bevy::input::touch::TouchInput) — or a
//! `pointer: coarse` media query at startup on web — and every touch action
//! dispatches through the exact same path as its keyboard/mouse equivalent
//! (`ClientMsg`s, `PlayerJump`, `Orbit`, `Cooldowns`, `dispatch_chat_line`,
//! `builder_try_submit`).
//!
//! Design notes:
//! - No per-frame allocations: touch slots are a fixed `[Option<_>; 8]`
//!   array and button rects are fixed arrays recomputed in place.
//! - Bevy 0.15's UI `Interaction` does not respond to touch, so buttons are
//!   rendered with Bevy UI but hit-tested manually against known rects.
//! - Text entry (chat, builder name) goes through a DOM `<input>` overlay in
//!   `web/index.html`: Bevy exposes no API to summon the OS keyboard. The
//!   page posts submitted text back into the wasm via an exported function;
//!   a system drains it each frame. The overlay field stays visibly tappable
//!   so iOS always has a genuine-gesture path to the keyboard.

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::math::Rect;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use antediluvia_protocol::{Class, ClientMsg};

#[cfg(target_arch = "wasm32")]
use super::dispatch_chat_line;
use super::menu::MenuOpen;
use super::net::NetTx;
use super::ui::{ActionBarRoot, Cooldowns};
use super::variety::{HAIR_CHOICES, SKIN_CHOICES};
use super::{builder_try_submit, class_abilities, PlayerJump, Session};

// ─── Resources ───────────────────────────────────────────────────────────────

/// True once we have seen a real touch (or the device reports a coarse primary
/// pointer on web). Gates every touch UI element; desktop never sets it.
#[derive(Resource, Default)]
pub struct TouchMode(pub bool);

/// Virtual-joystick vector: x = strafe (right +), y = forward (up +), -1..1.
#[derive(Resource, Default)]
pub struct TouchMove(pub Vec2);

/// Camera-look deltas accumulated this frame by right-half drags; consumed and
/// zeroed by `orbit_camera` in main.rs.
#[derive(Resource, Default)]
pub struct TouchLookDelta(pub Vec2);

/// Joystick deflection radius in logical px.
pub const JOY_RADIUS: f32 = 60.0;
/// Stick deadzone for the (f, s) quantization shared with the keyboard path.
const STICK_DEADZONE: f32 = 0.3;
/// Extra hit slop around buttons, in logical px.
const BTN_SLOP: f32 = 12.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SlotRole {
    Joystick,
    Look,
    GameBtn(GameBtn),
    BuilderBtn(BuilderBtn),
}

#[derive(Clone, Copy)]
struct TouchSlot {
    id: u64,
    role: SlotRole,
    start: Vec2,
    last: Vec2,
}

/// Fixed-size touch tracking: no per-frame allocation.
#[derive(Resource)]
pub struct TouchSlots {
    slots: [Option<TouchSlot>; 8],
}

impl Default for TouchSlots {
    fn default() -> Self {
        Self { slots: [None; 8] }
    }
}

// ─── Buttons ─────────────────────────────────────────────────────────────────

/// Game-screen touch buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameBtn {
    Attack,
    Jump,
    Interact,
    Ability1,
    Ability2,
    Mount,
    Chat,
}

/// Character-builder touch buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuilderBtn {
    Class(u8),
    Lineage,
    BodyMinus,
    BodyPlus,
    SkinMinus,
    SkinPlus,
    HairMinus,
    HairPlus,
    Name,
    Create,
}

struct GameBtnDef {
    id: GameBtn,
    label: &'static str,
    w: f32,
    h: f32,
    right: f32,
    bottom: f32,
}

/// Right-side cluster; all ≥44 px. Positions are `right`/`bottom` offsets in
/// logical px — the same constants drive both the Bevy layout and the manual
/// hit-testing, so they cannot drift apart.
const GAME_BTNS: [GameBtnDef; 7] = [
    GameBtnDef {
        id: GameBtn::Attack,
        label: "ATK",
        w: 76.0,
        h: 76.0,
        right: 16.0,
        bottom: 24.0,
    },
    GameBtnDef {
        id: GameBtn::Jump,
        label: "JMP",
        w: 60.0,
        h: 60.0,
        right: 108.0,
        bottom: 24.0,
    },
    GameBtnDef {
        id: GameBtn::Interact,
        label: "E",
        w: 60.0,
        h: 60.0,
        right: 16.0,
        bottom: 116.0,
    },
    GameBtnDef {
        id: GameBtn::Ability1,
        label: "1",
        w: 60.0,
        h: 60.0,
        right: 108.0,
        bottom: 100.0,
    },
    GameBtnDef {
        id: GameBtn::Ability2,
        label: "2",
        w: 60.0,
        h: 60.0,
        right: 180.0,
        bottom: 100.0,
    },
    GameBtnDef {
        id: GameBtn::Mount,
        label: "MNT",
        w: 60.0,
        h: 60.0,
        right: 16.0,
        bottom: 192.0,
    },
    GameBtnDef {
        id: GameBtn::Chat,
        label: "CHT",
        w: 60.0,
        h: 60.0,
        right: 108.0,
        bottom: 176.0,
    },
];

struct BuilderBtnDef {
    id: BuilderBtn,
    label: &'static str,
    w: f32,
    h: f32,
    left: f32,
    bottom: f32,
}

/// Bottom sheet over the builder preview; `left`/`bottom` anchored.
const BUILDER_BTNS: [BuilderBtnDef; 13] = [
    BuilderBtnDef {
        id: BuilderBtn::Class(0),
        label: "Warrior",
        w: 84.0,
        h: 48.0,
        left: 12.0,
        bottom: 190.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::Class(1),
        label: "Hunter",
        w: 84.0,
        h: 48.0,
        left: 104.0,
        bottom: 190.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::Class(2),
        label: "Priest",
        w: 84.0,
        h: 48.0,
        left: 196.0,
        bottom: 190.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::Class(3),
        label: "Mage",
        w: 84.0,
        h: 48.0,
        left: 288.0,
        bottom: 190.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::Lineage,
        label: "Lineage",
        w: 180.0,
        h: 48.0,
        left: 12.0,
        bottom: 134.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::BodyMinus,
        label: "< Body",
        w: 84.0,
        h: 48.0,
        left: 200.0,
        bottom: 134.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::BodyPlus,
        label: "Body >",
        w: 84.0,
        h: 48.0,
        left: 292.0,
        bottom: 134.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::SkinMinus,
        label: "< Skin",
        w: 84.0,
        h: 48.0,
        left: 12.0,
        bottom: 78.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::SkinPlus,
        label: "Skin >",
        w: 84.0,
        h: 48.0,
        left: 104.0,
        bottom: 78.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::HairMinus,
        label: "< Hair",
        w: 84.0,
        h: 48.0,
        left: 196.0,
        bottom: 78.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::HairPlus,
        label: "Hair >",
        w: 84.0,
        h: 48.0,
        left: 288.0,
        bottom: 78.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::Name,
        label: "Name…",
        w: 176.0,
        h: 52.0,
        left: 12.0,
        bottom: 18.0,
    },
    BuilderBtnDef {
        id: BuilderBtn::Create,
        label: "CREATE",
        w: 176.0,
        h: 52.0,
        left: 196.0,
        bottom: 18.0,
    },
];

/// Screen-space hit rects for the buttons, recomputed every frame from the
/// layout constants and the window size. Origin is top-left, logical px.
#[derive(Resource, Default)]
pub struct TouchButtons {
    game_rects: [Rect; 7],
    builder_rects: [Rect; 13],
}

// ─── UI markers ──────────────────────────────────────────────────────────────

#[derive(Component)]
pub(crate) struct TouchUiRoot;
#[derive(Component)]
pub(crate) struct TouchBuilderRoot;
#[derive(Component)]
pub(crate) struct TouchGameBtn(GameBtn);
#[derive(Component)]
pub(crate) struct TouchBuilderBtn(BuilderBtn);
#[derive(Component)]
pub(crate) struct JoystickBase;
#[derive(Component)]
pub(crate) struct JoystickKnob;

const BTN_BG: Color = Color::srgba(0.10, 0.12, 0.16, 0.72);
const BTN_BG_PRESSED: Color = Color::srgba(0.20, 0.45, 0.55, 0.85);
const BTN_BORDER: Color = Color::srgb(0.45, 0.40, 0.28);
const BTN_TEXT: Color = Color::srgb(0.92, 0.90, 0.85);

// ─── Pure helpers (unit-tested) ──────────────────────────────────────────────

/// Joystick deflection from a drag: clamped to [`JOY_RADIUS`], y flipped so
/// up on screen is +1 (forward).
pub fn stick_vector(start: Vec2, cur: Vec2) -> Vec2 {
    let d = cur - start;
    let len = d.length();
    if len < 1.0 {
        return Vec2::ZERO;
    }
    let clamped = if len > JOY_RADIUS {
        d * (JOY_RADIUS / len)
    } else {
        d
    };
    Vec2::new(clamped.x / JOY_RADIUS, -clamped.y / JOY_RADIUS)
}

/// Quantize a stick vector to the `(f, s)` i8 grid the keyboard path uses, so
/// touch movement flows through the identical `ClientMsg::Move` logic.
pub fn quantize_stick(v: Vec2) -> (i8, i8) {
    let f = if v.y > STICK_DEADZONE {
        1
    } else if v.y < -STICK_DEADZONE {
        -1
    } else {
        0
    };
    let s = if v.x > STICK_DEADZONE {
        1
    } else if v.x < -STICK_DEADZONE {
        -1
    } else {
        0
    };
    (f, s)
}

/// Index of the first rect containing `p` (with slop), else `None`.
fn hit_rect(rects: &[Rect], p: Vec2) -> Option<usize> {
    rects.iter().position(|r| {
        p.x >= r.min.x - BTN_SLOP
            && p.x <= r.max.x + BTN_SLOP
            && p.y >= r.min.y - BTN_SLOP
            && p.y <= r.max.y + BTN_SLOP
    })
}

// ─── Setup ───────────────────────────────────────────────────────────────────

/// Spawn the touch UI (hidden until [`TouchMode`] flips) and, on web,
/// pre-enable touch mode when the primary pointer is coarse.
pub fn touch_startup(mut commands: Commands, _touch_mode: ResMut<TouchMode>) {
    #[cfg(target_arch = "wasm32")]
    enable_touch_for_coarse_pointer(_touch_mode);

    // Game buttons + joystick visuals, full-screen transparent root.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            Visibility::Hidden,
            TouchUiRoot,
        ))
        .with_children(|root| {
            for def in &GAME_BTNS {
                root.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        right: Val::Px(def.right),
                        bottom: Val::Px(def.bottom),
                        width: Val::Px(def.w),
                        height: Val::Px(def.h),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(BTN_BG),
                    BorderColor(BTN_BORDER),
                    BorderRadius::all(Val::Px(14.0)),
                    TouchGameBtn(def.id),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(def.label),
                        TextFont {
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(BTN_TEXT),
                    ));
                });
            }
            // Joystick base + knob: repositioned to the touch origin while a
            // joystick slot is active, hidden otherwise.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(JOY_RADIUS * 2.0),
                    height: Val::Px(JOY_RADIUS * 2.0),
                    border: UiRect::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.1, 0.14, 0.35)),
                BorderColor(Color::srgba(0.9, 0.9, 0.95, 0.5)),
                BorderRadius::all(Val::Px(JOY_RADIUS)),
                Visibility::Hidden,
                JoystickBase,
            ));
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(48.0),
                    height: Val::Px(48.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.9, 0.9, 0.95, 0.55)),
                BorderRadius::all(Val::Px(24.0)),
                Visibility::Hidden,
                JoystickKnob,
            ));
        });

    // Character-builder touch sheet.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            Visibility::Hidden,
            TouchBuilderRoot,
        ))
        .with_children(|root| {
            for def in &BUILDER_BTNS {
                root.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(def.left),
                        bottom: Val::Px(def.bottom),
                        width: Val::Px(def.w),
                        height: Val::Px(def.h),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(BTN_BG),
                    BorderColor(BTN_BORDER),
                    BorderRadius::all(Val::Px(10.0)),
                    TouchBuilderBtn(def.id),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(def.label),
                        TextFont {
                            font_size: 15.0,
                            ..default()
                        },
                        TextColor(BTN_TEXT),
                    ));
                });
            }
        });
}

/// wasm-only: flip [`TouchMode`] on when the primary pointer is coarse.
#[cfg(target_arch = "wasm32")]
fn enable_touch_for_coarse_pointer(mut touch_mode: ResMut<TouchMode>) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(mql)) = window.match_media("(pointer: coarse)") {
            if mql.matches() {
                touch_mode.0 = true;
            }
        }
    }
}

// ─── Input ───────────────────────────────────────────────────────────────────

/// Route `TouchInput` events: left half → joystick, right half → camera drag,
/// button rects → the same actions as their keyboard/mouse equivalents.
/// The first `Started` event flips [`TouchMode`] on; before that this system
/// only drains the event queue.
#[allow(clippy::too_many_arguments)]
pub fn touch_input(
    mut touch_events: EventReader<TouchInput>,
    mut touch_mode: ResMut<TouchMode>,
    mut slots: ResMut<TouchSlots>,
    mut touch_move: ResMut<TouchMove>,
    mut look_delta: ResMut<TouchLookDelta>,
    buttons: Res<TouchButtons>,
    mut session: ResMut<Session>,
    tx: Res<NetTx>,
    time: Res<Time>,
    mut cooldowns: ResMut<Cooldowns>,
    mut jump: ResMut<PlayerJump>,
    menu_open: Res<MenuOpen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut game_visuals: Query<(&TouchGameBtn, &mut BackgroundColor)>,
    mut builder_visuals: Query<(&TouchBuilderBtn, &mut BackgroundColor)>,
) {
    let Ok(window) = windows.get_single() else {
        touch_events.clear();
        return;
    };
    let win_w = window.width();
    let in_builder = session.builder.is_some();
    // The builder, dev console, chat and menu own the input while they're up
    // (same rule as `mouse_click_attack`).
    let ui_blocked =
        in_builder || session.dev_input.is_some() || session.chat_active || menu_open.0;

    for ev in touch_events.read() {
        if ev.phase == TouchPhase::Started && !touch_mode.0 {
            touch_mode.0 = true;
        }
        if !touch_mode.0 {
            continue;
        }
        match ev.phase {
            TouchPhase::Started => {
                let role = if in_builder {
                    // Only the builder sheet is live on the builder screen.
                    hit_rect(&buttons.builder_rects, ev.position)
                        .map(|i| SlotRole::BuilderBtn(BUILDER_BTNS[i].id))
                } else if ui_blocked {
                    None
                } else {
                    if let Some(i) = hit_rect(&buttons.game_rects, ev.position) {
                        Some(SlotRole::GameBtn(GAME_BTNS[i].id))
                    } else if ev.position.x < win_w * 0.5 {
                        Some(SlotRole::Joystick)
                    } else {
                        Some(SlotRole::Look)
                    }
                };
                let Some(role) = role else { continue };
                let slot = TouchSlot {
                    id: ev.id,
                    role,
                    start: ev.position,
                    last: ev.position,
                };
                if let Some(free) = slots.slots.iter_mut().find(|s| s.is_none()) {
                    *free = Some(slot);
                }
                set_pressed_visual(role, true, &mut game_visuals, &mut builder_visuals);
            }
            TouchPhase::Moved => {
                let Some(slot) = slots.slots.iter_mut().flatten().find(|s| s.id == ev.id) else {
                    continue;
                };
                let prev = slot.last;
                slot.last = ev.position;
                match slot.role {
                    SlotRole::Look => {
                        look_delta.0 += ev.position - prev;
                    }
                    SlotRole::GameBtn(id) => {
                        let inside = hit_rect(&buttons.game_rects, ev.position)
                            .map(|i| GAME_BTNS[i].id == id)
                            .unwrap_or(false);
                        set_pressed_visual(
                            slot.role,
                            inside,
                            &mut game_visuals,
                            &mut builder_visuals,
                        );
                    }
                    SlotRole::BuilderBtn(id) => {
                        let inside = hit_rect(&buttons.builder_rects, ev.position)
                            .map(|i| BUILDER_BTNS[i].id == id)
                            .unwrap_or(false);
                        set_pressed_visual(
                            slot.role,
                            inside,
                            &mut game_visuals,
                            &mut builder_visuals,
                        );
                    }
                    SlotRole::Joystick => {}
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                let pos = slots
                    .slots
                    .iter()
                    .position(|s| matches!(s, Some(s) if s.id == ev.id));
                let Some(pos) = pos else { continue };
                let slot = slots.slots[pos].take().unwrap();
                set_pressed_visual(slot.role, false, &mut game_visuals, &mut builder_visuals);
                if ev.phase == TouchPhase::Canceled {
                    continue;
                }
                match slot.role {
                    SlotRole::GameBtn(id) => {
                        // Fire on release-inside, like a click.
                        let inside = hit_rect(&buttons.game_rects, ev.position)
                            .map(|i| GAME_BTNS[i].id == id)
                            .unwrap_or(false);
                        if inside {
                            // Chat may only toggle while it owns input; every
                            // other button is dead while chat is open.
                            if !session.chat_active || id == GameBtn::Chat {
                                dispatch_game_btn(
                                    id,
                                    &mut session,
                                    &tx,
                                    &time,
                                    &mut cooldowns,
                                    &mut jump,
                                    &mut slots,
                                    &mut touch_move,
                                );
                            }
                        }
                    }
                    SlotRole::BuilderBtn(id) => {
                        let inside = hit_rect(&buttons.builder_rects, ev.position)
                            .map(|i| BUILDER_BTNS[i].id == id)
                            .unwrap_or(false);
                        if inside {
                            dispatch_builder_btn(id, &mut session, &tx);
                        }
                    }
                    SlotRole::Joystick | SlotRole::Look => {}
                }
            }
        }
    }

    // The earliest live joystick drives movement; lifting it stops, even if a
    // second finger is still down elsewhere.
    touch_move.0 = slots
        .slots
        .iter()
        .flatten()
        .find(|s| s.role == SlotRole::Joystick)
        .map(|s| stick_vector(s.start, s.last))
        .unwrap_or(Vec2::ZERO);
}

/// Mirror the pressed state onto the button's background.
fn set_pressed_visual(
    role: SlotRole,
    pressed: bool,
    game_visuals: &mut Query<(&TouchGameBtn, &mut BackgroundColor)>,
    builder_visuals: &mut Query<(&TouchBuilderBtn, &mut BackgroundColor)>,
) {
    let color = if pressed { BTN_BG_PRESSED } else { BTN_BG };
    match role {
        SlotRole::GameBtn(id) => {
            for (btn, mut bg) in game_visuals.iter_mut() {
                if btn.0 == id {
                    bg.0 = color;
                }
            }
        }
        SlotRole::BuilderBtn(id) => {
            for (btn, mut bg) in builder_visuals.iter_mut() {
                if btn.0 == id {
                    bg.0 = color;
                }
            }
        }
        SlotRole::Joystick | SlotRole::Look => {}
    }
}

/// Clear joystick state (e.g. when chat opens mid-drag).
fn clear_joystick(slots: &mut TouchSlots, touch_move: &mut TouchMove) {
    for s in slots.slots.iter_mut() {
        if matches!(s, Some(s) if s.role == SlotRole::Joystick) {
            *s = None;
        }
    }
    touch_move.0 = Vec2::ZERO;
}

/// Dispatch a game-screen touch button through the same path as its
/// keyboard/mouse equivalent.
#[allow(clippy::too_many_arguments)]
fn dispatch_game_btn(
    id: GameBtn,
    session: &mut Session,
    tx: &NetTx,
    time: &Time,
    cooldowns: &mut Cooldowns,
    jump: &mut PlayerJump,
    slots: &mut TouchSlots,
    touch_move: &mut TouchMove,
) {
    let now = time.elapsed_secs();
    match id {
        // Identical to `mouse_click_attack`: Attack + slot-0 cooldown.
        GameBtn::Attack => {
            tx.send(ClientMsg::Attack);
            cooldowns.trigger(0, now);
        }
        // Identical to Space in `send_input`.
        GameBtn::Jump => {
            jump.start.get_or_insert(now);
        }
        // Identical to KeyE in `send_input`.
        GameBtn::Interact => {
            tx.send(ClientMsg::Talk);
        }
        // Identical to Digit1/Digit2 in `send_input`.
        GameBtn::Ability1 | GameBtn::Ability2 => {
            if let Some(class) = session.class {
                let [a, b] = class_abilities(class);
                let slot = if id == GameBtn::Ability1 { 1 } else { 2 };
                tx.send(ClientMsg::Cast {
                    ability: if slot == 1 { a } else { b }.into(),
                });
                cooldowns.trigger(slot, now);
            }
        }
        // Identical to KeyM in `send_input`.
        GameBtn::Mount => {
            tx.send(ClientMsg::Mount);
        }
        GameBtn::Chat => {
            if session.chat_active {
                session.chat_input.clear();
                session.chat_active = false;
                #[cfg(target_arch = "wasm32")]
                web_bridge::overlay_hide();
            } else {
                session.chat_active = true;
                // Chat owns the input now: stop any in-progress movement.
                clear_joystick(slots, touch_move);
                #[cfg(target_arch = "wasm32")]
                web_bridge::overlay_show(web_bridge::MODE_CHAT, "Say something…");
            }
        }
    }
}

/// Dispatch a builder-screen touch button through the same mutations as the
/// keyboard builder (`builder_screen`).
fn dispatch_builder_btn(id: BuilderBtn, session: &mut Session, tx: &NetTx) {
    let Some(b) = session.builder.as_mut() else {
        return;
    };
    match id {
        BuilderBtn::Class(i) => {
            b.class = match i {
                0 => Class::Warrior,
                1 => Class::Hunter,
                2 => Class::Priest,
                _ => Class::Mage,
            };
        }
        // Identical to the F5 cycle in `builder_screen`.
        BuilderBtn::Lineage => {
            b.faction = match b.faction.as_deref() {
                None => Some("sethite".into()),
                Some("sethite") => Some("cainite".into()),
                _ => None,
            };
        }
        BuilderBtn::BodyMinus => b.appearance[0] = (b.appearance[0] + 3) % 4,
        BuilderBtn::BodyPlus => b.appearance[0] = (b.appearance[0] + 1) % 4,
        BuilderBtn::SkinMinus => {
            b.appearance[1] = (b.appearance[1] + SKIN_CHOICES - 1) % SKIN_CHOICES;
        }
        BuilderBtn::SkinPlus => b.appearance[1] = (b.appearance[1] + 1) % SKIN_CHOICES,
        BuilderBtn::HairMinus => {
            b.appearance[2] = (b.appearance[2] + HAIR_CHOICES - 1) % HAIR_CHOICES;
        }
        BuilderBtn::HairPlus => b.appearance[2] = (b.appearance[2] + 1) % HAIR_CHOICES,
        // Name entry summons the OS keyboard via the DOM overlay (wasm); the
        // submitted text is filtered exactly like keyboard entry.
        BuilderBtn::Name => {
            #[cfg(target_arch = "wasm32")]
            web_bridge::overlay_show(web_bridge::MODE_NAME, "Character name");
        }
        // Identical to Enter in `builder_screen`.
        BuilderBtn::Create => builder_try_submit(session, tx),
    }
}

// ─── Layout / visibility ─────────────────────────────────────────────────────

/// Per-frame: recompute button hit rects, toggle touch UI visibility, move
/// the joystick visuals, and hide the desktop action bar while in touch mode.
#[allow(clippy::too_many_arguments)]
pub fn touch_layout(
    touch_mode: Res<TouchMode>,
    session: Res<Session>,
    menu_open: Res<MenuOpen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut buttons: ResMut<TouchButtons>,
    slots: Res<TouchSlots>,
    mut ui_root: Query<&mut Visibility, With<TouchUiRoot>>,
    mut builder_root: Query<&mut Visibility, With<TouchBuilderRoot>>,
    mut action_bar: Query<&mut Visibility, With<ActionBarRoot>>,
    mut joy_base: Query<(&mut Node, &mut Visibility), (With<JoystickBase>, Without<JoystickKnob>)>,
    mut joy_knob: Query<(&mut Node, &mut Visibility), With<JoystickKnob>>,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };
    let win = Vec2::new(window.width(), window.height());

    for (i, def) in GAME_BTNS.iter().enumerate() {
        let x = win.x - def.right - def.w;
        let y = win.y - def.bottom - def.h;
        buttons.game_rects[i] = Rect::new(x, y, x + def.w, y + def.h);
    }
    for (i, def) in BUILDER_BTNS.iter().enumerate() {
        let y = win.y - def.bottom - def.h;
        buttons.builder_rects[i] = Rect::new(def.left, y, def.left + def.w, y + def.h);
    }

    let show = touch_mode.0;
    // Game buttons (and the joystick visuals, children of the same root)
    // hide whenever something else owns the screen or the input — same rule
    // as the hit-testing in `touch_input`.
    let in_builder = session.builder.is_some();
    let ui_blocked =
        in_builder || session.dev_input.is_some() || session.chat_active || menu_open.0;
    for mut v in ui_root.iter_mut() {
        *v = if show && !ui_blocked {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for mut v in builder_root.iter_mut() {
        *v = if show && in_builder {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    // The desktop action bar is replaced by the touch buttons; hiding it also
    // keeps it from overlapping the button cluster on narrow screens.
    for mut v in action_bar.iter_mut() {
        *v = if show {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }

    // Joystick visuals follow the earliest live joystick slot.
    let joy = slots
        .slots
        .iter()
        .flatten()
        .find(|s| s.role == SlotRole::Joystick);
    for (mut node, mut vis) in joy_base.iter_mut() {
        if let Some(s) = joy {
            node.left = Val::Px(s.start.x - JOY_RADIUS);
            node.top = Val::Px(s.start.y - JOY_RADIUS);
            *vis = Visibility::Inherited;
        } else {
            *vis = Visibility::Hidden;
        }
    }
    for (mut node, mut vis) in joy_knob.iter_mut() {
        if let Some(s) = joy {
            let v = stick_vector(s.start, s.last);
            // Knob position mirrors the clamped deflection.
            node.left = Val::Px(s.start.x + v.x * JOY_RADIUS - 24.0);
            node.top = Val::Px(s.start.y - v.y * JOY_RADIUS - 24.0);
            *vis = Visibility::Inherited;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

// ─── DOM text-entry bridge (wasm only) ───────────────────────────────────────

/// Drain text submitted through the DOM overlay (chat / builder name) and
/// cancelled overlays. No-op on native.
pub fn overlay_poll(_session: ResMut<Session>, _tx: Res<NetTx>) {
    #[cfg(target_arch = "wasm32")]
    {
        let mut session = _session;
        let tx = _tx;
        for (mode, text) in web_bridge::drain_inbox() {
            match mode {
                web_bridge::MODE_CHAT => {
                    let t = text.trim();
                    if !t.is_empty() {
                        dispatch_chat_line(t, &tx);
                    }
                    session.chat_input.clear();
                    session.chat_active = false;
                    web_bridge::overlay_hide();
                }
                web_bridge::MODE_NAME => {
                    // Same filter as keyboard name entry in `builder_screen`.
                    if let Some(b) = session.builder.as_mut() {
                        b.name = text
                            .chars()
                            .filter(|c| c.is_alphanumeric())
                            .take(24)
                            .collect();
                    }
                    web_bridge::overlay_hide();
                }
                _ => {}
            }
        }
        match web_bridge::take_cancel() {
            web_bridge::MODE_CHAT => {
                session.chat_input.clear();
                session.chat_active = false;
                web_bridge::overlay_hide();
            }
            web_bridge::MODE_NAME => {
                web_bridge::overlay_hide();
            }
            _ => {}
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web_bridge {
    //! JS↔wasm bridge for the DOM text-entry overlay in `web/index.html`.
    //!
    //! Game → page is direct DOM manipulation via web-sys (show/hide/focus).
    //! Page → game is [`antediluvia_submit_overlay`] /
    //! [`antediluvia_cancel_overlay`], called by the page's event listeners
    //! and drained by [`super::overlay_poll`].

    use std::sync::Mutex;

    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    pub const MODE_CHAT: u8 = 1;
    pub const MODE_NAME: u8 = 2;

    static INBOX: Mutex<Vec<(u8, String)>> = Mutex::new(Vec::new());
    static MODE: Mutex<u8> = Mutex::new(0);
    static CANCELLED: Mutex<bool> = Mutex::new(false);

    fn bar() -> Option<web_sys::Element> {
        web_sys::window()?
            .document()?
            .get_element_by_id("antediluvia-chatbar")
    }

    fn field() -> Option<web_sys::HtmlInputElement> {
        web_sys::window()?
            .document()?
            .get_element_by_id("antediluvia-chat-input")?
            .dyn_into::<web_sys::HtmlInputElement>()
            .ok()
    }

    /// Show the overlay in `mode` and focus the field. Called on the game
    /// thread (wasm is single-threaded). `focus()` summons the OS keyboard on
    /// Android; on iOS it only works from a real user gesture, so the field
    /// stays visibly tappable as the guaranteed fallback.
    pub fn overlay_show(mode: u8, placeholder: &str) {
        *MODE.lock().unwrap() = mode;
        if let Some(b) = bar() {
            b.set_attribute("style", "display: flex;").ok();
        }
        if let Some(f) = field() {
            f.set_placeholder(placeholder);
            f.set_value("");
            let _ = f.focus();
        }
    }

    pub fn overlay_hide() {
        *MODE.lock().unwrap() = 0;
        if let Some(b) = bar() {
            b.set_attribute("style", "display: none;").ok();
        }
        if let Some(f) = field() {
            let _ = f.blur();
        }
    }

    /// Called by the host page when the player submits the overlay field.
    #[wasm_bindgen]
    pub fn antediluvia_submit_overlay(text: String) {
        let mode = *MODE.lock().unwrap();
        if mode != 0 {
            INBOX.lock().unwrap().push((mode, text));
        }
    }

    /// Called by the host page when the player cancels the overlay.
    #[wasm_bindgen]
    pub fn antediluvia_cancel_overlay() {
        *CANCELLED.lock().unwrap() = true;
    }

    /// Drain submitted texts. `Vec::new()` does not allocate, so polling an
    /// empty inbox every frame is free.
    pub fn drain_inbox() -> Vec<(u8, String)> {
        std::mem::take(&mut *INBOX.lock().unwrap())
    }

    /// Take a pending cancel; returns the overlay mode that was cancelled.
    pub fn take_cancel() -> u8 {
        if std::mem::replace(&mut *CANCELLED.lock().unwrap(), false) {
            std::mem::replace(&mut *MODE.lock().unwrap(), 0)
        } else {
            0
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stick_is_zero_at_rest() {
        let v = stick_vector(Vec2::new(100.0, 200.0), Vec2::new(100.0, 200.0));
        assert_eq!(v, Vec2::ZERO);
        assert_eq!(quantize_stick(v), (0, 0));
        // The desktop no-op path: default TouchMove must quantize to (0, 0).
        assert_eq!(quantize_stick(TouchMove::default().0), (0, 0));
    }

    #[test]
    fn stick_up_is_forward() {
        // Drag up on screen (y decreases) → +y forward.
        let v = stick_vector(Vec2::new(100.0, 200.0), Vec2::new(100.0, 140.0));
        assert!(
            (v.y - 1.0).abs() < 1e-5,
            "up drag should be full forward, got {v}"
        );
        assert!(v.x.abs() < 1e-5);
        assert_eq!(quantize_stick(v), (1, 0));
    }

    #[test]
    fn stick_clamps_to_radius() {
        let v = stick_vector(Vec2::new(0.0, 0.0), Vec2::new(600.0, 0.0));
        assert!(
            (v.length() - 1.0).abs() < 1e-5,
            "must clamp to unit, got {v}"
        );
        assert_eq!(quantize_stick(v), (0, 1));
    }

    #[test]
    fn stick_deadzone() {
        // 10% deflection is inside the deadzone.
        let v = stick_vector(Vec2::new(0.0, 0.0), Vec2::new(6.0, 6.0));
        assert_eq!(quantize_stick(v), (0, 0));
        // Diagonal past the deadzone moves on both axes.
        let v = stick_vector(Vec2::new(0.0, 0.0), Vec2::new(30.0, -30.0));
        assert_eq!(quantize_stick(v), (1, 1));
    }

    #[test]
    fn hit_rect_finds_buttons() {
        let rects = [
            Rect::new(10.0, 10.0, 70.0, 70.0),
            Rect::new(100.0, 100.0, 160.0, 160.0),
        ];
        assert_eq!(hit_rect(&rects, Vec2::new(40.0, 40.0)), Some(0));
        assert_eq!(hit_rect(&rects, Vec2::new(130.0, 130.0)), Some(1));
        assert_eq!(hit_rect(&rects, Vec2::new(500.0, 500.0)), None);
        // Slop extends the hit area: 5px outside still hits with 12px slop.
        assert_eq!(hit_rect(&rects, Vec2::new(75.0, 40.0)), Some(0));
        // …but not 20px outside.
        assert_eq!(hit_rect(&rects, Vec2::new(95.0, 40.0)), None);
    }

    #[test]
    fn game_button_layout_has_no_overlaps() {
        // Recompute rects for a 390x844 phone and check pairwise overlap.
        let win = Vec2::new(390.0, 844.0);
        let rects: Vec<Rect> = GAME_BTNS
            .iter()
            .map(|d| {
                let x = win.x - d.right - d.w;
                let y = win.y - d.bottom - d.h;
                Rect::new(x, y, x + d.w, y + d.h)
            })
            .collect();
        for (i, a) in rects.iter().enumerate() {
            // All buttons on screen.
            assert!(a.min.x >= 0.0 && a.max.x <= win.x && a.min.y >= 0.0 && a.max.y <= win.y);
            for (j, b) in rects.iter().enumerate() {
                if i != j {
                    assert!(
                        a.max.x <= b.min.x
                            || b.max.x <= a.min.x
                            || a.max.y <= b.min.y
                            || b.max.y <= a.min.y,
                        "game buttons {i} and {j} overlap"
                    );
                }
            }
        }
    }

    #[test]
    fn builder_button_layout_fits_phone() {
        let win = Vec2::new(390.0, 844.0);
        for d in &BUILDER_BTNS {
            let y = win.y - d.bottom - d.h;
            assert!(
                d.left >= 0.0 && d.left + d.w <= win.x,
                "{} overflows horizontally",
                d.label
            );
            assert!(
                y >= 0.0 && y + d.h <= win.y,
                "{} overflows vertically",
                d.label
            );
            assert!(d.w >= 44.0 && d.h >= 44.0, "{} smaller than 44px", d.label);
        }
    }
}
