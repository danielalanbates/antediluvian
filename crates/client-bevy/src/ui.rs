//! WoW-Classic-style HUD overlay — player unit frame, action bar, quest
//! tracker and chat log.
//!
//! `spawn_ui` creates the full hierarchy once from `setup`.
//! `update_ui` runs every frame, reading `Session` + `Time` to drive bar
//! widths, text labels, quest list, chat lines and cooldown overlays.

use bevy::prelude::*;


use super::{class_abilities, Session};


// ─── Design tokens ───────────────────────────────────────────────────────────

const PANEL: Color = Color::srgba(0.08, 0.08, 0.12, 0.85);

const BAR_BG: Color = Color::srgb(0.12, 0.12, 0.15);
const HP_COLOR: Color = Color::srgb(0.15, 0.78, 0.22);
const MP_COLOR: Color = Color::srgb(0.20, 0.45, 0.90);
const XP_COLOR: Color = Color::srgb(0.58, 0.30, 0.82);
const TEXT_COLOR: Color = Color::srgb(0.92, 0.90, 0.85);
const TEXT_DIM: Color = Color::srgb(0.62, 0.60, 0.55);
const GOLD_ACCENT: Color = Color::srgb(0.98, 0.90, 0.55);
const COOLDOWN_OVERLAY: Color = Color::srgba(0.10, 0.10, 0.10, 0.65);
const SLOT_BG: Color = Color::srgb(0.16, 0.16, 0.20);
const SLOT_BORDER: Color = Color::srgb(0.45, 0.40, 0.28);

const FONT_STAT: f32 = 15.0;
const FONT_LABEL: f32 = 13.0;
const FONT_CHAT: f32 = 12.0;
const FONT_KEY: f32 = 11.0;
const FONT_SLOT: f32 = 14.0;

// ─── Marker components ──────────────────────────────────────────────────────

// Player unit frame.
#[derive(Component)]
pub struct PlayerNameText;
#[derive(Component)]
pub struct PlayerLevelText;
#[derive(Component)]
pub struct TargetFrameRoot;
#[derive(Component)]
pub struct TargetNameText;
#[derive(Component)]
pub struct TargetHpText;
#[derive(Component)]
pub struct BannerText;
#[derive(Component)]
pub struct HpFill;
#[derive(Component)]
pub struct HpText;
#[derive(Component)]
pub struct MpFill;
#[derive(Component)]
pub struct MpText;
#[derive(Component)]
pub struct XpFill;

// Action bar.
#[derive(Component)]
pub struct ActionBarSlot;
#[derive(Component)]
pub struct ActionBarLabel(pub u8);
#[derive(Component)]
pub struct ActionBarCooldown(pub u8);
#[derive(Component)]
pub struct ClassSelectText;

// Quest tracker.
#[derive(Component)]
pub struct QuestTrackerRoot;
#[derive(Component)]
pub struct QuestTrackerText;

// Chat.
#[derive(Component)]
pub struct ChatLogText;
#[derive(Component)]
pub struct ChatInputText;
#[derive(Component)]
pub struct ChatInputBar;

// ─── Cooldowns ───────────────────────────────────────────────────────────────

/// Client-side cooldown approximation — the server is authoritative, but we
/// show a gray overlay so the player has visual feedback between frames.
#[derive(Resource)]
pub struct Cooldowns {
    pub gcd_until: f32,
    pub slot_until: [f32; 3],
}

impl Default for Cooldowns {
    fn default() -> Self {
        Self { gcd_until: 0.0, slot_until: [0.0; 3] }
    }
}

impl Cooldowns {
    /// Trigger the global cooldown (1.0 s) and the per-slot cooldown.
    pub fn trigger(&mut self, slot: u8, now: f32) {
        self.gcd_until = now + 1.0;
        if (slot as usize) < self.slot_until.len() {
            self.slot_until[slot as usize] = now + 1.0;
        }
    }

    /// Is a specific slot on cooldown right now?
    pub fn on_cd(&self, slot: u8, now: f32) -> bool {
        now < self.gcd_until || now < self.slot_until.get(slot as usize).copied().unwrap_or(0.0)
    }
}

// ─── Spawn ───────────────────────────────────────────────────────────────────

/// Build the entire HUD hierarchy.  Called once from `setup`.
pub fn spawn_ui(commands: &mut Commands) {
    spawn_player_frame(commands);
    spawn_target_frame(commands);
    // Discovery banner: big gold text, top-center, faded in/out by system.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(18.0),
            left: Val::Px(0.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(""),
                TextFont { font_size: 34.0, ..default() },
                TextColor(GOLD_ACCENT),
                BannerText,
            ));
        });
    spawn_action_bar(commands);
    spawn_quest_tracker(commands);
    spawn_chat(commands);
}

// ── Target frame (below the player frame): nearest hostile in range ─────────

fn spawn_target_frame(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(132.0),
                left: Val::Px(14.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(8.0)),
                row_gap: Val::Px(3.0),
                min_width: Val::Px(220.0),
                ..default()
            },
            Visibility::Hidden,
            TargetFrameRoot,
        ))
        .insert(BackgroundColor(PANEL))
        .with_children(|frame| {
            frame.spawn((
                Text::new(""),
                TextFont { font_size: FONT_STAT, ..default() },
                TextColor(GOLD_ACCENT),
                TargetNameText,
            ));
            frame.spawn((
                Text::new(""),
                TextFont { font_size: FONT_LABEL, ..default() },
                TextColor(TEXT_COLOR),
                TargetHpText,
            ));
        });
}

// ── Player unit frame (top-left) ─────────────────────────────────────────────

fn spawn_player_frame(commands: &mut Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(14.0),
            left: Val::Px(14.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(8.0)),
            row_gap: Val::Px(3.0),
            min_width: Val::Px(220.0),
            ..default()
        })
        .insert(BackgroundColor(PANEL))
        .with_children(|frame| {
            // ─ Name + Level row ─
            frame
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    margin: UiRect::bottom(Val::Px(2.0)),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("Player"),
                        TextFont { font_size: FONT_STAT, ..default() },
                        TextColor(TEXT_COLOR),
                        PlayerNameText,
                    ));
                    row.spawn((
                        Text::new("Lv 1"),
                        TextFont { font_size: FONT_LABEL, ..default() },
                        TextColor(GOLD_ACCENT),
                        PlayerLevelText,
                    ));
                });

            // ─ HP bar ─
            spawn_stat_bar(frame, HP_COLOR, "HP", HpFill, HpText);

            // ─ MP bar ─
            spawn_stat_bar(frame, MP_COLOR, "MP", MpFill, MpText);

            // ─ XP bar (thin, no text) ─
            frame
                .spawn(Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(5.0),
                    margin: UiRect::top(Val::Px(2.0)),
                    ..default()
                })
                .insert(BackgroundColor(BAR_BG))
                .with_children(|bar| {
                    bar.spawn((
                        Node {
                            width: Val::Percent(0.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(XP_COLOR),
                        XpFill,
                    ));
                });
        });
}

/// Reusable HP/MP bar: dark container → coloured fill + overlaid text.
fn spawn_stat_bar(
    parent: &mut ChildBuilder,
    color: Color,
    label: &str,
    fill_marker: impl Component,
    text_marker: impl Component,
) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Px(18.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        })
        .insert(BackgroundColor(BAR_BG))
        .with_children(|bar_bg| {
            // Coloured fill.
            bar_bg.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(color),
                fill_marker,
            ));
            // Centred text (in-flow so it can never escape the bar).
            bar_bg.spawn((
                Text::new(format!("{label} — / —")),
                TextFont { font_size: FONT_LABEL, ..default() },
                TextColor(TEXT_COLOR),
                TextLayout::new_with_no_wrap(),
                text_marker,
            ));
        });
}

// ── Action bar (bottom-center) ───────────────────────────────────────────────

fn spawn_action_bar(commands: &mut Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(16.0),
            left: Val::Percent(50.0),
            // Shift left by half its own width so it's truly centred.
            // Bevy 0.15 doesn't have `translate`, so we use a wrapper:
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|anchor| {
            // Inner row that we offset via negative margin.
            anchor
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.0),
                    padding: UiRect::all(Val::Px(6.0)),
                    // Pull back by ~half the expected width so the bar centres.
                    margin: UiRect::left(Val::Px(-120.0)),
                    ..default()
                })
                .insert(BackgroundColor(PANEL))
                .with_children(|bar| {
                    for slot in 0u8..3 {
                        let key_label = match slot {
                            0 => "Space",
                            1 => "1",
                            _ => "2",
                        };
                        spawn_action_slot(bar, slot, key_label);
                    }
                });

            // Class-selection prompt (shown when no class chosen).
            anchor.spawn((
                Text::new("F1 warrior · F2 hunter · F3 priest · F4 mage"),
                TextFont { font_size: FONT_LABEL, ..default() },
                TextColor(GOLD_ACCENT),
                Node {
                    margin: UiRect {
                        top: Val::Px(6.0),
                        left: Val::Px(-120.0),
                        ..default()
                    },
                    ..default()
                },
                ClassSelectText,
            ));
        });
}

fn spawn_action_slot(parent: &mut ChildBuilder, slot: u8, key: &str) {
    parent
        .spawn((
            Node {
                width: Val::Px(56.0),
                height: Val::Px(56.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(SLOT_BG),
            BorderColor(SLOT_BORDER),
            ActionBarSlot,
        ))
        .with_children(|s| {
            // Key hint (top).
            s.spawn((
                Text::new(key),
                TextFont { font_size: FONT_KEY, ..default() },
                TextColor(TEXT_DIM),
            ));
            // Ability name (centre).
            let default_name = match slot {
                0 => "Attack",
                1 => "—",
                _ => "—",
            };
            s.spawn((
                Text::new(default_name),
                TextFont { font_size: FONT_SLOT, ..default() },
                TextColor(TEXT_COLOR),
                ActionBarLabel(slot),
            ));
            // Cooldown overlay (hidden by default).
            s.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(COOLDOWN_OVERLAY),
                Visibility::Hidden,
                ActionBarCooldown(slot),
            ));
        });
}

// ── Quest tracker (right side) ───────────────────────────────────────────────

fn spawn_quest_tracker(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(14.0),
                right: Val::Px(14.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(8.0)),
                row_gap: Val::Px(2.0),
                min_width: Val::Px(180.0),
                max_width: Val::Px(240.0),
                ..default()
            },
            BackgroundColor(PANEL),
            Visibility::Hidden,
            QuestTrackerRoot,
        ))
        .with_children(|panel| {
            // Title.
            panel.spawn((
                Text::new("Quests"),
                TextFont { font_size: FONT_STAT, ..default() },
                TextColor(GOLD_ACCENT),
            ));
            // Dynamic quest lines (filled by update_ui).
            panel.spawn((
                Text::new(""),
                TextFont { font_size: FONT_LABEL, ..default() },
                TextColor(TEXT_COLOR),
                QuestTrackerText,
            ));
        });
}

// ── Chat (bottom-left) ───────────────────────────────────────────────────────

fn spawn_chat(commands: &mut Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(16.0),
            left: Val::Px(14.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            width: Val::Px(340.0),
            ..default()
        })
        .with_children(|chat| {
            // Log area.
            chat.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(6.0)),
                    max_height: Val::Px(160.0),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(|log_panel| {
                log_panel.spawn((
                    Text::new(""),
                    TextFont { font_size: FONT_CHAT, ..default() },
                    TextColor(TEXT_COLOR),
                    ChatLogText,
                ));
            });

            // Input bar.
            chat.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor(SLOT_BORDER),
                Visibility::Hidden,
                ChatInputBar,
            ))
            .with_children(|input_bg| {
                input_bg.spawn((
                    Text::new(""),
                    TextFont { font_size: FONT_CHAT, ..default() },
                    TextColor(TEXT_COLOR),
                    ChatInputText,
                ));
            });
        });
}

/// Drive player frame + action bar elements from `Session` each frame.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_ui_frames(
    session: Res<Session>,
    time: Res<Time>,
    cooldowns: Res<Cooldowns>,
    mut q_name: Query<&mut Text, (With<PlayerNameText>, Without<PlayerLevelText>, Without<HpText>, Without<MpText>, Without<ActionBarLabel>, Without<ClassSelectText>)>,
    mut q_level: Query<&mut Text, (With<PlayerLevelText>, Without<PlayerNameText>, Without<HpText>, Without<MpText>, Without<ActionBarLabel>, Without<ClassSelectText>)>,
    mut q_hp_fill: Query<&mut Node, (With<HpFill>, Without<MpFill>, Without<XpFill>, Without<ActionBarCooldown>)>,
    mut q_hp_text: Query<&mut Text, (With<HpText>, Without<PlayerNameText>, Without<PlayerLevelText>, Without<MpText>, Without<ActionBarLabel>, Without<ClassSelectText>)>,
    mut q_mp_fill: Query<&mut Node, (With<MpFill>, Without<HpFill>, Without<XpFill>, Without<ActionBarCooldown>)>,
    mut q_mp_text: Query<&mut Text, (With<MpText>, Without<PlayerNameText>, Without<PlayerLevelText>, Without<HpText>, Without<ActionBarLabel>, Without<ClassSelectText>)>,
    mut q_xp_fill: Query<&mut Node, (With<XpFill>, Without<HpFill>, Without<MpFill>, Without<ActionBarCooldown>)>,
    mut q_action_labels: Query<(&ActionBarLabel, &mut Text), (Without<PlayerNameText>, Without<PlayerLevelText>, Without<HpText>, Without<MpText>, Without<ClassSelectText>)>,
    mut q_cd_overlay: Query<(&ActionBarCooldown, &mut Visibility), Without<QuestTrackerRoot>>,
    mut q_class_select: Query<&mut Visibility, (With<ClassSelectText>, Without<ActionBarCooldown>, Without<QuestTrackerRoot>, Without<ChatInputBar>)>,
) {
    let now = time.elapsed_secs();
    let sheet = session.sheet.as_ref();

    // ── Player frame ─────────────────────────────────────────────────────
    if let Some(cs) = sheet {
        if let Ok(mut t) = q_name.get_single_mut() {
            **t = cs.name.clone();
        }
        if let Ok(mut t) = q_level.get_single_mut() {
            // Unit frame shows lineage + standing once chosen (C10).
            **t = match cs.faction.as_deref() {
                Some(f) => {
                    let rep = cs.reputation.get(f).copied().unwrap_or(0);
                    let rank = match rep {
                        i32::MIN..=2999 => "neutral",
                        3000..=8999 => "friendly",
                        9000..=20999 => "honored",
                        _ => "revered",
                    };
                    format!("Lv {} · {f} ({rank})", cs.level)
                }
                None => format!("Lv {}", cs.level),
            };
        }

        // HP bar.
        let hp_frac = if cs.max_health > 0 {
            (cs.health.max(0) as f32 / cs.max_health as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if let Ok(mut n) = q_hp_fill.get_single_mut() {
            n.width = Val::Percent(hp_frac * 100.0);
        }
        if let Ok(mut t) = q_hp_text.get_single_mut() {
            **t = format!("{} / {}", cs.health.max(0), cs.max_health);
        }

        // MP bar.
        let mp_frac = if cs.max_mana > 0 {
            (cs.mana.max(0) as f32 / cs.max_mana as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if let Ok(mut n) = q_mp_fill.get_single_mut() {
            n.width = Val::Percent(mp_frac * 100.0);
        }
        if let Ok(mut t) = q_mp_text.get_single_mut() {
            **t = format!("{} / {}", cs.mana.max(0), cs.max_mana);
        }

        // XP bar.
        let xp_frac = if cs.max_xp > 0 {
            (cs.xp as f32 / cs.max_xp as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if let Ok(mut n) = q_xp_fill.get_single_mut() {
            n.width = Val::Percent(xp_frac * 100.0);
        }
    }

    // ── Action bar labels ────────────────────────────────────────────────
    let abilities: Option<[&str; 2]> = session.class.map(class_abilities);
    for (label, mut text) in q_action_labels.iter_mut() {
        let name = match label.0 {
            0 => "Attack",
            1 => abilities.map_or("—", |a| a[0]),
            2 => abilities.map_or("—", |a| a[1]),
            _ => "—",
        };
        // Prettify: replace underscores, title-case first letter.
        let pretty = prettify_ability(name);
        if **text != pretty {
            **text = pretty;
        }
    }

    // Cooldown overlays.
    for (cd, mut vis) in q_cd_overlay.iter_mut() {
        *vis = if cooldowns.on_cd(cd.0, now) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    // Class-select prompt visibility.
    if let Ok(mut vis) = q_class_select.get_single_mut() {
        *vis = if session.class.is_none() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Drive quest tracker + chat panels from `Session` each frame.
#[allow(clippy::type_complexity)]
pub fn update_ui_panels(
    session: Res<Session>,
    mut q_quest_root: Query<&mut Visibility, (With<QuestTrackerRoot>, Without<ChatInputBar>)>,
    mut q_quest_text: Query<&mut Text, (With<QuestTrackerText>, Without<ChatLogText>, Without<ChatInputText>)>,
    mut q_chat_log: Query<&mut Text, (With<ChatLogText>, Without<QuestTrackerText>, Without<ChatInputText>)>,
    mut q_chat_input_text: Query<&mut Text, (With<ChatInputText>, Without<QuestTrackerText>, Without<ChatLogText>)>,
    mut q_chat_input_bar: Query<&mut Visibility, (With<ChatInputBar>, Without<QuestTrackerRoot>)>,
) {
    let sheet = session.sheet.as_ref();

    // ── Quest tracker ────────────────────────────────────────────────────
    if let Some(cs) = sheet {
        let has_quests = !cs.quests.is_empty();
        if let Ok(mut vis) = q_quest_root.get_single_mut() {
            *vis = if has_quests { Visibility::Inherited } else { Visibility::Hidden };
        }
        if has_quests {
            if let Ok(mut t) = q_quest_text.get_single_mut() {
                let mut lines = String::new();
                for (name, progress) in &cs.quests {
                    if !lines.is_empty() {
                        lines.push('\n');
                    }
                    let pretty = prettify_ability(name.strip_prefix("fa_").unwrap_or(name));
                    // Theme-pillar quests (C08) carry their pillar as a prefix.
                    let theme = if name.starts_with("fa_") { "[Forbidden Arts] " } else { "" };
                    lines.push_str(&format!("• {theme}{} — {}/target", pretty, progress));
                }
                **t = lines;
            }
        }
    }

    // ── Chat log ─────────────────────────────────────────────────────────
    if let Ok(mut t) = q_chat_log.get_single_mut() {
        // Show last ~8 lines.
        let start = session.chat_log.len().saturating_sub(8);
        let visible: Vec<&str> = session.chat_log.iter().skip(start).map(|s| s.as_str()).collect();
        let joined = visible.join("\n");
        if **t != joined {
            **t = joined;
        }
    }

    // ── Chat input ───────────────────────────────────────────────────────
    if let Ok(mut vis) = q_chat_input_bar.get_single_mut() {
        *vis = if session.chat_active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if session.chat_active {
        if let Ok(mut t) = q_chat_input_text.get_single_mut() {
            let display = format!("Say: {}▌", session.chat_input);
            if **t != display {
                **t = display;
            }
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Turn `"heroic_strike"` into `"Heroic Strike"`.
fn prettify_ability(raw: &str) -> String {
    raw.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    upper + c.as_str()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fill the target frame from `Session.target`; hide it when nothing is near.
pub fn update_target_frame(
    session: Res<Session>,
    mut q_root: Query<&mut Visibility, With<TargetFrameRoot>>,
    mut q_name: Query<&mut Text, (With<TargetNameText>, Without<TargetHpText>)>,
    mut q_hp: Query<&mut Text, (With<TargetHpText>, Without<TargetNameText>)>,
) {
    let Ok(mut vis) = q_root.get_single_mut() else { return };
    match &session.target {
        Some((name, hp, max)) => {
            *vis = Visibility::Visible;
            if let Ok(mut t) = q_name.get_single_mut() {
                **t = name.clone();
            }
            if let Ok(mut t) = q_hp.get_single_mut() {
                **t = format!("{} / {}", hp.max(&0), max);
            }
        }
        None => {
            *vis = Visibility::Hidden;
        }
    }
}

/// Show and fade the discovery banner from `Session.banner`.
pub fn update_banner(
    time: Res<Time>,
    mut session: ResMut<Session>,
    mut q: Query<(&mut Text, &mut TextColor), With<BannerText>>,
) {
    let Ok((mut text, mut color)) = q.get_single_mut() else { return };
    match session.banner.as_mut() {
        Some((msg, left)) => {
            *left -= time.delta_secs().min(0.1); // long loading frames must not eat the banner
            let alpha = (*left / 0.8).clamp(0.0, 1.0); // fade out over the last 0.8 s
            **text = msg.clone();
            *color = TextColor(GOLD_ACCENT.with_alpha(alpha));
            if *left <= 0.0 {
                session.banner = None;
                **text = String::new();
            }
        }
        None => {
            if !text.is_empty() {
                **text = String::new();
            }
        }
    }
}
