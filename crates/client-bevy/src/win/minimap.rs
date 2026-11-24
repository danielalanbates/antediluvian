// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Minimap: always on, top-right. North-up, centred on the player. Shows
//! nearby creatures and players from the latest snapshot, the inn, boards,
//! caves, graveyards and flight masters, with the place name and
//! coordinates underneath.

use antediluvia_protocol::{Act, EntityKind};
use antediluvia_sim::{questgen, travel};
use bevy::prelude::*;

use super::{WinState, BORDER, GOLD, PANEL, TEXT};
use crate::Session;

/// Side of the minimap, in pixels.
pub const SIZE: f32 = 164.0;
/// World units from the centre to the rim.
pub const RANGE: f32 = 950.0;
const DOTS: usize = 56;

/// Offset from the minimap centre for something `dx, dy` world units from
/// the player, or `None` when it is outside the circle.
pub fn mini_project(dx: f32, dy: f32, range: f32, half: f32) -> Option<(f32, f32)> {
    if range <= 0.0 || dx.hypot(dy) > range {
        return None;
    }
    Some((dx / range * half, dy / range * half))
}

/// Like [`mini_project`] but pins far things to the rim (used for the inn,
/// so there is always a way home on the minimap).
pub fn mini_pin(dx: f32, dy: f32, range: f32, half: f32) -> (f32, f32) {
    let d = dx.hypot(dy);
    if d <= range || d == 0.0 {
        (dx / range * half, dy / range * half)
    } else {
        (dx / d * half, dy / d * half)
    }
}

/// Dot colour and size for an entity kind.
pub fn blip_style(kind: EntityKind, named_npc: bool) -> ([f32; 3], f32) {
    match kind {
        EntityKind::Enemy => ([0.95, 0.25, 0.20], 5.0),
        EntityKind::Wildlife => ([0.95, 0.80, 0.30], 4.0),
        EntityKind::Player => ([0.35, 0.65, 1.0], 6.0),
        EntityKind::Npc if named_npc => ([1.0, 0.86, 0.15], 7.0),
        EntityKind::Npc => ([0.40, 0.90, 0.45], 6.0),
        EntityKind::Resource => ([0.62, 0.62, 0.62], 3.0),
    }
}

/// Static places near a position: (x, y, colour, size).
pub fn landmarks(act: Act, x: f32, y: f32, range: f32) -> Vec<(f32, f32, [f32; 3], f32)> {
    let mut out = Vec::new();
    let near = |px: f32, py: f32| (px - x).hypot(py - y) <= range;
    for a in questgen::anchors().iter().filter(|a| a.act == act && near(a.pos.x, a.pos.y)) {
        out.push((a.pos.x, a.pos.y, if a.cave { [0.60, 0.45, 0.30] } else { [0.90, 0.82, 0.60] }, 6.0));
    }
    for (_, g) in travel::graveyards(act).iter().skip(1).filter(|(_, g)| near(g.x, g.y)) {
        out.push((g.x, g.y, [0.88, 0.88, 0.95], 5.0));
    }
    for n in travel::flight_nodes().iter().filter(|n| n.act == act && !n.inn && near(n.pos.x, n.pos.y)) {
        out.push((n.pos.x, n.pos.y, [0.45, 0.85, 1.0], 7.0));
    }
    out
}

#[derive(Component)]
pub(crate) struct MiniRoot;
#[derive(Component)]
pub(crate) struct MiniDot(usize);
#[derive(Component)]
pub(crate) struct MiniLabel;

/// Indices of the fixed dots at the end of the pool.
const NOSE: usize = DOTS;
const ME: usize = DOTS + 1;

pub(crate) fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(14.0),
                right: Val::Px(14.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.0),
                ..default()
            },
            Visibility::Hidden,
            MiniRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(SIZE),
                    height: Val::Px(SIZE),
                    border: UiRect::all(Val::Px(2.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.10, 0.14, 0.10, 0.88)),
                BorderColor(BORDER),
                BorderRadius::all(Val::Px(10.0)),
            ))
            .with_children(|map| {
                for i in 0..DOTS + 2 {
                    let (size, color) = match i {
                        ME => (9.0, Color::srgb(0.25, 1.0, 0.35)),
                        NOSE => (5.0, Color::srgb(0.25, 1.0, 0.35)),
                        _ => (4.0, Color::WHITE),
                    };
                    map.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(SIZE / 2.0),
                            top: Val::Px(SIZE / 2.0),
                            width: Val::Px(size),
                            height: Val::Px(size),
                            ..default()
                        },
                        BackgroundColor(color),
                        BorderRadius::MAX,
                        Visibility::Hidden,
                        MiniDot(i),
                    ));
                }
                // "N" at the top rim.
                map.spawn((
                    Text::new("N"),
                    TextFont { font_size: 10.0, ..default() },
                    TextColor(GOLD),
                    Node { position_type: PositionType::Absolute, left: Val::Px(SIZE / 2.0 - 6.0), top: Val::Px(1.0), ..default() },
                ));
            });
            root.spawn((
                Node { padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)), max_width: Val::Px(SIZE + 30.0), ..default() },
                BackgroundColor(PANEL),
            ))
            .with_children(|l| {
                l.spawn((
                    Text::new(""),
                    TextFont { font_size: 11.0, ..default() },
                    TextColor(TEXT),
                    TextLayout::new_with_justify(JustifyText::Center),
                    MiniLabel,
                ));
            });
        });
}

#[allow(clippy::type_complexity)]
pub(crate) fn update(
    session: Res<Session>,
    st: Res<WinState>,
    mut root: Query<&mut Visibility, (With<MiniRoot>, Without<MiniDot>)>,
    mut dots: Query<(&MiniDot, &mut Node, &mut BackgroundColor, &mut Visibility), Without<MiniRoot>>,
    mut label: Query<&mut Text, With<MiniLabel>>,
) {
    let Ok(mut vis) = root.get_single_mut() else { return };
    let me = st.me.filter(|_| session.sheet.is_some() && session.builder.is_none());
    let Some((mx, my, rot)) = me else {
        if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
        return;
    };
    if *vis != Visibility::Inherited {
        *vis = Visibility::Inherited;
    }
    // Inner half-size: the border eats two pixels on each side.
    let half = SIZE / 2.0 - 2.0;
    // Gather what to draw: (offset x, offset y, colour, size).
    let mut draw: Vec<(f32, f32, [f32; 3], f32)> = Vec::with_capacity(DOTS);
    let (ix, iy) = mini_pin(-mx, -my, RANGE, half - 5.0);
    draw.push((ix, iy, [0.98, 0.90, 0.55], 9.0));
    for (x, y, c, s) in landmarks(session.act, mx, my, RANGE) {
        if let Some((ox, oy)) = mini_project(x - mx, y - my, RANGE, half) {
            draw.push((ox, oy, c, s));
        }
    }
    // Nearest blips first, so a crowd never hides what is next to you.
    let mut blips: Vec<_> = st.blips.iter().filter(|b| b.alive).map(|b| ((b.x - mx).hypot(b.y - my), b)).collect();
    blips.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, b) in blips {
        if draw.len() >= DOTS {
            break;
        }
        if let Some((ox, oy)) = mini_project(b.x - mx, b.y - my, RANGE, half) {
            let (c, s) = blip_style(b.kind, b.name.as_deref().is_some_and(super::markers::is_giver));
            draw.push((ox, oy, c, s));
        }
    }
    let (nx, ny) = super::worldmap::nose_offset(rot, 7.0);
    for (dot, mut node, mut bg, mut dvis) in &mut dots {
        let (ox, oy, color, size) = match dot.0 {
            ME => (0.0, 0.0, None, 9.0),
            NOSE => (nx, ny, None, 5.0),
            i => match draw.get(i) {
                Some((x, y, c, s)) => (*x, *y, Some(*c), *s),
                None => {
                    if *dvis != Visibility::Hidden {
                        *dvis = Visibility::Hidden;
                    }
                    continue;
                }
            },
        };
        let (l, t) = (Val::Px(half + ox - size / 2.0), Val::Px(half + oy - size / 2.0));
        if node.left != l || node.top != t {
            node.left = l;
            node.top = t;
        }
        if node.width != Val::Px(size) {
            node.width = Val::Px(size);
            node.height = Val::Px(size);
        }
        if let Some([r, g, b]) = color {
            let c = Color::srgb(r, g, b);
            if bg.0 != c {
                bg.0 = c;
            }
        }
        if *dvis != Visibility::Inherited {
            *dvis = Visibility::Inherited;
        }
    }
    if let Ok(mut t) = label.get_single_mut() {
        let faction = session.sheet.as_ref().and_then(|s| s.faction.as_deref());
        let region = travel::region_at(session.act, glam::Vec2::new(mx, my), faction);
        let line = format!("{}\n{:.0}, {:.0}", region.name, mx, my);
        if **t != line {
            **t = line;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_is_centred_and_clips_to_the_circle() {
        assert_eq!(mini_project(0.0, 0.0, 1000.0, 80.0), Some((0.0, 0.0)));
        assert_eq!(mini_project(500.0, -500.0, 1000.0, 80.0), Some((40.0, -40.0)));
        assert_eq!(mini_project(1000.0, 0.0, 1000.0, 80.0), Some((80.0, 0.0)));
        assert_eq!(mini_project(800.0, 800.0, 1000.0, 80.0), None, "corner is outside the circle");
        assert_eq!(mini_project(1.0, 1.0, 0.0, 80.0), None);
    }

    #[test]
    fn far_inn_is_pinned_to_the_rim() {
        let (x, y) = mini_pin(-3000.0, 0.0, 1000.0, 80.0);
        assert!((x + 80.0).abs() < 1e-3 && y.abs() < 1e-3);
        assert_eq!(mini_pin(100.0, 0.0, 1000.0, 80.0), (8.0, 0.0));
        assert_eq!(mini_pin(0.0, 0.0, 1000.0, 80.0), (0.0, 0.0));
    }

    #[test]
    fn hostile_and_friendly_blips_differ() {
        assert_ne!(blip_style(EntityKind::Enemy, false).0, blip_style(EntityKind::Player, false).0);
        assert_ne!(blip_style(EntityKind::Npc, true).0, blip_style(EntityKind::Npc, false).0);
    }

    #[test]
    fn landmarks_are_local() {
        for act in Act::ALL {
            for (x, y, _, _) in landmarks(act, 0.0, 0.0, RANGE) {
                assert!(x.hypot(y) <= RANGE);
            }
            assert!(landmarks(act, 0.0, 0.0, 10_000.0).len() >= 4 + 4, "{act:?}: graveyards and flight masters at least");
        }
    }
}
