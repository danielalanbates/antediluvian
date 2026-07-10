//! CHUNK_09 — visible equipment.
//!
//! The KayKit Adventurer rigs ship every held-item mesh (swords, axes, staff,
//! shields) parented to their hand slots; glTF has no visibility flag so they
//! all render by default. This module mirrors each player's equipped
//! weapon/chest off the snapshot onto a `Loadout` component and then toggles
//! `Visibility` on the rig's item nodes so only the equipped weapon shows.

use bevy::prelude::*;

/// Wire loadout for a player character, mirrored onto its scene root.
#[derive(Component, Default, Clone, PartialEq)]
pub struct Loadout {
    pub weapon: Option<String>,
    pub chest: Option<String>,
}

/// The loadout most recently applied to this rig's nodes (None = never).
#[derive(Component, Default)]
pub struct LoadoutApplied(Option<Loadout>);

/// Shared chest-armor override material (hide_vest tint).
#[derive(Resource)]
pub struct EquipAssets {
    vest: Handle<StandardMaterial>,
}

pub fn init_equip_assets(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(EquipAssets {
        vest: materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.30, 0.18), // tanned hide
            perceptual_roughness: 0.9,
            ..default()
        }),
    });
}

/// Every held-item node name across the Adventurer rigs. Everything here is
/// hidden unless it is the equipped weapon's node.
const ITEM_NODES: &[&str] = &[
    "1H_Sword",
    "1H_Sword_Offhand",
    "2H_Sword",
    "1H_Axe",
    "1H_Axe_Offhand",
    "2H_Axe",
    "2H_Staff",
    "Badge_Shield",
    "Rectangle_Shield",
    "Round_Shield",
    "Spike_Shield",
    "Barbarian_Round_Shield",
    "1H_Crossbow",
    "2H_Crossbow",
    "Knife",
    "Knife_Offhand",
    "Throwable",
    "Spellbook",
    "Spellbook_open",
];

/// Item id → rig node to show. Rigs missing the node just show bare hands.
fn weapon_node(item: &str) -> Option<&'static str> {
    match item {
        "bronze_sword" => Some("1H_Sword"),
        "stone_axe" => Some("1H_Axe"),
        "oak_staff" => Some("2H_Staff"),
        _ => None,
    }
}

/// Walk a character's scene and set item-node visibility to match `Loadout`.
/// Scenes load async, so this retries every frame until the rig's nodes exist,
/// then re-runs only when the loadout changes.
pub fn apply_loadouts(
    mut commands: Commands,
    mut roots: Query<(Entity, &Loadout, Option<&mut LoadoutApplied>)>,
    children_q: Query<&Children>,
    names: Query<&Name>,
    meshes: Query<&Mesh3d>,
    equip: Res<EquipAssets>,
) {
    for (root, loadout, applied) in &mut roots {
        if let Some(a) = &applied {
            if a.0.as_ref() == Some(loadout) {
                continue;
            }
        }
        let show = loadout.weapon.as_deref().and_then(weapon_node);
        let mut touched = false;
        // Breadth-first over the whole character hierarchy.
        let mut stack = vec![root];
        while let Some(ent) = stack.pop() {
            if let Ok(kids) = children_q.get(ent) {
                stack.extend(kids.iter().copied());
            }
            let Ok(name) = names.get(ent) else { continue };
            if ITEM_NODES.contains(&name.as_str()) {
                let vis = if Some(name.as_str()) == show {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                commands.entity(ent).insert(vis);
                touched = true;
            } else if loadout.chest.as_deref() == Some("hide_vest") && name.as_str().ends_with("_Body")
            {
                // Chest armor v1: tint the torso mesh (children of the *_Body
                // node carry the Mesh3d + material).
                if let Ok(kids) = children_q.get(ent) {
                    for kid in kids.iter() {
                        if meshes.get(*kid).is_ok() {
                            commands.entity(*kid).insert(MeshMaterial3d(equip.vest.clone()));
                        }
                    }
                }
            }
        }
        // Only record success once the rig's nodes were actually found —
        // before the GLB scene spawns there is nothing to toggle yet.
        if touched {
            match applied {
                Some(mut a) => a.0 = Some(loadout.clone()),
                None => {
                    commands.entity(root).insert(LoadoutApplied(Some(loadout.clone())));
                }
            }
        }
    }
}
