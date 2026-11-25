// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Wire-cost guard for the appearance fields: every player in a snapshot
//! carries its look, ten times a second, to up to 150 neighbours' clients.

use crate::world::{new_character, World};
use antediluvia_protocol::{Act, Look};

fn player_fragment(look: u64) -> String {
    let mut w = World::new(5);
    let mut s = new_character("Sizecheck");
    s.look = look;
    let id = w.spawn_player(1, s);
    let (_, frags) = w.zone_serialized(Act::Eden);
    let needle = format!("\"id\":{id},");
    frags.into_iter().map(|(_, j)| j).find(|j| j.contains(&needle)).expect("player fragment")
}

/// The look is one number: at most `,"look":` plus 16 digits per player.
#[test]
fn look_costs_at_most_24_bytes_per_player_in_a_snapshot() {
    let widest = (0..4000u64).map(|s| Look::random(s).pack()).max().unwrap();
    assert!(widest < 1u64 << 53, "must stay exact as a JSON number in a browser");
    let plain = player_fragment(0);
    let full = player_fragment(widest);
    assert!(plain.contains("\"look\":0"), "{plain}");
    let field = full.len() - plain.len() + "\"look\":0,".len();
    assert!(field <= 24, "look field costs {field} bytes");
    // Whole player entity, for the record in docs/beta/HANDOFF_2026-11-23.md.
    // A cap here catches a field that starts serializing by accident.
    assert!(full.len() <= 330, "player entity grew to {} bytes: {full}", full.len());
    // Mobs carry neither look nor appearance.
    let mut w = World::new(5);
    let _ = w.spawn_player(1, new_character("Sizecheck"));
    let (_, frags) = w.zone_serialized(Act::Eden);
    let mob = frags.iter().map(|(_, j)| j).find(|j| j.contains("\"kind\":\"enemy\"")).expect("a mob");
    assert!(!mob.contains("look") && !mob.contains("appearance"), "{mob}");
    eprintln!("snapshot bytes: player {} (look field {field}), mob {}", full.len(), mob.len());
}
