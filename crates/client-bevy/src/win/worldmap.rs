// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! World map (M): a top-down picture of the current act's terrain with the
//! inn, points of interest (named once discovered), caves, graveyards,
//! flight masters, enemy capitals, quest pins and the player's arrow.
//! North (server -y, the default camera's "forward") is up.

use antediluvia_protocol::{Act, CharacterSheet, WORLD_BOUNDS};
use antediluvia_sim::quests::{self, Objective};
use antediluvia_sim::{caves, pois, questgen, travel, world as simw};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::{questlog, text, Blocks, Ctx, Tip, GOLD, TEXT_DIM};
use crate::terrain::{terrain_height, water_level};

/// Side of the square map picture, in pixels.
pub const MAP_PX: f32 = 580.0;
/// Resolution of the terrain picture.
const TEX: u32 = 96;

/// World position -> map pixel (origin top-left). Positions outside the
/// playable square are clamped onto its edge.
pub fn project(x: f32, y: f32, size: f32) -> (f32, f32) {
    let f = |v: f32| ((v + WORLD_BOUNDS) / (2.0 * WORLD_BOUNDS)).clamp(0.0, 1.0) * size;
    (f(x), f(y))
}

/// Inverse of [`project`] (centre of the pixel grid cell).
pub fn unproject(px: f32, py: f32, size: f32) -> (f32, f32) {
    let f = |p: f32| (p / size) * 2.0 * WORLD_BOUNDS - WORLD_BOUNDS;
    (f(px), f(py))
}

/// Offset of the arrow's nose from its centre for a facing angle (the sim's
/// `rot` is `atan2(dy, dx)` in world space, which maps straight onto the map).
pub fn nose_offset(rot: f32, len: f32) -> (f32, f32) {
    (rot.cos() * len, rot.sin() * len)
}

/// Map colour for a terrain sample: water, then low green to high rock.
pub fn terrain_rgb(height: f32, lo: f32, hi: f32, water: Option<f32>) -> [u8; 3] {
    if water.is_some_and(|w| height < w) {
        return [38, 70, 110];
    }
    let t = if hi > lo { ((height - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.5 };
    let lerp = |a: f32, b: f32, t: f32| (a + (b - a) * t) as u8;
    if t < 0.6 {
        let k = t / 0.6;
        [lerp(52.0, 112.0, k), lerp(78.0, 104.0, k), lerp(44.0, 62.0, k)]
    } else {
        let k = (t - 0.6) / 0.4;
        [lerp(112.0, 190.0, k), lerp(104.0, 184.0, k), lerp(62.0, 176.0, k)]
    }
}

/// Render the act's heightfield to a small image.
pub fn terrain_image(act: Act) -> Image {
    let n = TEX as usize;
    let mut heights = vec![0.0f32; n * n];
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for j in 0..n {
        for i in 0..n {
            let (x, y) = unproject(i as f32 + 0.5, j as f32 + 0.5, TEX as f32);
            let h = terrain_height(act, x, y);
            heights[j * n + i] = h;
            lo = lo.min(h);
            hi = hi.max(h);
        }
    }
    let water = water_level(act);
    let mut data = Vec::with_capacity(n * n * 4);
    for h in heights {
        let [r, g, b] = terrain_rgb(h, lo, hi, water);
        data.extend_from_slice(&[r, g, b, 255]);
    }
    Image::new(
        Extent3d { width: TEX, height: TEX, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// What a quest pin says.
#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub x: f32,
    pub y: f32,
    /// True: ready to hand in here ("?"). False: objective here ("!").
    pub ready: bool,
    pub label: String,
}

/// Where the authored quest givers stand, relative to the act's inn.
pub fn giver_pos(giver: &str) -> (f32, f32) {
    match giver {
        "elder" => (90.0, 0.0),
        "wanderer" => (-70.0, 110.0),
        "seer" => (260.0, -170.0),
        "jabal" => (0.0, -220.0),
        "sentinel" => (simw::GATE_OF_EDEN.x + 60.0, simw::GATE_OF_EDEN.y - 40.0),
        _ => (0.0, 0.0),
    }
}

/// Pins for the active quests that point somewhere in `act`.
pub fn quest_pins(sheet: &CharacterSheet, act: Act) -> Vec<Pin> {
    let mut out = Vec::new();
    for id in sheet.quests.keys() {
        let Some(def) = quests::quest(id) else { continue };
        let ready = questlog::complete(def, sheet);
        let title = questgen::title(id);
        let anchor_pin = |key: &str, ready: bool| -> Option<Pin> {
            if key == "inn" {
                return (def.act == act).then(|| Pin { x: 0.0, y: 0.0, ready, label: title.clone() });
            }
            questgen::anchor_by_key(key).filter(|a| a.act == act).map(|a| Pin { x: a.pos.x, y: a.pos.y, ready, label: title.clone() })
        };
        match (questgen::gen_quest(id), def.objective) {
            (Some(_), Objective::Deliver { to, .. }) => out.extend(anchor_pin(to, true)),
            (Some(_), Objective::Explore { place }) if !ready => out.extend(anchor_pin(place, false)),
            (Some(g), _) => out.extend(anchor_pin(questgen::anchors()[g.anchor].key, ready)),
            (None, _) if def.act == act => {
                let (x, y) = giver_pos(def.giver);
                // Authored objectives roam the act; the pin marks who to
                // return to, and only once there is something to hand in.
                if ready {
                    out.push(Pin { x, y, ready, label: title.clone() });
                }
            }
            _ => {}
        }
    }
    out
}

#[derive(Component)]
pub(crate) enum MapMark {
    Player,
    Nose,
}

fn act_title(act: Act) -> &'static str {
    match act {
        Act::Eden => "Eden and Nod",
        Act::Hermon => "The Hermon Range",
        Act::Nephilim => "The Nephilim Wastes",
        Act::Enoch => "The Sprawl of Enoch",
        Act::Flood => "The Abyssal Basins",
    }
}

/// A square marker centred on a world position, with a hover name.
fn dot(m: &mut ChildBuilder, x: f32, y: f32, size: f32, color: Color, round: bool, tip: String) {
    let (px, py) = project(x, y, MAP_PX);
    let mut e = m.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(px - size / 2.0),
            top: Val::Px(py - size / 2.0),
            width: Val::Px(size),
            height: Val::Px(size),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(color),
        BorderColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
        Interaction::default(),
        Blocks,
        Tip(tip),
    ));
    if round {
        e.insert(BorderRadius::MAX);
    }
}

/// A text glyph centred on a world position.
fn glyph(m: &mut ChildBuilder, x: f32, y: f32, s: &str, size: f32, color: Color, tip: String) {
    let (px, py) = project(x, y, MAP_PX);
    m.spawn((
        Text::new(s),
        TextFont { font_size: size, ..default() },
        TextColor(color),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(px - size * 0.3),
            top: Val::Px(py - size * 0.55),
            ..default()
        },
        Interaction::default(),
        Blocks,
        Tip(tip),
    ));
}

fn label(m: &mut ChildBuilder, x: f32, y: f32, s: &str, color: Color) {
    let (px, py) = project(x, y, MAP_PX);
    m.spawn((
        Text::new(s),
        TextFont { font_size: 10.0, ..default() },
        TextColor(color),
        TextLayout::new_with_no_wrap(),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px((px + 7.0).min(MAP_PX - 6.0 * s.len() as f32).max(0.0)),
            top: Val::Px(py - 6.0),
            ..default()
        },
    ));
}

pub(crate) fn build(p: &mut ChildBuilder, ctx: &Ctx) {
    let s = ctx.sheet;
    let act = ctx.act;
    let (lo, hi) = antediluvia_sim::mobs::act_band(act);
    text(p, format!("{} - levels {lo}-{hi}", act_title(act)), 12.0, TEXT_DIM);
    let mut map = p.spawn((
        Node { width: Val::Px(MAP_PX), height: Val::Px(MAP_PX), overflow: Overflow::clip(), ..default() },
        BackgroundColor(Color::srgb(0.20, 0.26, 0.18)),
    ));
    map.with_children(|m| {
        if let Some(bg) = ctx.map_bg.clone() {
            m.spawn((
                ImageNode::new(bg),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(MAP_PX),
                    height: Val::Px(MAP_PX),
                    ..default()
                },
            ));
        }
        // Enemy capitals: a translucent disc the size of the PvP ground.
        for z in simw::pvp_zones(act) {
            let (px, py) = project(z.x, z.y, MAP_PX);
            let r = z.radius / (2.0 * WORLD_BOUNDS) * MAP_PX;
            let own = s.faction.as_deref() == Some(z.faction.as_str());
            let c = if own { Color::srgba(0.3, 0.7, 0.3, 0.28) } else { Color::srgba(0.85, 0.2, 0.15, 0.30) };
            m.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(px - r),
                    top: Val::Px(py - r),
                    width: Val::Px(r * 2.0),
                    height: Val::Px(r * 2.0),
                    ..default()
                },
                BackgroundColor(c),
                BorderRadius::MAX,
                Interaction::default(),
                Tip(format!("{} - {} capital{}", z.name, super::pretty(&z.faction), if own { "" } else { " (flags you for PvP)" })),
            ));
        }
        // Caves and points of interest.
        for c in caves::caves_for_act(act) {
            let known = s.discovered.iter().any(|d| *d == c.name);
            let tip = if known { format!("{} (cave)", c.name) } else { "Unexplored cave".to_string() };
            dot(m, c.x, c.y, 8.0, if known { Color::srgb(0.55, 0.40, 0.28) } else { Color::srgb(0.25, 0.20, 0.16) }, false, tip);
        }
        for poi in pois::pois_for_act(act) {
            let known = s.discovered.iter().any(|d| *d == poi.name);
            let tip = if known { poi.name.clone() } else { "Undiscovered place".to_string() };
            dot(m, poi.x, poi.y, 7.0, if known { Color::srgb(0.90, 0.82, 0.60) } else { Color::srgb(0.30, 0.30, 0.30) }, true, tip);
        }
        // Graveyards.
        for (name, pos) in travel::graveyards(act) {
            glyph(m, pos.x, pos.y, "+", 16.0, Color::srgb(0.88, 0.88, 0.95), format!("Graveyard: {name}"));
        }
        // Flight masters (the inn's is drawn as the inn).
        for n in travel::flight_nodes().iter().filter(|n| n.act == act && !n.inn) {
            let known = travel::node_known(s, n);
            let c = if known { Color::srgb(0.45, 0.85, 1.0) } else { Color::srgb(0.30, 0.42, 0.50) };
            dot(m, n.pos.x, n.pos.y, 11.0, c, false, format!("Flight master: {}{}", n.name, if known { "" } else { " (not yet discovered)" }));
            if known {
                label(m, n.pos.x, n.pos.y, n.name, c);
            }
        }
        dot(m, 0.0, 0.0, 13.0, GOLD, false, format!("{} - vendor, bank, flight master", travel::inn_name(act)));
        // Label below the marker so the player's arrow (usually near the
        // inn early on) does not sit on the text.
        label(m, -60.0, 190.0, travel::inn_name(act), GOLD);
        // Quest pins.
        for pin in quest_pins(s, act) {
            let (g, c) = if pin.ready { ("?", Color::srgb(1.0, 0.85, 0.1)) } else { ("!", Color::srgb(1.0, 0.85, 0.1)) };
            // Lifted off the spot (like a pin) so the player's arrow or a
            // place marker underneath stays visible.
            let lift = 14.0 / MAP_PX * 2.0 * WORLD_BOUNDS;
            glyph(m, pin.x, pin.y - lift, g, 22.0, c, format!("{}{}", pin.label, if pin.ready { " - hand in here" } else { "" }));
        }
        // The player: a dot and a nose showing the facing.
        let (x, y, rot) = ctx.st.me.unwrap_or((s.x, s.y, 0.0));
        let (px, py) = project(x, y, MAP_PX);
        let (nx, ny) = nose_offset(rot, 8.0);
        let mark = |m: &mut ChildBuilder, cx: f32, cy: f32, size: f32, kind: MapMark| {
            m.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(cx - size / 2.0),
                    top: Val::Px(cy - size / 2.0),
                    width: Val::Px(size),
                    height: Val::Px(size),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.25, 1.0, 0.35)),
                BorderColor(Color::BLACK),
                BorderRadius::MAX,
                kind,
            ));
        };
        mark(m, px + nx, py + ny, 6.0, MapMark::Nose);
        mark(m, px, py, 11.0, MapMark::Player);
    });
    text(
        p,
        format!(
            "Green you | Gold inn | Tan place ({} found) | Brown cave | Blue flight master\n+ graveyard | ! and ? quests | Red disc enemy capital | hover for names",
            s.discovered.len()
        ),
        10.0,
        TEXT_DIM,
    );
}

/// Keep the player's arrow current while the map is open.
pub(crate) fn update_player(st: Res<super::WinState>, mut q: Query<(&MapMark, &mut Node)>) {
    let Some((x, y, rot)) = st.me else { return };
    let (px, py) = project(x, y, MAP_PX);
    let (nx, ny) = nose_offset(rot, 8.0);
    for (mark, mut node) in &mut q {
        let (cx, cy, size) = match mark {
            MapMark::Player => (px, py, 11.0),
            MapMark::Nose => (px + nx, py + ny, 6.0),
        };
        let (l, t) = (Val::Px(cx - size / 2.0), Val::Px(cy - size / 2.0));
        if node.left != l || node.top != t {
            node.left = l;
            node.top = t;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_maps_the_world_square_onto_the_picture() {
        assert_eq!(project(0.0, 0.0, 600.0), (300.0, 300.0));
        assert_eq!(project(-WORLD_BOUNDS, -WORLD_BOUNDS, 600.0), (0.0, 0.0));
        assert_eq!(project(WORLD_BOUNDS, WORLD_BOUNDS, 600.0), (600.0, 600.0));
        // North (-y) is up, east (+x) is right.
        assert!(project(0.0, -1000.0, 600.0).1 < 300.0);
        assert!(project(1000.0, 0.0, 600.0).0 > 300.0);
        // Out-of-bounds positions clamp to the edge.
        assert_eq!(project(99_999.0, -99_999.0, 600.0), (600.0, 0.0));
    }

    #[test]
    fn unproject_inverts_project() {
        for (x, y) in [(0.0, 0.0), (1234.0, -2500.0), (-3000.0, 3599.0)] {
            let (px, py) = project(x, y, MAP_PX);
            let (bx, by) = unproject(px, py, MAP_PX);
            assert!((bx - x).abs() < 0.01 && (by - y).abs() < 0.01, "{x},{y} -> {bx},{by}");
        }
    }

    #[test]
    fn nose_points_along_the_facing() {
        let (x, y) = nose_offset(0.0, 8.0);
        assert!((x - 8.0).abs() < 1e-4 && y.abs() < 1e-4, "rot 0 faces +x (east)");
        let (x, y) = nose_offset(-std::f32::consts::FRAC_PI_2, 8.0);
        assert!(x.abs() < 1e-4 && (y + 8.0).abs() < 1e-4, "rot -90deg faces -y (north, up)");
    }

    #[test]
    fn terrain_colours() {
        assert_eq!(terrain_rgb(-30.0, -40.0, 100.0, Some(-16.0)), [38, 70, 110], "below the water table is water");
        let low = terrain_rgb(-10.0, -40.0, 100.0, Some(-16.0));
        let high = terrain_rgb(100.0, -40.0, 100.0, None);
        assert!(low[1] > low[0] && low[1] > low[2], "lowland is green: {low:?}");
        assert!(high.iter().all(|c| *c > 150), "peaks are pale: {high:?}");
        assert_eq!(terrain_rgb(5.0, 5.0, 5.0, None).len(), 3); // flat map does not divide by zero
    }

    #[test]
    fn terrain_picture_has_the_right_size() {
        let img = terrain_image(Act::Eden);
        assert_eq!(img.data.len(), (TEX * TEX * 4) as usize);
    }

    #[test]
    fn pins_point_at_board_quests_in_this_act_only() {
        let mut s = antediluvia_sim::world::new_character("Tester");
        let g = questgen::generated().iter().find(|g| matches!(g.def.objective, Objective::Kill { .. })).unwrap();
        let a = &questgen::anchors()[g.anchor];
        s.quests.insert(g.def.id.into(), 0);
        let pins = quest_pins(&s, a.act);
        assert_eq!(pins.len(), 1);
        assert!(!pins[0].ready);
        assert_eq!((pins[0].x, pins[0].y), (a.pos.x, a.pos.y));
        let other = Act::ALL.into_iter().find(|x| *x != a.act).unwrap();
        assert!(quest_pins(&s, other).is_empty());
        // Finished: the same pin turns into a hand-in.
        s.quests.insert(g.def.id.into(), g.def.objective.count());
        assert!(quest_pins(&s, a.act)[0].ready);
    }
}
