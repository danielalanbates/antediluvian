//! POI set-dressing with Kenney CC0 model kits (assets/models/kenney/*).
//!
//! Every POI keyword-matches its name to a themed prop arrangement (camp,
//! wreckage, graveyard, altar, market…) with an act-flavored fallback, so
//! discoverable sites read as *places* instead of a lone cairn. All props
//! are static scenery: `Terrain` + the CHUNK_10 visibility band.

use bevy::prelude::*;
use bevy::render::view::VisibilityRange;

use antediluvia_protocol::Act;

/// Same fade band as decor scatter (CHUNK_10).
const DECOR_RANGE: VisibilityRange = VisibilityRange {
    start_margin: 0.0..0.0,
    end_margin: 2200.0..2600.0,
    use_aabb: false,
};

/// Kenney kit models are ~1–2 units across; world characters are ~55u.
/// Baseline multiplier applied on top of each prop's own scale below.
const KENNEY_SCALE: f32 = 30.0;

/// One prop of a set: (path, dx, dz, scale, yaw-turns).
type P = (&'static str, f32, f32, f32, f32);

fn set_camp() -> &'static [P] {
    &[
        ("models/kenney/survival/tent-canvas.glb", -1.8, -1.2, 1.3, 0.15),
        ("models/kenney/survival/tent-canvas-half.glb", 2.1, -1.6, 1.2, 0.65),
        ("models/kenney/nature/campfire_stones.glb", 0.0, 0.6, 1.0, 0.0),
        ("models/kenney/survival/bedroll.glb", -0.9, 1.4, 1.0, 0.4),
        ("models/kenney/survival/box-large.glb", 1.6, 1.1, 0.9, 0.1),
        ("models/kenney/nature/log_stack.glb", -2.4, 0.8, 1.0, 0.8),
    ]
}

fn set_wreckage() -> &'static [P] {
    &[
        ("models/kenney/fantasy-town/cart.glb", 0.0, 0.0, 1.4, 0.2),
        ("models/kenney/survival/box-large-open.glb", 1.5, 0.9, 1.0, 0.55),
        ("models/kenney/pirate/barrel.glb", -1.4, 0.7, 1.0, 0.0),
        ("models/kenney/graveyard/debris-wood.glb", 0.8, -1.3, 1.2, 0.35),
        ("models/kenney/pirate/crate.glb", -0.9, -1.5, 0.9, 0.7),
    ]
}

fn set_graveyard() -> &'static [P] {
    &[
        ("models/kenney/graveyard/crypt-small.glb", 0.0, -2.0, 1.6, 0.0),
        ("models/kenney/graveyard/gravestone-cross.glb", -1.5, 0.4, 1.0, 0.05),
        ("models/kenney/graveyard/gravestone-round.glb", -0.5, 0.9, 1.0, 0.95),
        ("models/kenney/graveyard/gravestone-broken.glb", 0.6, 0.7, 1.0, 0.1),
        ("models/kenney/graveyard/gravestone-wide.glb", 1.6, 0.2, 1.0, 0.9),
        ("models/kenney/graveyard/iron-fence-bar.glb", -2.2, -0.8, 1.2, 0.25),
        ("models/kenney/graveyard/lantern-glass.glb", 2.2, -1.0, 1.1, 0.0),
    ]
}

fn set_altar() -> &'static [P] {
    &[
        ("models/kenney/graveyard/altar-stone.glb", 0.0, 0.0, 1.6, 0.0),
        ("models/kenney/graveyard/candle-multiple.glb", 0.7, 0.5, 1.0, 0.3),
        ("models/kenney/graveyard/candle.glb", -0.7, 0.4, 1.0, 0.0),
        ("models/kenney/graveyard/column-large.glb", -1.9, -1.2, 1.3, 0.0),
        ("models/kenney/graveyard/border-pillar.glb", 1.9, -1.2, 1.3, 0.0),
    ]
}

fn set_outpost() -> &'static [P] {
    &[
        ("models/kenney/castle/tower-square-base.glb", 0.0, -1.8, 2.2, 0.0),
        ("models/kenney/survival/fence-fortified.glb", -1.8, 0.6, 1.4, 0.1),
        ("models/kenney/survival/fence-fortified.glb", 1.8, 0.6, 1.4, 0.9),
        ("models/kenney/castle/flag-banner-long.glb", 0.9, -0.6, 1.4, 0.0),
        ("models/kenney/survival/resource-stone-large.glb", -1.2, 1.5, 1.0, 0.5),
    ]
}

fn set_market() -> &'static [P] {
    &[
        ("models/kenney/fantasy-town/stall-red.glb", -1.6, -0.8, 1.4, 0.1),
        ("models/kenney/fantasy-town/stall-green.glb", 1.6, -0.9, 1.4, 0.9),
        ("models/kenney/fantasy-town/cart-high.glb", 0.2, 1.4, 1.2, 0.45),
        ("models/kenney/fantasy-town/lantern.glb", -0.9, 1.0, 1.1, 0.0),
        ("models/kenney/pirate/crate-bottles.glb", 0.9, -0.1, 0.9, 0.2),
    ]
}

fn set_grove() -> &'static [P] {
    &[
        ("models/kenney/fantasy-town/tree-high-round.glb", -1.8, -1.2, 1.8, 0.0),
        ("models/kenney/fantasy-town/tree-crooked.glb", 1.9, -0.9, 1.6, 0.4),
        ("models/kenney/nature/mushroom_redGroup.glb", 0.6, 0.9, 1.1, 0.2),
        ("models/kenney/nature/plant_bushDetailed.glb", -0.8, 1.2, 1.2, 0.6),
        ("models/kenney/nature/log_large.glb", 0.1, -0.2, 1.0, 0.75),
    ]
}

fn set_shipwreck() -> &'static [P] {
    &[
        ("models/kenney/pirate/boat-row-large.glb", 0.0, 0.0, 1.8, 0.3),
        ("models/kenney/pirate/mast-ropes.glb", 1.2, -0.8, 1.4, 0.1),
        ("models/kenney/pirate/barrel.glb", -1.5, 0.9, 1.0, 0.0),
        ("models/kenney/pirate/chest.glb", 1.7, 1.0, 1.0, 0.6),
        ("models/kenney/pirate/rocks-sand-a.glb", -2.0, -1.3, 1.3, 0.45),
    ]
}

fn set_spring() -> &'static [P] {
    &[
        ("models/kenney/fantasy-town/fountain-round-detail.glb", 0.0, 0.0, 1.6, 0.0),
        ("models/kenney/nature/plant_bushLarge.glb", -1.7, 0.9, 1.2, 0.3),
        ("models/kenney/nature/plant_bush.glb", 1.6, 0.8, 1.1, 0.8),
        ("models/kenney/fantasy-town/lantern.glb", 1.1, -1.2, 1.1, 0.0),
    ]
}

// ── Hero landmarks: Blender-authored GLBs (scripts/render/models.py) ────────

fn set_hero_ziggurat() -> &'static [P] {
    &[
        ("models/hero/ziggurat.glb", 0.0, 0.0, 0.40, 0.0),
        ("models/kenney/castle/flag-banner-long.glb", -2.6, 2.4, 1.4, 0.0),
        ("models/kenney/castle/flag-banner-long.glb", 2.6, 2.4, 1.4, 0.0),
    ]
}

fn set_hero_altar() -> &'static [P] {
    &[
        ("models/hero/altar.glb", 0.0, 0.0, 0.30, 0.0),
        ("models/kenney/graveyard/candle-multiple.glb", 1.4, 1.2, 1.0, 0.2),
        ("models/kenney/graveyard/cross-wood.glb", -1.8, 1.5, 1.1, 0.1),
    ]
}

fn set_hero_boundary() -> &'static [P] {
    &[
        ("models/hero/boundary.glb", 0.0, 0.0, 0.40, 0.0),
        ("models/kenney/graveyard/debris.glb", 1.8, 1.4, 1.2, 0.3),
        ("models/kenney/graveyard/debris-wood.glb", -1.9, 1.2, 1.2, 0.7),
    ]
}

fn set_hero_ark() -> &'static [P] {
    &[
        ("models/hero/ark.glb", 0.0, 0.0, 0.50, 0.0),
        ("models/kenney/survival/resource-planks.glb", 3.2, 1.8, 1.3, 0.2),
        ("models/kenney/survival/resource-wood.glb", -3.0, 1.9, 1.3, 0.6),
        ("models/kenney/pirate/barrel.glb", 2.4, -1.8, 1.0, 0.0),
        ("models/kenney/survival/campfire-pit.glb", -2.6, -1.7, 1.0, 0.0),
    ]
}

fn set_hero_descent() -> &'static [P] {
    &[
        ("models/hero/descent.glb", 0.0, 0.0, 0.70, 0.0),
        ("models/kenney/graveyard/debris.glb", 2.2, 1.6, 1.3, 0.4),
        ("models/kenney/survival/rock-b.glb", -2.3, 1.4, 1.4, 0.1),
    ]
}

fn set_hero_observatory() -> &'static [P] {
    &[
        ("models/hero/observatory.glb", 0.0, 0.0, 0.65, 0.0),
        ("models/kenney/fantasy-town/lantern.glb", 1.6, 1.4, 1.1, 0.0),
        ("models/kenney/survival/box.glb", -1.5, 1.3, 0.9, 0.3),
    ]
}

fn set_hero_bonetotem() -> &'static [P] {
    &[
        ("models/hero/bonetotem.glb", 0.0, 0.0, 0.70, 0.0),
        ("models/kenney/graveyard/debris.glb", 2.4, 1.8, 1.4, 0.2),
        ("models/kenney/survival/rock-flat.glb", -2.2, 1.7, 1.3, 0.6),
    ]
}

fn set_hero_wartent() -> &'static [P] {
    &[
        ("models/hero/wartent.glb", 0.0, 0.0, 0.55, 0.0),
        ("models/kenney/survival/campfire-stand.glb", 2.6, 1.6, 1.1, 0.0),
        ("models/kenney/castle/flag-banner-short.glb", -2.5, 1.8, 1.3, 0.1),
        ("models/kenney/survival/box-large.glb", 2.2, -2.0, 1.0, 0.4),
    ]
}

fn set_hero_leviathan() -> &'static [P] {
    &[
        ("models/hero/leviathan.glb", 0.0, 0.0, 0.70, 0.0),
        ("models/kenney/pirate/rocks-sand-b.glb", 2.6, 1.8, 1.4, 0.3),
        ("models/kenney/pirate/grass-patch.glb", -2.4, 1.6, 1.3, 0.6),
    ]
}

fn set_hero_footprint() -> &'static [P] {
    &[
        ("models/hero/footprint.glb", 0.0, 0.0, 0.80, 0.0),
        ("models/kenney/nature/plant_bush.glb", 2.8, 1.8, 1.2, 0.2),
        ("models/kenney/survival/rock-c.glb", -2.7, 1.9, 1.3, 0.7),
    ]
}

fn set_hero_geyser() -> &'static [P] {
    &[
        ("models/hero/geyser.glb", 0.0, 0.0, 0.45, 0.0),
        ("models/kenney/survival/rock-sand-a.glb", 2.4, 1.5, 1.2, 0.1),
        ("models/kenney/pirate/rocks-c.glb", -2.3, 1.6, 1.2, 0.5),
    ]
}

fn set_hero_feastpit() -> &'static [P] {
    &[
        ("models/hero/feastpit.glb", 0.0, 0.0, 0.45, 0.0),
        ("models/kenney/graveyard/debris.glb", 2.6, 1.7, 1.3, 0.4),
        ("models/kenney/survival/box-open.glb", -2.5, 1.6, 1.0, 0.2),
    ]
}

/// Act-flavored fallback for POI names that match no keyword.
fn fallback_set(act: Act) -> &'static [P] {
    match act {
        Act::Eden => set_grove(),
        Act::Hermon => set_camp(),
        Act::Nephilim => set_graveyard(),
        Act::Enoch => set_market(),
        Act::Flood => set_shipwreck(),
    }
}

/// Keyword → set. First match wins; scan order is specific → generic.
fn set_for(name: &str, act: Act) -> &'static [P] {
    let n = name.to_ascii_lowercase();
    let table: [(&[&str], &'static [P]); 20] = [
        // Hero landmarks first — they outrank generic keywords.
        (&["ziggurat"], set_hero_ziggurat()),
        (&["first-generation altar"], set_hero_altar()),
        (&["cherubim", "burn-scar", "flaming boundary"], set_hero_boundary()),
        (&["ark construction"], set_hero_ark()),
        (&["descent point", "impact crater"], set_hero_descent()),
        (&["observatory"], set_hero_observatory()),
        (&["bone-totem"], set_hero_bonetotem()),
        (&["command tent"], set_hero_wartent()),
        (&["leviathan"], set_hero_leviathan()),
        (&["footprint"], set_hero_footprint()),
        (&["geyser"], set_hero_geyser()),
        (&["feasting pit"], set_hero_feastpit()),
        (&["wreckage", "wreck", "sunken"], set_wreckage()),
        (&["camp", "hideout", "den"], set_camp()),
        (&["grave", "tomb", "crypt", "barrow"], set_graveyard()),
        (&["altar", "shrine", "sacred", "temple"], set_altar()),
        (&["outpost", "fortified", "fort", "watchtower"], set_outpost()),
        (&["market", "bazaar", "caravan"], set_market()),
        (&["grove", "baobab", "orchard", "thicket"], set_grove()),
        (&["spring", "oasis", "well", "delta"], set_spring()),
    ];
    for (keys, set) in table {
        if keys.iter().any(|k| n.contains(k)) {
            return set;
        }
    }
    fallback_set(act)
}

/// Spawn a themed prop arrangement at each POI of the act.
/// `height` maps world (x, z) → terrain y so props sit on the ground.
pub fn dress_pois<'a>(
    commands: &mut Commands,
    asset_server: &AssetServer,
    act: Act,
    pois: impl Iterator<Item = (&'a str, f32, f32)>,
    height: impl Fn(f32, f32) -> f32,
) {
    for (i, (name, px, pz)) in pois.enumerate() {
        let set = set_for(name, act);
        // Per-POI deterministic yaw so identical sets don't tile visibly.
        let base_yaw = ((i as u64).wrapping_mul(0x9E37_79B9) % 628) as f32 / 100.0;
        for (path, dx, dz, scale, yaw_turns) in set {
            let (s, c) = base_yaw.sin_cos();
            let (wx, wz) = (
                px + (dx * c - dz * s) * KENNEY_SCALE,
                pz + (dx * s + dz * c) * KENNEY_SCALE,
            );
            let y = height(wx, wz);
            commands.spawn((
                SceneRoot(
                    asset_server.load(GltfAssetLabel::Scene(0).from_asset(path.to_string())),
                ),
                Transform::from_xyz(wx, y, wz)
                    .with_scale(Vec3::splat(scale * KENNEY_SCALE))
                    .with_rotation(Quat::from_rotation_y(
                        base_yaw + yaw_turns * std::f32::consts::TAU,
                    )),
                super::Terrain,
                DECOR_RANGE,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every model path referenced by a set must exist on disk.
    #[test]
    fn all_set_models_exist() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut checked = 0;
        for set in [
            set_camp(), set_wreckage(), set_graveyard(), set_altar(),
            set_outpost(), set_market(), set_grove(), set_shipwreck(), set_spring(),
            set_hero_ziggurat(), set_hero_altar(), set_hero_boundary(),
            set_hero_ark(), set_hero_descent(), set_hero_observatory(),
            set_hero_bonetotem(), set_hero_wartent(),
            set_hero_leviathan(), set_hero_footprint(), set_hero_geyser(),
            set_hero_feastpit(),
        ] {
            for (path, ..) in set {
                assert!(root.join(path).exists(), "missing model: {path}");
                checked += 1;
            }
        }
        assert!(checked > 40);
    }

    #[test]
    fn keyword_matching_covers_real_names() {
        // "camp" scans before "fortified", so a fortified camp is a camp.
        assert!(std::ptr::eq(set_for("The Fortified Nomad Camp", Act::Eden).as_ptr(), set_camp().as_ptr()));
        assert!(std::ptr::eq(set_for("The Smoldering Caravan Wreckage", Act::Eden).as_ptr(), set_wreckage().as_ptr()));
        assert!(std::ptr::eq(set_for("The Corrupted First-Generation Altar", Act::Nephilim).as_ptr(), set_hero_altar().as_ptr()));
        assert!(std::ptr::eq(set_for("The Fortified Ziggurat Tier", Act::Enoch).as_ptr(), set_hero_ziggurat().as_ptr()));
        assert!(std::ptr::eq(set_for("The Abandoned Cherubim Burn-Scar", Act::Eden).as_ptr(), set_hero_boundary().as_ptr()));
        assert!(std::ptr::eq(set_for("The Pristine Sacred Grove", Act::Eden).as_ptr(), set_altar().as_ptr()));
        // No keyword → act fallback.
        assert!(std::ptr::eq(set_for("The Silent Nowhere", Act::Flood).as_ptr(), set_shipwreck().as_ptr()));
    }
}
