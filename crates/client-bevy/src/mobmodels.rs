// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Which model a bestiary species renders with, as one pure function of its
//! tag, so the "hundreds of distinct mob models" claim can be counted over
//! the real `mobs.json` instead of over made-up names.
//!
//! A species is either a generated body (a `creaturegen` plan, its seeded
//! proportions and its `beastgrafts`) or one of the downloaded animal rigs
//! reshaped per species (`variety`: body-plan stretch plus grafted horns,
//! ridges, sails and plates on the rig's bones).

use bevy::prelude::*;

use crate::{beastgrafts, creaturegen, formations, variety};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MobModel {
    /// A generated body: plan, graft loadout, coarse proportions.
    Generated { plan: u32, grafts: u32, shape: [i32; 3] },
    /// An animal rig: file, body-plan family, coarse stretch, part loadout.
    Rig { file: &'static str, plan: u32, stretch: [i32; 3], parts: u32 },
}

/// The generated-body plan for a species, or `None` when it rides a rig.
/// Mirrors `main.rs::proc_body_plan` for bestiary tags.
pub fn generated_plan(tag: &str) -> Option<u32> {
    match creaturegen::named_plan(tag) {
        Some(p) => Some(p),
        None if crate::beast_rig(tag).is_none() => Some(creaturegen::plan_for_species(tag)),
        None => None,
    }
}

/// Coarse "what does it look like" key. Two species with different keys
/// differ in silhouette (plan, a graft, or proportions a bin apart).
pub fn model_key(tag: &str) -> MobModel {
    match generated_plan(tag) {
        Some(plan) => {
            let seed = creaturegen::species_seed(tag);
            let mesh = creaturegen::creature_mesh_plan(seed, plan);
            MobModel::Generated { plan, grafts: beastgrafts::graft_key(seed, plan), shape: formations::silhouette(&mesh).1 }
        }
        None => {
            let (file, _, _) = crate::beast_rig(tag).expect("rig species");
            let (plan, stretch, parts) = variety::silhouette_key(tag);
            // Halve the stretch resolution: a twentieth of a unit is not a
            // difference anyone sees.
            MobModel::Rig { file, plan, stretch: [stretch[0] / 2, stretch[1] / 2, stretch[2] / 2], parts }
        }
    }
}

/// Exact geometry fingerprint of a species' model.
pub fn model_geometry(tag: &str) -> u64 {
    match generated_plan(tag) {
        Some(plan) => formations::geom_hash(&creaturegen::creature_mesh_plan(creaturegen::species_seed(tag), plan)),
        None => {
            let (file, _, _) = crate::beast_rig(tag).expect("rig species");
            let s = variety::species_stretch(tag);
            let mut h = file.bytes().fold(0xCBF2_9CE4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01B3));
            for c in [s.x, s.y, s.z] {
                h = (h ^ ((c * 1000.0) as i64 as u64)).wrapping_mul(0x100_0000_01B3);
            }
            // The grafted parts are generated from this seed alone.
            (h ^ variety::species_parts_seed(tag)).wrapping_mul(0x100_0000_01B3)
        }
    }
}

/// Art-review sheet of real bestiary species: the first N generated-body
/// species of `mobs.json`, in file order, one per cell.
pub fn spawn_species_sheet(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    let mat = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.85, ..default() });
    let (cols, rows) = (12usize, 7usize);
    let mut seen = std::collections::HashSet::new();
    let tags: Vec<&str> = antediluvia_sim::mobs::all_mobs()
        .iter()
        .map(|m| m.tag.as_str())
        .filter(|t| generated_plan(t).is_some() && seen.insert(model_key(t)))
        .take(cols * rows)
        .collect();
    const SPACING: f32 = 3.4;
    for (i, tag) in tags.iter().enumerate() {
        let plan = generated_plan(tag).unwrap_or(0);
        let (c, r) = (i % cols, i / cols);
        commands.spawn((
            Mesh3d(meshes.add(creaturegen::creature_mesh_plan(creaturegen::species_seed(tag), plan))),
            MeshMaterial3d(mat.clone()),
            Transform::from_xyz((c as f32 - (cols - 1) as f32 * 0.5) * SPACING, 0.0, (r as f32 - (rows - 1) as f32 * 0.5) * SPACING)
                .with_rotation(Quat::from_rotation_y(-0.9)),
        ));
    }
    let ground = meshes.add(Plane3d::default().mesh().size(300.0, 300.0));
    let gmat = materials.add(StandardMaterial { base_color: Color::srgb(0.30, 0.33, 0.26), perceptual_roughness: 1.0, ..default() });
    commands.spawn((Mesh3d(ground), MeshMaterial3d(gmat)));
    commands.spawn((
        DirectionalLight { illuminance: 14_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(20.0, 40.0, 14.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 22.0, 24.0).looking_at(Vec3::new(0.0, 0.0, 1.5), Vec3::Y)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// The headline claim, counted over the real bestiary: at least 300
    /// species of `mobs.json` render with visually distinct models.
    #[test]
    fn mobs_json_reaches_over_300_distinct_models() {
        let tags: HashSet<&str> = antediluvia_sim::mobs::all_mobs().iter().map(|m| m.tag.as_str()).collect();
        let mut keys = HashSet::new();
        let mut geoms = HashSet::new();
        let mut generated = HashSet::new();
        let mut per_plan: HashMap<u32, usize> = HashMap::new();
        let mut per_rig: HashMap<&str, usize> = HashMap::new();
        for t in &tags {
            let k = model_key(t);
            match &k {
                MobModel::Generated { plan, .. } => {
                    *per_plan.entry(*plan).or_default() += 1;
                    generated.insert(k.clone());
                }
                MobModel::Rig { file, .. } => *per_rig.entry(*file).or_default() += 1,
            }
            keys.insert(k);
            geoms.insert(model_geometry(t));
        }
        println!(
            "species {}  distinct silhouette keys {}  (generated bodies {})  distinct geometry {}\n generated per plan {:?}\n rig species {:?}",
            tags.len(),
            keys.len(),
            generated.len(),
            geoms.len(),
            per_plan,
            per_rig
        );
        assert!(keys.len() >= 300, "only {} distinct mob silhouettes in {} species", keys.len(), tags.len());
        assert!(geoms.len() >= 300, "only {} distinct mob geometries", geoms.len());
        // Generated bodies alone must carry the claim too: they are whole
        // meshes of our own, not a reshaped download.
        assert!(generated.len() >= 300, "only {} distinct generated bodies", generated.len());
    }

    #[test]
    fn model_choice_matches_the_spawn_code() {
        // Named plans win over rigs; rig species stay on rigs.
        assert!(matches!(model_key("gilded_pterosaur_behemoth_rider_mount"), MobModel::Generated { plan: 9, .. }));
        assert!(matches!(model_key("rabid_cave_bear_scavenger"), MobModel::Rig { .. }));
        assert_eq!(model_key("starving_smilodon"), model_key("starving_smilodon"));
    }
}
