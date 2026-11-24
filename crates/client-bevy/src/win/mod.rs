// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Classic MMO windows (B01/B02/B04/B06-B08 client side).
//!
//! One small framework, one module per window:
//!
//! * `WinState` holds which windows are open (a stack: the last one is on
//!   top and is what Escape closes), the data the server sent for them
//!   (board, flight routes, friends, who, raid, channels, loot) and the
//!   latest snapshot reduced to radar blips (minimap, markers, death).
//! * Windows are rebuilt from scratch whenever their data changes. They are
//!   plain Bevy UI; nothing here reads movement keys, so an open window never
//!   stops the character from walking.
//! * Every button carries a [`WinAction`]; most simply send `ClientMsg`s, so
//!   the same windows work against the server and the solo (`local`) driver.
//!
//! `ANTEDILUVIA_AUTOCMD="ui bags;ui map;ui close"` opens windows in the
//! windowless screenshot mode (see [`WinState::autocmd`]).

pub mod bags;
pub mod board;
pub mod buffs;
pub mod charsheet;
pub mod chattabs;
pub mod death;
pub mod devpanel;
pub mod flight;
pub mod layout;
pub mod loot;
pub mod markers;
pub mod minimap;
pub mod questlog;
pub mod rested;
pub mod social;
pub mod spellbook;
pub mod vendor;
pub mod worldmap;
pub mod zonebanner;

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

use antediluvia_protocol::{
    Act, BoardQuest, CharacterSheet, ClientMsg, EntityKind, EntityState, FlightNode, FriendEntry,
    ServerMsg, WhoEntry,
};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use crate::net::NetTx;
use crate::Session;

// ─── Design tokens (match ui.rs / menu.rs) ───────────────────────────────────

pub(crate) const PANEL: Color = Color::srgba(0.08, 0.08, 0.12, 0.94);
pub(crate) const INSET: Color = Color::srgba(0.13, 0.13, 0.17, 0.95);
pub(crate) const TEXT: Color = Color::srgb(0.92, 0.90, 0.85);
pub(crate) const TEXT_DIM: Color = Color::srgb(0.62, 0.60, 0.55);
pub(crate) const GOLD: Color = Color::srgb(0.98, 0.90, 0.55);
pub(crate) const GOOD: Color = Color::srgb(0.45, 0.90, 0.45);
pub(crate) const BAD: Color = Color::srgb(0.95, 0.38, 0.32);
pub(crate) const BTN: Color = Color::srgb(0.16, 0.16, 0.20);
pub(crate) const BTN_HOVER: Color = Color::srgb(0.26, 0.26, 0.33);
pub(crate) const BORDER: Color = Color::srgb(0.45, 0.40, 0.28);

// ─── Window identity ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Win {
    Bags,
    Character,
    QuestLog,
    Spellbook,
    Map,
    Vendor,
    Loot,
    Board,
    Flight,
    Social,
}

impl Win {
    pub const ALL: [Win; 10] = [
        Win::Bags,
        Win::Character,
        Win::QuestLog,
        Win::Spellbook,
        Win::Map,
        Win::Vendor,
        Win::Loot,
        Win::Board,
        Win::Flight,
        Win::Social,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Win::Bags => "Bags",
            Win::Character => "Character",
            Win::QuestLog => "Quest Log",
            Win::Spellbook => "Spellbook",
            Win::Map => "World Map",
            Win::Vendor => "Vendor",
            Win::Loot => "Loot",
            Win::Board => "Notice Board",
            Win::Flight => "Flight Master",
            Win::Social => "Social",
        }
    }

    /// Name used by the `ui <name>` autocmd and `/ui <name>`.
    pub fn name(self) -> &'static str {
        match self {
            Win::Bags => "bags",
            Win::Character => "character",
            Win::QuestLog => "quests",
            Win::Spellbook => "spellbook",
            Win::Map => "map",
            Win::Vendor => "vendor",
            Win::Loot => "loot",
            Win::Board => "board",
            Win::Flight => "flight",
            Win::Social => "social",
        }
    }

    pub fn from_name(s: &str) -> Option<Win> {
        let s = s.trim().to_lowercase();
        Win::ALL.into_iter().find(|w| w.name() == s).or(match s.as_str() {
            "bag" | "inventory" => Some(Win::Bags),
            "char" | "sheet" | "paperdoll" => Some(Win::Character),
            "questlog" | "quest" | "log" => Some(Win::QuestLog),
            "spells" | "abilities" => Some(Win::Spellbook),
            "worldmap" => Some(Win::Map),
            "flights" => Some(Win::Flight),
            "friends" | "who" | "raid" => Some(Win::Social),
            _ => None,
        })
    }

    /// Key that toggles the window (the data-driven ones open on arrival).
    pub fn key(self) -> Option<KeyCode> {
        match self {
            Win::Bags => Some(KeyCode::KeyB),
            Win::Character => Some(KeyCode::KeyC),
            Win::QuestLog => Some(KeyCode::KeyL),
            Win::Spellbook => Some(KeyCode::KeyP),
            Win::Map => Some(KeyCode::KeyM),
            Win::Social => Some(KeyCode::KeyO),
            _ => None,
        }
    }

    /// Messages that refresh a window's server-side data when it opens.
    fn on_open(self) -> Vec<ClientMsg> {
        match self {
            Win::Social => vec![ClientMsg::FriendList, ClientMsg::Who { filter: None }, ClientMsg::ChannelList],
            _ => Vec::new(),
        }
    }
}

// ─── State ───────────────────────────────────────────────────────────────────

/// One entity of the latest snapshot, reduced to what the radar needs.
#[derive(Clone, Debug)]
pub struct Blip {
    pub kind: EntityKind,
    pub x: f32,
    pub y: f32,
    pub name: Option<String>,
    pub alive: bool,
}

/// A timed effect shown in the buff row.
#[derive(Clone, Debug, PartialEq)]
pub struct Buff {
    pub label: String,
    pub tip: String,
    pub until: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SocialTab {
    #[default]
    Friends,
    Ignore,
    Who,
    Raid,
    Channels,
}

#[derive(Clone, Debug)]
pub struct ZoneCard {
    pub name: String,
    pub subtitle: String,
    pub pvp: bool,
    pub left: f32,
}

#[derive(Resource)]
pub struct WinState {
    /// Open windows, bottom to top.
    pub open: Vec<Win>,
    /// Something a window shows changed: rebuild.
    pub dirty: bool,
    /// Escape closed a window this frame, so the game menu must not open.
    pub esc_consumed: bool,
    /// `ui menu` autocmd: open the Escape menu without a key press.
    pub want_menu: bool,
    /// The pointer is over a window (suppresses click-to-attack).
    pub pointer_busy: bool,
    // ── radar ──
    pub blips: Vec<Blip>,
    /// Own position and facing from the latest snapshot.
    pub me: Option<(f32, f32, f32)>,
    pub me_health: i32,
    // ── server data ──
    pub board: Option<(String, Vec<BoardQuest>)>,
    pub flights: Option<(Option<String>, Vec<FlightNode>)>,
    pub friends: Vec<FriendEntry>,
    pub ignored: Vec<String>,
    pub who: Vec<WhoEntry>,
    pub who_total: u32,
    pub raid_leader: String,
    pub raid: Vec<String>,
    pub channels_joined: Vec<String>,
    pub channels_available: Vec<String>,
    /// Most recent drops, newest last.
    pub loot: VecDeque<String>,
    /// A drop arrived: (re)start the loot window's timer.
    pub loot_fresh: bool,
    /// Seconds until the auto-opened loot window closes itself.
    pub loot_left: f32,
    pub zone: Option<ZoneCard>,
    pub buffs: Vec<Buff>,
    pub chat_tab: chattabs::ChatTab,
    pub social_tab: SocialTab,
}

impl Default for WinState {
    fn default() -> Self {
        Self {
            open: Vec::new(),
            dirty: false,
            esc_consumed: false,
            want_menu: false,
            pointer_busy: false,
            blips: Vec::new(),
            me: None,
            me_health: 1,
            board: None,
            flights: None,
            friends: Vec::new(),
            ignored: Vec::new(),
            who: Vec::new(),
            who_total: 0,
            raid_leader: String::new(),
            raid: Vec::new(),
            channels_joined: Vec::new(),
            channels_available: Vec::new(),
            loot: VecDeque::new(),
            loot_fresh: false,
            loot_left: 0.0,
            zone: None,
            buffs: Vec::new(),
            chat_tab: chattabs::ChatTab::All,
            social_tab: SocialTab::Friends,
        }
    }
}

impl WinState {
    pub fn is_open(&self, w: Win) -> bool {
        self.open.contains(&w)
    }

    /// Open (or raise) a window.
    pub fn open(&mut self, w: Win) {
        self.open.retain(|o| *o != w);
        self.open.push(w);
        self.dirty = true;
    }

    pub fn close(&mut self, w: Win) {
        let before = self.open.len();
        self.open.retain(|o| *o != w);
        if self.open.len() != before {
            self.dirty = true;
        }
    }

    /// Returns true when the window is open afterwards.
    pub fn toggle(&mut self, w: Win) -> bool {
        if self.is_open(w) {
            self.close(w);
            false
        } else {
            self.open(w);
            true
        }
    }

    /// Close the topmost window. False when none was open.
    pub fn close_top(&mut self) -> bool {
        match self.open.pop() {
            Some(_) => {
                self.dirty = true;
                true
            }
            None => false,
        }
    }

    /// `ui <what>` from `ANTEDILUVIA_AUTOCMD`: `ui bags`, `ui only map`,
    /// `ui close`, `ui menu`, `ui social:who`, `ui chat:trade`.
    /// Returns the refresh messages the opened window wants sent.
    pub fn autocmd(&mut self, arg: &str) -> Vec<ClientMsg> {
        let arg = arg.trim();
        if let Some(rest) = arg.strip_prefix("only ") {
            self.open.clear();
            self.dirty = true;
            return self.autocmd(rest);
        }
        match arg {
            "close" | "none" => {
                self.open.clear();
                self.dirty = true;
                return Vec::new();
            }
            "menu" => {
                self.want_menu = true;
                return Vec::new();
            }
            _ => {}
        }
        if let Some(tab) = arg.strip_prefix("chat:") {
            if let Some(t) = chattabs::ChatTab::from_name(tab) {
                self.chat_tab = t;
            }
            return Vec::new();
        }
        let (name, sub) = arg.split_once(':').unwrap_or((arg, ""));
        let Some(w) = Win::from_name(name) else { return Vec::new() };
        if w == Win::Social {
            self.social_tab = match sub {
                "ignore" => SocialTab::Ignore,
                "who" => SocialTab::Who,
                "raid" => SocialTab::Raid,
                "channels" => SocialTab::Channels,
                _ => self.social_tab,
            };
        }
        self.open(w);
        // The data-driven windows ask the server; it replies only when the
        // character stands at a board / flight master.
        match w {
            Win::Board => vec![ClientMsg::QuestBoard],
            Win::Flight => vec![ClientMsg::FlightList],
            other => other.on_open(),
        }
    }

    /// Reduce a snapshot to radar blips.
    pub fn radar(&mut self, entities: &[EntityState], my_id: Option<u64>) {
        self.blips.clear();
        for e in entities {
            if Some(e.id) == my_id {
                self.me = Some((e.x, e.y, e.rot));
                self.me_health = e.health;
                continue;
            }
            self.blips.push(Blip { kind: e.kind, x: e.x, y: e.y, name: e.name.clone(), alive: e.health > 0 });
        }
    }

    /// A server notice: loot lines feed the loot window, vendor greetings
    /// open the vendor window.
    pub fn on_notice(&mut self, text: &str) {
        if let Some(item) = loot::parse_loot(text) {
            if self.loot.len() >= loot::KEEP {
                self.loot.pop_front();
            }
            self.loot.push_back(item.to_string());
            self.loot_fresh = true;
            self.dirty = true;
        } else if text.starts_with("Innkeeper:") || text.starts_with("Quartermaster:") {
            self.open(Win::Vendor);
        }
    }

    /// One of our abilities fired: start its timed self-buffs.
    pub fn on_ability(&mut self, ability: &str, now: f32) {
        for b in buffs::buffs_of(ability, now) {
            self.buffs.retain(|o| o.label != b.label);
            self.buffs.push(b);
        }
    }
}

/// Route the beta-server-track messages: structured data goes to the windows,
/// the rest (and anything a closed window would otherwise hide) to the chat.
pub(crate) fn on_server_msg(msg: ServerMsg, session: &mut Session, st: &mut WinState) {
    match msg {
        ServerMsg::QuestBoard { place, quests } => {
            st.board = Some((place, quests));
            st.open(Win::Board);
        }
        ServerMsg::FlightPaths { here, nodes } => {
            st.flights = Some((here, nodes));
            st.open(Win::Flight);
        }
        ServerMsg::ZoneBanner { ref name, ref subtitle, pvp, .. } => {
            st.zone = Some(ZoneCard { name: name.clone(), subtitle: subtitle.clone(), pvp, left: zonebanner::SHOW_SECS });
            crate::track_ui::on_server_msg(msg, session);
            // The dedicated banner replaces the one-line fallback.
            session.banner = None;
        }
        ServerMsg::Friends { ref friends, ref ignored } => {
            st.friends = friends.clone();
            st.ignored = ignored.clone();
            st.dirty = true;
            if !st.is_open(Win::Social) {
                crate::track_ui::on_server_msg(msg, session);
            }
        }
        ServerMsg::WhoList { ref players, total } => {
            st.who = players.clone();
            st.who_total = total;
            st.dirty = true;
            if !st.is_open(Win::Social) {
                crate::track_ui::on_server_msg(msg, session);
            }
        }
        ServerMsg::RaidInfo { ref leader, ref members } => {
            st.raid_leader = leader.clone();
            st.raid = members.clone();
            st.dirty = true;
            crate::track_ui::on_server_msg(msg, session);
        }
        ServerMsg::Channels { ref joined, ref available } => {
            st.channels_joined = joined.clone();
            st.channels_available = available.clone();
            st.dirty = true;
            if !st.is_open(Win::Social) {
                crate::track_ui::on_server_msg(msg, session);
            }
        }
        // The bags and character sheet compute rarity and wear from the sheet
        // (same sim tables as the server), so `Gear` only matters to `/gear`.
        other => crate::track_ui::on_server_msg(other, session),
    }
}

// ─── Actions ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum Action {
    Close(Win),
    /// Send these messages in order.
    Send(Vec<ClientMsg>),
    SocialTab(SocialTab),
    ChatTab(chattabs::ChatTab),
}

/// Left-click action of a button.
#[derive(Component)]
pub struct WinAction(pub Action);
/// Right-click action (bag slots).
#[derive(Component)]
pub struct RightAction(pub Action);
/// Hover text.
#[derive(Component)]
pub struct Tip(pub String);
/// Root node of a window.
#[derive(Component)]
pub struct WinRoot;
/// The pointer over this node belongs to the UI, not the world.
#[derive(Component)]
pub struct Blocks;
/// Gets the standard hover highlight.
#[derive(Component)]
pub struct Hoverable;
#[derive(Component)]
struct TipRoot;
#[derive(Component)]
struct TipText;

fn perform(action: &Action, st: &mut WinState, tx: &NetTx) {
    match action {
        Action::Close(w) => st.close(*w),
        Action::Send(msgs) => {
            for m in msgs {
                tx.send(m.clone());
            }
        }
        Action::SocialTab(t) => {
            st.social_tab = *t;
            st.dirty = true;
        }
        Action::ChatTab(t) => st.chat_tab = *t,
    }
}

// ─── UI helpers ──────────────────────────────────────────────────────────────

pub(crate) fn text(p: &mut ChildBuilder, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s), TextFont { font_size: size, ..default() }, TextColor(color)));
}

/// A standard bordered button.
pub(crate) fn button(p: &mut ChildBuilder, label: impl Into<String>, action: Action, color: Color) {
    p.spawn((
        Button,
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
            border: UiRect::all(Val::Px(1.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(BTN),
        BorderColor(BORDER),
        WinAction(action),
        Blocks,
        Hoverable,
    ))
    .with_children(|b| {
        b.spawn((
            Text::new(label),
            TextFont { font_size: 12.0, ..default() },
            TextColor(color),
            TextLayout::new_with_no_wrap(),
        ));
    });
}

/// A horizontal row with a gap; children are laid out left to right.
pub(crate) fn row(p: &mut ChildBuilder, f: impl FnOnce(&mut ChildBuilder)) {
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::SpaceBetween,
        column_gap: Val::Px(8.0),
        ..default()
    })
    .with_children(f);
}

/// A dark inset block (list rows, cards).
pub(crate) fn card(p: &mut ChildBuilder, f: impl FnOnce(&mut ChildBuilder)) {
    p.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(6.0)),
            row_gap: Val::Px(2.0),
            ..default()
        },
        BackgroundColor(INSET),
    ))
    .with_children(f);
}

/// "bronze_sword" -> "Bronze Sword"; "mount:dire_wolf" -> "Mount: Dire Wolf".
pub fn pretty(id: &str) -> String {
    id.replace(':', ": ")
        .split(['_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn spawn_window(commands: &mut Commands, win: Win, screen: (f32, f32), body: impl FnOnce(&mut ChildBuilder)) {
    let r = layout::rect(win, screen.0, screen.1);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(r.x),
                top: Val::Px(r.y),
                width: Val::Px(r.w),
                max_height: Val::Px(r.h),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(10.0)),
                row_gap: Val::Px(5.0),
                border: UiRect::all(Val::Px(1.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor(BORDER),
            Interaction::default(),
            FocusPolicy::Block,
            Blocks,
            WinRoot,
        ))
        .with_children(|p| {
            row(p, |r| {
                text(r, win.title(), 17.0, GOLD);
                button(r, "X", Action::Close(win), TEXT_DIM);
            });
            body(p);
        });
}

/// Everything a window body may read.
pub struct Ctx<'a> {
    pub sheet: &'a CharacterSheet,
    pub act: Act,
    pub st: &'a WinState,
    pub map_bg: Option<Handle<Image>>,
}

// ─── Systems ─────────────────────────────────────────────────────────────────

fn keys_free(session: &Session, menu_open: &crate::menu::MenuOpen) -> bool {
    !session.chat_active && session.builder.is_none() && session.dev_input.is_none() && !menu_open.0
}

/// Window hotkeys. Escape closes the top window (and tells the game menu to
/// stay shut this frame); F uses the board or flight master you stand at.
fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    session: Res<Session>,
    menu_open: Res<crate::menu::MenuOpen>,
    mut st: ResMut<WinState>,
    tx: Res<NetTx>,
) {
    if st.esc_consumed {
        st.esc_consumed = false;
    }
    if !keys_free(&session, &menu_open) || session.sheet.is_none() {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) && st.close_top() {
        st.esc_consumed = true;
        return;
    }
    for w in Win::ALL {
        if w.key().is_some_and(|k| keys.just_pressed(k)) && st.toggle(w) {
            for m in w.on_open() {
                tx.send(m);
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyF) {
        if let Some(msg) = st.me.and_then(|(x, y, _)| markers::place_action(session.act, x, y)) {
            tx.send(msg.1);
        }
    }
}

/// Cheap digest of everything the windows read from the sheet.
fn fingerprint(s: &CharacterSheet) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.inventory.hash(&mut h);
    s.equipment.hash(&mut h);
    s.durability.hash(&mut h);
    s.quests.hash(&mut h);
    s.quests_done.len().hash(&mut h);
    s.discovered.len().hash(&mut h);
    s.reputation.hash(&mut h);
    s.professions.hash(&mut h);
    (s.gold, s.level, s.xp, s.rested_xp, s.pvp, s.talent_points, s.max_health, s.max_mana).hash(&mut h);
    s.class.map(|c| c.as_str()).hash(&mut h);
    s.faction.hash(&mut h);
    s.guild.hash(&mut h);
    h.finish()
}

#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    mut st: ResMut<WinState>,
    session: Res<Session>,
    roots: Query<Entity, With<WinRoot>>,
    windows: Query<&Window>,
    mut images: ResMut<Assets<Image>>,
    mut last: Local<(u64, f32, f32, bool)>,
    mut map_bg: Local<Option<(Act, Handle<Image>)>>,
) {
    let screen = windows.iter().next().map_or((1600.0, 900.0), |w| (w.width(), w.height()));
    let Some(sheet) = session.sheet.as_ref().filter(|_| session.builder.is_none()) else {
        for e in &roots {
            commands.entity(e).despawn_recursive();
        }
        last.3 = false;
        return;
    };
    let fp = if st.open.is_empty() { 0 } else { fingerprint(sheet) ^ (session.act as u64) };
    if !st.dirty && last.3 && *last == (fp, screen.0, screen.1, true) {
        return;
    }
    *last = (fp, screen.0, screen.1, true);
    st.dirty = false;
    for e in &roots {
        commands.entity(e).despawn_recursive();
    }
    if st.is_open(Win::Map) && map_bg.as_ref().map(|(a, _)| *a) != Some(session.act) {
        *map_bg = Some((session.act, images.add(worldmap::terrain_image(session.act))));
    }
    let ctx = Ctx { sheet, act: session.act, st: &st, map_bg: map_bg.as_ref().map(|(_, h)| h.clone()) };
    for w in st.open.clone() {
        spawn_window(&mut commands, w, screen, |p| match w {
            Win::Bags => bags::build(p, &ctx),
            Win::Character => charsheet::build(p, &ctx),
            Win::QuestLog => questlog::build(p, &ctx),
            Win::Spellbook => spellbook::build(p, &ctx),
            Win::Map => worldmap::build(p, &ctx),
            Win::Vendor => vendor::build(p, &ctx),
            Win::Loot => loot::build(p, &ctx),
            Win::Board => board::build(p, &ctx),
            Win::Flight => flight::build(p, &ctx),
            Win::Social => social::build(p, &ctx),
        });
    }
}

#[allow(clippy::type_complexity)]
fn clicks(
    left: Query<(&Interaction, &WinAction), Changed<Interaction>>,
    right: Query<(&Interaction, &RightAction)>,
    blockers: Query<&Interaction, With<Blocks>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut st: ResMut<WinState>,
    tx: Res<NetTx>,
) {
    for (i, a) in &left {
        if *i == Interaction::Pressed {
            perform(&a.0, &mut st, &tx);
        }
    }
    if buttons.just_pressed(MouseButton::Right) {
        if let Some((_, a)) = right.iter().find(|(i, _)| **i != Interaction::None) {
            perform(&a.0, &mut st, &tx);
        }
    }
    let busy = blockers.iter().any(|i| *i != Interaction::None);
    if st.pointer_busy != busy {
        st.pointer_busy = busy;
    }
}

#[allow(clippy::type_complexity)]
fn hover(mut q: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<Hoverable>)>) {
    for (i, mut bg) in &mut q {
        bg.0 = if *i == Interaction::None { BTN } else { BTN_HOVER };
    }
}

fn spawn_tooltip(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                max_width: Val::Px(280.0),
                padding: UiRect::all(Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.03, 0.05, 0.97)),
            BorderColor(BORDER),
            GlobalZIndex(100),
            Visibility::Hidden,
            TipRoot,
        ))
        .with_children(|p| {
            p.spawn((Text::new(""), TextFont { font_size: 12.0, ..default() }, TextColor(TEXT), TipText));
        });
}

fn tooltip(
    tips: Query<(&Interaction, &Tip)>,
    windows: Query<&Window>,
    mut root: Query<(&mut Node, &mut Visibility), With<TipRoot>>,
    mut label: Query<&mut Text, With<TipText>>,
) {
    let Ok((mut node, mut vis)) = root.get_single_mut() else { return };
    let cursor = windows.iter().next().and_then(|w| w.cursor_position().map(|c| (c, w.width(), w.height())));
    let hovered = tips.iter().find(|(i, _)| **i != Interaction::None);
    match (hovered, cursor) {
        (Some((_, tip)), Some((c, w, h))) => {
            let (x, y) = layout::tip_pos(c.x, c.y, w, h);
            node.left = Val::Px(x);
            node.top = Val::Px(y);
            if let Ok(mut t) = label.get_single_mut() {
                if **t != tip.0 {
                    **t = tip.0.clone();
                }
            }
            *vis = Visibility::Inherited;
        }
        _ => {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        }
    }
}

pub struct WinPlugin;

impl Plugin for WinPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WinState>()
            .add_systems(
                Startup,
                (
                    spawn_tooltip,
                    minimap::spawn,
                    buffs::spawn,
                    zonebanner::spawn,
                    markers::spawn,
                    chattabs::spawn,
                ),
            )
            .add_systems(Update, hotkeys.before(crate::menu::menu_toggle))
            .add_systems(Update, (clicks, rebuild, hover, tooltip).chain().after(hotkeys))
            .add_systems(
                Update,
                (
                    minimap::update,
                    worldmap::update_player,
                    buffs::update,
                    rested::update,
                    death::update,
                    zonebanner::update,
                    markers::update,
                    chattabs::update,
                    loot::auto_close,
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_closes_the_top_window_first() {
        let mut st = WinState::default();
        assert!(!st.close_top(), "nothing open: Escape falls through to the menu");
        st.open(Win::Bags);
        st.open(Win::Map);
        st.open(Win::Bags); // raising must not duplicate
        assert_eq!(st.open, vec![Win::Map, Win::Bags]);
        assert!(st.close_top());
        assert_eq!(st.open, vec![Win::Map]);
        assert!(st.close_top());
        assert!(!st.close_top());
    }

    #[test]
    fn toggle_and_names_round_trip() {
        let mut st = WinState::default();
        for w in Win::ALL {
            assert_eq!(Win::from_name(w.name()), Some(w));
            assert!(st.toggle(w));
            assert!(!st.toggle(w));
        }
        assert!(st.open.is_empty());
        assert_eq!(Win::from_name("nonsense"), None);
    }

    #[test]
    fn autocmd_opens_closes_and_asks_the_server() {
        let mut st = WinState::default();
        assert!(st.autocmd("bags").is_empty());
        assert!(st.is_open(Win::Bags));
        assert_eq!(st.autocmd("only social:who").len(), 3);
        assert_eq!(st.open, vec![Win::Social]);
        assert_eq!(st.social_tab, SocialTab::Who);
        assert!(matches!(st.autocmd("board")[..], [ClientMsg::QuestBoard]));
        st.autocmd("chat:trade");
        assert_eq!(st.chat_tab, chattabs::ChatTab::Trade);
        st.autocmd("menu");
        assert!(st.want_menu);
        st.autocmd("close");
        assert!(st.open.is_empty());
    }

    #[test]
    fn notices_feed_loot_and_vendor() {
        let mut st = WinState::default();
        st.on_notice("You collected wolf_pelt.");
        assert_eq!(st.loot.back().map(String::as_str), Some("wolf_pelt"));
        assert!(st.loot_fresh);
        st.on_notice("Innkeeper: I stock bread 2g.");
        assert!(st.is_open(Win::Vendor));
        for i in 0..40 {
            st.on_notice(&format!("You collected thing_{i}."));
        }
        assert_eq!(st.loot.len(), loot::KEEP);
    }

    #[test]
    fn pretty_names() {
        assert_eq!(pretty("bronze_sword"), "Bronze Sword");
        assert_eq!(pretty("mount:dire_wolf"), "Mount: Dire Wolf");
    }
}
