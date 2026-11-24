//! In-game menu (Escape): character summary, world-PvP opt-in/out, help, and
//! a developer section for dev-allowlisted accounts (C14).
//!
//! Client-side only — every button maps to an existing `ClientMsg` and the
//! server re-authorizes each one. The menu is rebuilt fresh on every open so
//! labels (PvP state, dev section) are never stale.

use antediluvia_protocol::{Act, ClientMsg, DevCmd};
use bevy::prelude::*;

use crate::net::NetTx;
use crate::Session;

/// True while the menu overlay is up (also used to suppress click-attacks).
#[derive(Resource, Default)]
pub struct MenuOpen(pub bool);

#[derive(Component)]
pub(crate) struct MenuRoot;

#[derive(Component)]
pub(crate) struct MenuAction(MenuCmd);

#[derive(Clone)]
pub(crate) enum MenuCmd {
    Close,
    TogglePvp,
    Dev(DevCmd),
    Travel(Act),
}

// ─── Design tokens (match ui.rs) ─────────────────────────────────────────────

const PANEL: Color = Color::srgba(0.08, 0.08, 0.12, 0.96);
const DIM: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const TEXT: Color = Color::srgb(0.92, 0.90, 0.85);
const TEXT_DIM: Color = Color::srgb(0.62, 0.60, 0.55);
const GOLD: Color = Color::srgb(0.98, 0.90, 0.55);
const BTN: Color = Color::srgb(0.16, 0.16, 0.20);
const BTN_HOVER: Color = Color::srgb(0.24, 0.24, 0.30);
const BTN_BORDER: Color = Color::srgb(0.45, 0.40, 0.28);
const DEV_ACCENT: Color = Color::srgb(0.95, 0.55, 0.30);

/// Escape toggles the menu. Chat, the character builder, and the dev console
/// own the keyboard while they're up, so the menu stays out of their way.
pub(crate) fn menu_toggle(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    session: Res<Session>,
    mut menu_open: ResMut<MenuOpen>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if session.builder.is_some() || session.chat_active || session.dev_input.is_some() {
        return;
    }
    menu_open.0 = !menu_open.0;
    for e in &roots {
        commands.entity(e).despawn_recursive();
    }
    if menu_open.0 {
        build_menu(&mut commands, &session);
    }
}

pub(crate) fn menu_buttons(
    mut commands: Commands,
    mut q: Query<(&Interaction, &MenuAction), (Changed<Interaction>, With<Button>)>,
    mut menu_open: ResMut<MenuOpen>,
    tx: Res<NetTx>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    for (interaction, action) in &mut q {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match &action.0 {
            MenuCmd::Close => close_menu(&mut commands, &mut menu_open, &roots),
            MenuCmd::TogglePvp => {
                tx.send(ClientMsg::TogglePvp);
                close_menu(&mut commands, &mut menu_open, &roots);
            }
            MenuCmd::Dev(cmd) => {
                tx.send(ClientMsg::Dev { cmd: cmd.clone() });
                // Stay open so testers can chain dev actions.
            }
            MenuCmd::Travel(act) => {
                tx.send(ClientMsg::Travel { act: *act });
                close_menu(&mut commands, &mut menu_open, &roots);
            }
        }
    }
}

fn close_menu(
    commands: &mut Commands,
    menu_open: &mut ResMut<MenuOpen>,
    roots: &Query<Entity, With<MenuRoot>>,
) {
    menu_open.0 = false;
    for e in roots {
        commands.entity(e).despawn_recursive();
    }
}

fn build_menu(commands: &mut Commands, session: &Session) {
    let (title, sub) = match &session.sheet {
        Some(s) => {
            let class = s.class.map(|c| c.as_str()).unwrap_or("classless");
            let lineage = s.faction.as_deref().unwrap_or("undecided");
            (
                format!("{} — level {} {}", s.name, s.level, class),
                format!(
                    "Lineage: {}   Gold: {}   World PvP: {}",
                    lineage,
                    s.gold,
                    if s.pvp { "ON" } else { "OFF" }
                ),
            )
        }
        None => ("Antediluvia".to_string(), "Not logged in yet.".to_string()),
    };

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(DIM),
            MenuRoot,
        ))
        .with_children(|dim| {
            dim.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Stretch,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(22.0)),
                    min_width: Val::Px(380.0),
                    max_width: Val::Px(520.0),
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(title),
                    TextFont { font_size: 22.0, ..default() },
                    TextColor(GOLD),
                ));
                panel.spawn((
                    Text::new(sub),
                    TextFont { font_size: 14.0, ..default() },
                    TextColor(TEXT_DIM),
                ));
                panel.spawn((
                    Text::new("ESC closes this menu."),
                    TextFont { font_size: 12.0, ..default() },
                    TextColor(TEXT_DIM),
                ));

                // ── PvP opt-in/out ──
                let pvp_on = session.sheet.as_ref().is_some_and(|s| s.pvp);
                menu_button(
                    &mut *panel,
                    MenuCmd::TogglePvp,
                    if pvp_on {
                        "World PvP: ON — click to opt out"
                    } else {
                        "World PvP: OFF — click to opt in"
                    },
                    GOLD,
                );
                panel.spawn((
                    Text::new("Enemy capital lands flag you for PvP while you stand in them, opt-in or not."),
                    TextFont { font_size: 12.0, ..default() },
                    TextColor(TEXT_DIM),
                ));

                // ── Developer section (dev-allowlisted accounts only) ──
                if session.is_dev {
                    panel.spawn((
                        Text::new("— DEVELOPER —"),
                        TextFont { font_size: 14.0, ..default() },
                        TextColor(DEV_ACCENT),
                    ));
                    let dev_rows: &[(&str, MenuCmd)] = &[
                        ("God mode (toggle)", MenuCmd::Dev(DevCmd::Godmode)),
                        ("Heal", MenuCmd::Dev(DevCmd::Heal)),
                        ("Level 60", MenuCmd::Dev(DevCmd::SetLevel { level: 60 })),
                        ("+1,000 XP", MenuCmd::Dev(DevCmd::GiveXp { amount: 1000 })),
                        (
                            "Spawn dire wolf",
                            MenuCmd::Dev(DevCmd::SpawnMob { tag: "dire_wolf".into() }),
                        ),
                        ("Slay target", MenuCmd::Dev(DevCmd::KillTarget)),
                        ("Noon", MenuCmd::Dev(DevCmd::TimeOfDay { t: 0.3 })),
                        ("Midnight", MenuCmd::Dev(DevCmd::TimeOfDay { t: 0.0 })),
                    ];
                    for (label, cmd) in dev_rows {
                        menu_button(&mut *panel, cmd.clone(), label, DEV_ACCENT);
                    }
                    panel.spawn((
                        Text::new("Travel:"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(TEXT),
                    ));
                    // Travel buttons sit in one wrapping row.
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            column_gap: Val::Px(6.0),
                            row_gap: Val::Px(6.0),
                            ..default()
                        })
                        .with_children(|row| {
                            for act in Act::ALL {
                                menu_button(&mut *row, MenuCmd::Travel(act), act.as_str(), TEXT_DIM);
                            }
                        });
                    panel.spawn((
                        Text::new("Full console: ` key.  xp N | questdone ID | give | tp | spawn | …"),
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(TEXT_DIM),
                    ));
                }

                // ── Help ──
                panel.spawn((
                    Text::new("— HELP —"),
                    TextFont { font_size: 14.0, ..default() },
                    TextColor(GOLD),
                ));
                panel.spawn((
                    Text::new(
                        "WASD move · click attack · 1/2 abilities · E talk ·\n\
                         /pvp toggle PvP · /duel NAME duel · /lineage sethite|cainite (10+)\n\
                         Party: /pinvite NAME (nearby members share XP) · Guild: /ginvite NAME",
                    ),
                    TextFont { font_size: 12.0, ..default() },
                    TextColor(TEXT),
                ));

                menu_button(&mut *panel, MenuCmd::Close, "Return to the world", GOLD);
            });
        });
}

/// One menu button: bordered node + label.
fn menu_button(parent: &mut ChildBuilder, cmd: MenuCmd, label: &str, accent: Color) {
    parent
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(BTN),
            BorderColor(BTN_BORDER),
            MenuAction(cmd),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(accent),
            ));
        });
}

/// Hover highlight for menu buttons (runs after `menu_buttons`).
pub(crate) fn menu_hover(
    mut q: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>, With<MenuAction>),
    >,
) {
    for (interaction, mut bg) in &mut q {
        *bg = BackgroundColor(match *interaction {
            Interaction::Hovered | Interaction::Pressed => BTN_HOVER,
            Interaction::None => BTN,
        });
    }
}
