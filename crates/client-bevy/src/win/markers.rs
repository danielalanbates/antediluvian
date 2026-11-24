// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! "!" / "?" markers over quest givers and notice boards (B02), and the
//! "F - ..." hint shown while standing at a board or a flight master.
//!
//! The client has the same quest tables as the server and the character
//! sheet carries quest state, so the markers need no extra messages.

use antediluvia_protocol::{Act, CharacterSheet, ClientMsg, EntityKind};
use antediluvia_sim::quests::{self, quest_faction};
use antediluvia_sim::{questgen, travel};
use bevy::prelude::*;

use super::{questlog, WinState, GOLD};
use crate::terrain::terrain_height;
use crate::Session;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mark {
    /// Gold "?": something to hand in here.
    Ready,
    /// Gold "!": a quest can be taken here.
    Available,
    /// Grey "?": a quest from here is still in progress.
    Pending,
}

impl Mark {
    pub fn glyph(self) -> &'static str {
        match self {
            Mark::Available => "!",
            Mark::Ready | Mark::Pending => "?",
        }
    }
    pub fn color(self) -> Color {
        match self {
            Mark::Ready | Mark::Available => Color::srgb(1.0, 0.86, 0.12),
            Mark::Pending => Color::srgb(0.62, 0.62, 0.62),
        }
    }
}

/// NPC names that give authored quests.
pub fn is_giver(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(), "elder" | "wanderer" | "seer" | "sentinel" | "jabal")
}

/// Marker for an authored quest giver (lowercase key, e.g. "elder").
pub fn giver_mark(sheet: &CharacterSheet, act: Act, giver: &str) -> Option<Mark> {
    let mut best: Option<Mark> = None;
    let mut raise = |m: Mark| {
        let rank = |m: Mark| match m {
            Mark::Ready => 3,
            Mark::Available => 2,
            Mark::Pending => 1,
        };
        if best.map_or(true, |b| rank(m) > rank(b)) {
            best = Some(m);
        }
    };
    for q in quests::quests_for(act, giver) {
        if sheet.quests_done.iter().any(|d| d == q.id) {
            continue;
        }
        if sheet.quests.contains_key(q.id) {
            raise(if questlog::complete(q, sheet) { Mark::Ready } else { Mark::Pending });
            continue;
        }
        let chained = q.requires.map_or(true, |r| sheet.quests_done.iter().any(|d| d == r));
        let lineage = quest_faction(q.id).map_or(true, |f| sheet.faction.as_deref() == Some(f));
        if chained && lineage && sheet.level >= q.min_level && sheet.quests.len() < quests::QUEST_CAP {
            raise(Mark::Available);
        }
    }
    best
}

/// Marker for the notice board at anchor index `anchor`.
pub fn board_mark(sheet: &CharacterSheet, anchor: usize) -> Option<Mark> {
    let mut best: Option<Mark> = None;
    for g in questgen::generated().iter().filter(|g| g.anchor == anchor) {
        let id = g.def.id;
        if sheet.quests_done.iter().any(|d| d == id) {
            continue;
        }
        let m = if sheet.quests.contains_key(id) {
            if questlog::complete(&g.def, sheet) {
                Mark::Ready
            } else {
                Mark::Pending
            }
        } else {
            // Board quests sit behind the act's main story and a level gate.
            let chained = g.def.requires.map_or(true, |r| sheet.quests_done.iter().any(|d| d == r));
            if !chained || sheet.level < g.def.min_level || sheet.quests.len() >= quests::QUEST_CAP {
                continue;
            }
            Mark::Available
        };
        best = match (best, m) {
            (Some(Mark::Ready), _) | (_, Mark::Ready) => Some(Mark::Ready),
            (Some(Mark::Available), _) | (_, Mark::Available) => Some(Mark::Available),
            _ => Some(Mark::Pending),
        };
    }
    best
}

/// What F does where the character stands: (hint, message).
pub fn place_action(act: Act, x: f32, y: f32) -> Option<(String, ClientMsg)> {
    let pos = glam::Vec2::new(x, y);
    if let Some(i) = questgen::anchor_at(act, pos) {
        return Some((format!("F - read the notice board at {}", questgen::anchors()[i].name), ClientMsg::QuestBoard));
    }
    travel::node_at(act, pos).map(|n| (format!("F - flight master ({})", n.name), ClientMsg::FlightList))
}

const POOL: usize = 14;
/// Marks float this far above the ground.
const HEIGHT: f32 = 98.0;
/// Boards farther than this are not marked.
const BOARD_RANGE: f32 = 1500.0;

#[derive(Component)]
pub(crate) struct MarkLabel(usize);
#[derive(Component)]
pub(crate) struct HintText;

pub(crate) fn spawn(mut commands: Commands) {
    for i in 0..POOL {
        commands.spawn((
            Text::new("!"),
            TextFont { font_size: 34.0, ..default() },
            TextColor(GOLD),
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), ..default() },
            // Under the HUD and windows: a marker is part of the world.
            GlobalZIndex(-1),
            Visibility::Hidden,
            MarkLabel(i),
        ));
    }
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(140.0),
            left: Val::Px(0.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|p| {
            p.spawn((Text::new(""), TextFont { font_size: 15.0, ..default() }, TextColor(GOLD), HintText));
        });
}

#[allow(clippy::type_complexity)]
pub(crate) fn update(
    time: Res<Time>,
    session: Res<Session>,
    st: Res<WinState>,
    cam: Query<(&Camera, &GlobalTransform), With<crate::MainCamera>>,
    mut labels: Query<(&MarkLabel, &mut Node, &mut Text, &mut TextColor, &mut Visibility), Without<HintText>>,
    mut hint: Query<&mut Text, (With<HintText>, Without<MarkLabel>)>,
    mut cache: Local<(f32, Vec<(Vec3, Mark)>, String)>,
) {
    let sheet = session.sheet.as_ref().filter(|_| session.builder.is_none());
    let now = time.elapsed_secs();
    // Recompute what is marked twice a second; positions follow every frame.
    if now - cache.0 > 0.5 || now < cache.0 {
        cache.0 = now;
        cache.1.clear();
        cache.2.clear();
        if let (Some(s), Some((mx, my, _))) = (sheet, st.me) {
            let act = session.act;
            let at = |x: f32, y: f32| Vec3::new(x, terrain_height(act, x, y) + HEIGHT, y);
            for b in st.blips.iter().filter(|b| b.kind == EntityKind::Npc) {
                let Some(name) = b.name.as_deref() else { continue };
                if let Some(m) = giver_mark(s, act, &name.to_lowercase()) {
                    cache.1.push((at(b.x, b.y), m));
                }
            }
            for (i, a) in questgen::anchors().iter().enumerate() {
                if a.act != act || (a.pos.x - mx).hypot(a.pos.y - my) > BOARD_RANGE {
                    continue;
                }
                if let Some(m) = board_mark(s, i) {
                    cache.1.push((at(a.pos.x, a.pos.y), m));
                }
            }
            cache.1.truncate(POOL);
            if st.me_health > 0 {
                if let Some((h, _)) = place_action(act, mx, my) {
                    cache.2 = h;
                }
            }
        }
    }
    if let Ok(mut t) = hint.get_single_mut() {
        if **t != cache.2 {
            **t = cache.2.clone();
        }
    }
    let cam = cam.get_single().ok();
    for (label, mut node, mut text, mut color, mut vis) in &mut labels {
        let shown = cache.1.get(label.0).and_then(|(world, mark)| {
            let (camera, xf) = cam?;
            // Behind the camera or very far away: no marker.
            if world.distance(xf.translation()) > 2600.0 {
                return None;
            }
            camera.world_to_viewport(xf, *world).ok().map(|p| (p, *mark))
        });
        match shown {
            Some((p, mark)) => {
                node.left = Val::Px(p.x - 9.0);
                node.top = Val::Px(p.y - 20.0);
                if **text != mark.glyph() {
                    **text = mark.glyph().to_string();
                }
                if color.0 != mark.color() {
                    color.0 = mark.color();
                }
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
            None => {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antediluvia_sim::quests::QUESTS;

    fn sheet() -> CharacterSheet {
        antediluvia_sim::world::new_character("Tester")
    }

    #[test]
    fn giver_goes_from_bang_to_grey_to_gold_question_to_nothing_or_next() {
        // A level-1 quest with no prerequisite and no lineage lock.
        let q = QUESTS
            .iter()
            .find(|q| q.requires.is_none() && q.min_level <= 1 && quest_faction(q.id).is_none())
            .expect("a starter quest exists");
        let mut s = sheet();
        s.quests.clear();
        s.quests_done.clear();
        assert_eq!(giver_mark(&s, q.act, q.giver), Some(Mark::Available));
        s.quests.insert(q.id.into(), 0);
        let m = giver_mark(&s, q.act, q.giver);
        assert!(matches!(m, Some(Mark::Pending) | Some(Mark::Available)), "{m:?}");
        // Complete it (kill count or bag contents, whichever it wants).
        s.quests.insert(q.id.into(), q.objective.count());
        if let antediluvia_sim::quests::Objective::Collect { item, count, .. } = q.objective {
            for _ in 0..count {
                s.inventory.push(item.into());
            }
        }
        assert_eq!(giver_mark(&s, q.act, q.giver), Some(Mark::Ready));
        assert_eq!(giver_mark(&s, q.act, "nobody"), None);
    }

    #[test]
    fn chained_quests_stay_hidden_until_their_prerequisite() {
        let q = QUESTS.iter().find(|q| q.requires.is_some()).unwrap();
        let mut s = sheet();
        s.level = 60;
        s.quests.clear();
        s.quests_done.clear();
        // Only this giver's unchained quests may show; mark every other
        // quest of the giver done so the chained one decides the marker.
        for o in quests::quests_for(q.act, q.giver).filter(|o| o.id != q.id) {
            s.quests_done.push(o.id.into());
        }
        s.quests_done.retain(|d| Some(d.as_str()) != q.requires);
        let before = giver_mark(&s, q.act, q.giver);
        s.quests_done.push(q.requires.unwrap().into());
        let after = giver_mark(&s, q.act, q.giver);
        if quest_faction(q.id).is_none() {
            assert_eq!(after, Some(Mark::Available));
        }
        // The prerequisite itself may belong to this giver (then `before`
        // shows it as available); otherwise nothing was on offer.
        let prereq_here = quests::quests_for(q.act, q.giver).any(|o| Some(o.id) == q.requires);
        if !prereq_here {
            assert_eq!(before, None);
        }
    }

    #[test]
    fn boards_are_gated_behind_the_main_story() {
        let g = &questgen::generated()[0];
        let mut s = sheet();
        s.level = 60;
        s.quests.clear();
        s.quests_done.clear();
        if g.def.requires.is_some() {
            assert_eq!(board_mark(&s, g.anchor), None, "unchained board quests must not be offered");
        }
        for other in questgen::generated().iter().filter(|o| o.anchor == g.anchor) {
            if let Some(r) = other.def.requires {
                s.quests_done.push(r.into());
            }
        }
        assert_eq!(board_mark(&s, g.anchor), Some(Mark::Available));
        s.quests.insert(g.def.id.into(), 0);
        assert!(board_mark(&s, g.anchor).is_some());
    }

    #[test]
    fn f_key_finds_boards_and_flight_masters() {
        let a = &questgen::anchors()[0];
        let (hint, msg) = place_action(a.act, a.pos.x, a.pos.y).unwrap();
        assert!(hint.contains(a.name));
        assert!(matches!(msg, ClientMsg::QuestBoard));
        // The inn is a flight master and never a board.
        assert!(matches!(place_action(Act::Eden, 0.0, 0.0), Some((_, ClientMsg::FlightList))));
    }

    #[test]
    fn giver_names() {
        assert!(is_giver("Elder") && is_giver("seer"));
        assert!(!is_giver("Innkeeper") && !is_giver("Quartermaster"));
    }
}
