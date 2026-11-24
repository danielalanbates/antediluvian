// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Crowd budget (B11): what a player in a crowd of 1,000 is allowed to cost.
//!
//! With binary delta snapshots the server tells one client about up to 1,200
//! players. The frame budget is kept by drawing almost all of them as the
//! shared block figure: one mesh, one of 16 shared skin materials, so Bevy
//! draws the whole far crowd in a handful of instanced batches. Anything
//! that is one entity or one UI node *per player* is reserved for the few
//! drawn as full rigs (`MAX_PLAYER_RIGS`, nearest first, inside
//! `RIG_LOD_RADIUS`):
//!
//! | per player            | full rig (near, at most 40) | block figure |
//! |-----------------------|-----------------------------|--------------|
//! | skinned mesh + clips  | yes                         | no           |
//! | health bar (3 quads)  | yes                         | no           |
//! | name plate (UI node)  | yes                         | no           |
//! | motion easing         | brisk (10 Hz updates)       | slow glide   |

use antediluvia_protocol::EntityKind;
use bevy::prelude::*;

/// Does this entity get the floating health bar? Mobs always; players only
/// when drawn as a full rig.
pub fn wants_health_bar(kind: EntityKind, cheap: bool) -> bool {
    match kind {
        EntityKind::Player => !cheap,
        EntityKind::Enemy | EntityKind::Wildlife => true,
        EntityKind::Resource | EntityKind::Npc => false,
    }
}

/// Does this entity get a screen-space name? NPCs always (gold); other
/// players only when drawn as a full rig, so the count is bounded by the rig
/// budget; never the local player (the unit frame names them).
pub fn name_plate(kind: EntityKind, is_me: bool, cheap: bool) -> Option<Color> {
    match kind {
        EntityKind::Npc => Some(Color::srgb(1.0, 0.84, 0.25)),
        EntityKind::Player if !is_me && !cheap => Some(Color::srgb(0.72, 0.86, 1.0)),
        _ => None,
    }
}

/// Easing rate (1/s) toward the last position the server sent. Far players
/// are block figures updated at 2.5 Hz: easing as briskly as a 10 Hz rig
/// would make them dart and stop, so they glide over the whole gap instead.
pub fn ease_rate(cheap: bool) -> f32 {
    if cheap {
        5.0
    } else {
        14.0
    }
}

/// Every 10 s, next to the fps line: how many other players are on screen
/// and how they are drawn. This is the measurement behind "1,000 players
/// visible" in a windowless run, where nobody can count the figures.
pub fn log_crowd(time: Res<Time>, map: Res<crate::EntityMap>, mut acc: Local<f32>) {
    *acc += time.delta_secs();
    if *acc < 10.0 {
        return;
    }
    *acc = 0.0;
    let (mut rigs, mut blocks) = (0usize, 0usize);
    for m in map.0.values() {
        match m.lod_cheap {
            Some(false) => rigs += 1,
            Some(true) => blocks += 1,
            None => {}
        }
    }
    if rigs + blocks > 0 {
        info!(
            "crowd: {} other players drawn ({rigs} rigs, {blocks} block figures), {} entities in all",
            rigs + blocks,
            map.0.len()
        );
    }
}

/// Squared ground distance of a snapshot entity from (x, y).
pub fn dist2(e: &antediluvia_protocol::EntityState, x: f32, y: f32) -> f32 {
    let (dx, dy) = (e.x - x, e.y - y);
    dx * dx + dy * dy
}

/// `ANTEDILUVIA_AUTOCMD="cam <distance> [pitch]"`: pull the orbit camera
/// back (and tilt it) for a windowless crowd shot. Distance is allowed past
/// the mouse-wheel limit, up to 2,400 units; pitch is radians above the
/// horizon. Returns (distance, pitch if given).
pub fn parse_cam(arg: &str) -> Option<(f32, Option<f32>)> {
    let mut it = arg.split_whitespace();
    let dist: f32 = it.next()?.parse().ok()?;
    if !dist.is_finite() {
        return None;
    }
    let pitch = it.next().and_then(|p| p.parse::<f32>().ok()).filter(|p| p.is_finite());
    Some((dist.clamp(140.0, 2400.0), pitch.map(|p| p.clamp(0.08, 1.35))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cam_command_parses_and_clamps() {
        assert_eq!(parse_cam("900"), Some((900.0, None)));
        assert_eq!(parse_cam("1500 0.9"), Some((1500.0, Some(0.9))));
        assert_eq!(parse_cam("99999 9"), Some((2400.0, Some(1.35))));
        assert_eq!(parse_cam("1 0"), Some((140.0, Some(0.08))));
        assert_eq!(parse_cam("far"), None);
        assert_eq!(parse_cam("NaN"), None);
        assert_eq!(parse_cam(""), None);
    }

    #[test]
    fn block_figures_carry_no_bar_and_no_name() {
        assert!(!wants_health_bar(EntityKind::Player, true));
        assert!(name_plate(EntityKind::Player, false, true).is_none());
        // 1,000 block figures therefore add no bar quads and no UI nodes.
    }

    #[test]
    fn rigs_mobs_and_npcs_keep_theirs() {
        assert!(wants_health_bar(EntityKind::Player, false));
        assert!(wants_health_bar(EntityKind::Enemy, false));
        assert!(wants_health_bar(EntityKind::Wildlife, false));
        assert!(!wants_health_bar(EntityKind::Npc, false));
        assert!(!wants_health_bar(EntityKind::Resource, false));
        assert!(name_plate(EntityKind::Player, false, false).is_some());
        assert!(name_plate(EntityKind::Npc, false, false).is_some());
        assert!(name_plate(EntityKind::Enemy, false, false).is_none());
    }

    #[test]
    fn the_local_player_has_no_name_plate() {
        assert!(name_plate(EntityKind::Player, true, false).is_none());
    }

    #[test]
    fn far_figures_ease_slower_than_rigs() {
        assert!(ease_rate(true) < ease_rate(false));
        // After one 0.4 s far-tier gap a block figure has covered most of
        // the distance, so it never visibly lags a whole update behind.
        let covered = 1.0 - (-ease_rate(true) * 0.4f32).exp();
        assert!(covered > 0.8, "{covered}");
    }
}
