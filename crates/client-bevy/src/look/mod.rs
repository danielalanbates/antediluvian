// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Character looks: the option tables behind the character builder and what
//! each option means on screen.
//!
//! A look is `antediluvia_protocol::Look` (twelve indices in one number) plus
//! the outfit in `appearance[0]`. This module names every option and gives
//! it a colour or a shape; `geom` builds the hair, beard and mantle meshes
//! and the face morphs; `apply` puts a look on a rig; `builder` is the
//! creation screen.

pub mod apply;
pub mod builder;
pub mod geom;

use antediluvia_protocol::look::Look;
use bevy::prelude::*;

pub type Rgb = [f32; 3];

pub const OUTFIT: [&str; 4] = ["Plate and mail", "Furs and hide", "Leathers", "Robes"];
pub const OUTFIT_FILES: [&str; 4] = [
    "models/characters/Knight.glb",
    "models/characters/Barbarian.glb",
    "models/characters/Rogue.glb",
    "models/characters/Mage.glb",
];

/// Skin tones, sRGB. Index 2 is the tone the rigs were painted in.
pub const SKIN: [(&str, Rgb); 20] = [
    ("Porcelain", [0.98, 0.87, 0.80]),
    ("Ivory", [0.96, 0.82, 0.72]),
    ("Fair", [0.965, 0.757, 0.616]),
    ("Rose", [0.93, 0.70, 0.62]),
    ("Sand", [0.90, 0.72, 0.55]),
    ("Honey", [0.85, 0.64, 0.44]),
    ("Olive", [0.76, 0.62, 0.42]),
    ("Tan", [0.78, 0.56, 0.38]),
    ("Bronze", [0.70, 0.47, 0.30]),
    ("Copper", [0.68, 0.40, 0.26]),
    ("Umber", [0.56, 0.36, 0.24]),
    ("Chestnut", [0.48, 0.30, 0.20]),
    ("Sepia", [0.40, 0.25, 0.17]),
    ("Mahogany", [0.34, 0.19, 0.13]),
    ("Ebony", [0.24, 0.15, 0.11]),
    ("Onyx", [0.17, 0.11, 0.09]),
    ("Ashen", [0.70, 0.68, 0.66]),
    ("Ruddy", [0.82, 0.48, 0.40]),
    ("Golden", [0.88, 0.70, 0.36]),
    ("Dusk", [0.50, 0.42, 0.48]),
];
/// The painted skin tone of the source rigs (texture average).
pub const SKIN_REF: Rgb = [0.965, 0.757, 0.616];

pub const HAIR_COLOR: [(&str, Rgb); 24] = [
    ("Raven", [0.07, 0.06, 0.07]),
    ("Soot", [0.15, 0.14, 0.14]),
    ("Dark brown", [0.23, 0.15, 0.10]),
    ("Brown", [0.36, 0.23, 0.14]),
    ("Chestnut", [0.45, 0.25, 0.15]),
    ("Auburn", [0.52, 0.21, 0.12]),
    ("Copper", [0.70, 0.34, 0.14]),
    ("Ginger", [0.82, 0.42, 0.16]),
    ("Strawberry", [0.86, 0.56, 0.38]),
    ("Dark blond", [0.62, 0.48, 0.28]),
    ("Blond", [0.84, 0.69, 0.40]),
    ("Flaxen", [0.92, 0.82, 0.56]),
    ("Platinum", [0.93, 0.90, 0.80]),
    ("White", [0.95, 0.95, 0.94]),
    ("Silver", [0.74, 0.76, 0.79]),
    ("Grey", [0.52, 0.52, 0.53]),
    ("Ash brown", [0.42, 0.36, 0.31]),
    ("Wine", [0.42, 0.12, 0.18]),
    ("Midnight", [0.10, 0.12, 0.24]),
    ("Moss", [0.28, 0.34, 0.18]),
    ("Ochre", [0.70, 0.52, 0.18]),
    ("Slate", [0.34, 0.42, 0.52]),
    ("Rust", [0.58, 0.28, 0.16]),
    ("Sandstone", [0.74, 0.62, 0.48]),
];

pub const EYES: [(&str, Rgb); 14] = [
    ("Black", [0.05, 0.05, 0.06]),
    ("Dark brown", [0.20, 0.11, 0.06]),
    ("Brown", [0.36, 0.20, 0.09]),
    ("Hazel", [0.44, 0.34, 0.14]),
    ("Amber", [0.72, 0.46, 0.10]),
    ("Gold", [0.82, 0.66, 0.16]),
    ("Green", [0.20, 0.44, 0.20]),
    ("Emerald", [0.08, 0.52, 0.32]),
    ("Grey", [0.42, 0.45, 0.48]),
    ("Steel", [0.26, 0.34, 0.44]),
    ("Blue", [0.14, 0.30, 0.66]),
    ("Ice", [0.50, 0.72, 0.86]),
    ("Violet", [0.42, 0.24, 0.62]),
    ("Russet", [0.56, 0.18, 0.10]),
];

/// Garment dyes. Index 0 leaves the outfit as painted.
pub const DYE: [(&str, Rgb); 24] = [
    ("Undyed", [0.42, 0.33, 0.22]),
    ("Bone", [0.82, 0.78, 0.66]),
    ("Ash", [0.48, 0.48, 0.47]),
    ("Charcoal", [0.16, 0.16, 0.17]),
    ("Madder red", [0.62, 0.16, 0.14]),
    ("Crimson", [0.48, 0.08, 0.12]),
    ("Rust", [0.62, 0.32, 0.16]),
    ("Ochre", [0.74, 0.56, 0.18]),
    ("Saffron", [0.86, 0.66, 0.14]),
    ("Olive", [0.40, 0.42, 0.18]),
    ("Moss", [0.24, 0.36, 0.16]),
    ("Forest", [0.10, 0.28, 0.16]),
    ("Teal", [0.10, 0.38, 0.38]),
    ("Sea", [0.18, 0.46, 0.58]),
    ("Woad blue", [0.16, 0.26, 0.56]),
    ("Indigo", [0.14, 0.14, 0.38]),
    ("Violet", [0.36, 0.20, 0.52]),
    ("Plum", [0.40, 0.14, 0.32]),
    ("Rose", [0.74, 0.40, 0.46]),
    ("Walnut", [0.30, 0.19, 0.11]),
    ("Sand", [0.76, 0.66, 0.46]),
    ("Slate", [0.30, 0.36, 0.44]),
    ("Wine", [0.36, 0.08, 0.16]),
    ("Sky", [0.48, 0.66, 0.82]),
];

pub const MARK_COLOR: [(&str, Rgb); 8] = [
    ("Red ochre", [0.62, 0.16, 0.10]),
    ("Woad", [0.12, 0.22, 0.56]),
    ("Ash white", [0.90, 0.89, 0.85]),
    ("Soot", [0.06, 0.06, 0.07]),
    ("Verdigris", [0.14, 0.46, 0.34]),
    ("Gold", [0.82, 0.64, 0.14]),
    ("Violet", [0.40, 0.18, 0.56]),
    ("Clay", [0.70, 0.44, 0.24]),
];

pub const HAIR: [&str; 32] = [
    "Natural",
    "Topknot",
    "Low bun",
    "High bun",
    "Mohawk",
    "Crest",
    "Spiked crown",
    "Pompadour",
    "Ponytail",
    "High ponytail",
    "Long fall",
    "Braid",
    "Twin tails",
    "Twin braids",
    "Side buns",
    "Temple locks",
    "Mane",
    "Flat top",
    "Curls",
    "Twin horns",
    "Mohawk and braid",
    "Topknot and locks",
    "Bun and braids",
    "Side buns and fall",
    "Curls and ponytail",
    "Crest and twin tails",
    "Pompadour and ponytail",
    "Spikes and mane",
    "Flat top and locks",
    "Braided crown",
    "Crown and long fall",
    "Warrior's queue",
];

pub const BEARD: [&str; 14] = [
    "None",
    "Goatee",
    "Short beard",
    "Full beard",
    "Long beard",
    "Forked beard",
    "Braided beard",
    "Moustache",
    "Handlebar",
    "Chinstrap",
    "Side whiskers",
    "Soul patch",
    "Twin-braided beard",
    "Sage's beard",
];

pub const MANTLE: [&str; 6] = ["None", "Cape", "Shoulder pelt", "Sash", "Scarf", "Shawl"];

/// Build: (name, [width, depth]) multipliers on the rig.
pub const BUILD: [(&str, [f32; 2]); 8] = [
    ("Slight", [0.84, 0.86]),
    ("Lean", [0.91, 0.90]),
    ("Average", [1.00, 1.00]),
    ("Athletic", [1.07, 0.97]),
    ("Sturdy", [1.10, 1.10]),
    ("Broad", [1.20, 1.00]),
    ("Heavy", [1.17, 1.22]),
    ("Barrel-chested", [1.26, 1.14]),
];

pub const HEIGHT: [(&str, f32); 9] = [
    ("Very short", 0.86),
    ("Short", 0.90),
    ("Below average", 0.94),
    ("Slightly short", 0.97),
    ("Average", 1.00),
    ("Slightly tall", 1.03),
    ("Tall", 1.06),
    ("Very tall", 1.10),
    ("Towering", 1.14),
];

pub const MARKING: [&str; 20] = [
    "None",
    "Brow band",
    "Eye mask",
    "Chin paint",
    "Lower face",
    "Left half",
    "Right half",
    "Centre stripe",
    "Cheek stripes",
    "Cross",
    "War stripes",
    "Cheek dots",
    "Forehead mark",
    "Tear lines",
    "Scar over the left eye",
    "Scar on the right cheek",
    "Scar across the nose",
    "Scar through the brow",
    "Burn scar",
    "Full face paint",
];

const JAW: [&str; 4] = ["Narrow", "Oval", "Broad", "Square"];
const NOSE: [&str; 3] = ["small nose", "straight nose", "strong nose"];
const BROW: [&str; 2] = ["soft brow", "heavy brow"];

pub fn face_name(i: u8) -> String {
    let i = i as usize % 24;
    format!("{}, {}, {}", JAW[i % 4], NOSE[(i / 4) % 3], BROW[i / 12])
}

/// Field order of `LOOK_FIELDS`, as indices.
pub const F_FACE: usize = 0;
pub const F_HAIR: usize = 1;
pub const F_HAIR_COLOR: usize = 2;
pub const F_BEARD: usize = 3;
pub const F_SKIN: usize = 4;
pub const F_BUILD: usize = 5;
pub const F_HEIGHT: usize = 6;
pub const F_MARKING: usize = 7;
pub const F_MARK_COLOR: usize = 8;
pub const F_EYES: usize = 9;
pub const F_MANTLE: usize = 10;
pub const F_DYE: usize = 11;

/// Player-facing label of a look field.
pub fn field_label(field: usize) -> &'static str {
    [
        "Face", "Hair", "Hair colour", "Facial hair", "Skin", "Build", "Height", "Marking",
        "Marking colour", "Eyes", "Mantle", "Dye",
    ][field % 12]
}

/// Player-facing name of one option.
pub fn option_name(field: usize, v: u8) -> String {
    let i = v as usize;
    match field {
        F_FACE => face_name(v),
        F_HAIR => HAIR[i % HAIR.len()].into(),
        F_HAIR_COLOR => HAIR_COLOR[i % HAIR_COLOR.len()].0.into(),
        F_BEARD => BEARD[i % BEARD.len()].into(),
        F_SKIN => SKIN[i % SKIN.len()].0.into(),
        F_BUILD => BUILD[i % BUILD.len()].0.into(),
        F_HEIGHT => HEIGHT[i % HEIGHT.len()].0.into(),
        F_MARKING => MARKING[i % MARKING.len()].into(),
        F_MARK_COLOR => MARK_COLOR[i % MARK_COLOR.len()].0.into(),
        F_EYES => EYES[i % EYES.len()].0.into(),
        F_MANTLE => MANTLE[i % MANTLE.len()].into(),
        _ => DYE[i % DYE.len()].0.into(),
    }
}

/// Colour swatch for the colour fields.
pub fn swatch(field: usize, v: u8) -> Option<Rgb> {
    let i = v as usize;
    match field {
        F_HAIR_COLOR => Some(HAIR_COLOR[i % HAIR_COLOR.len()].1),
        F_SKIN => Some(SKIN[i % SKIN.len()].1),
        F_MARK_COLOR => Some(MARK_COLOR[i % MARK_COLOR.len()].1),
        F_EYES => Some(EYES[i % EYES.len()].1),
        F_DYE => Some(DYE[i % DYE.len()].1),
        _ => None,
    }
}

/// Rig scale for build and height: (width, height, depth).
pub fn body_scale(look: Look) -> Vec3 {
    let b = BUILD[look.build as usize % BUILD.len()].1;
    let h = HEIGHT[look.height as usize % HEIGHT.len()].1;
    // Taller people are a little wider too, or tall reads as stretched.
    let k = 1.0 + (h - 1.0) * 0.5;
    Vec3::new(b[0] * k, h, b[1] * k)
}

/// What a look renders as, reduced to numbers: every colour, the scale, the
/// face morph, the marking mask and the geometry of the grafted meshes. Two
/// looks with the same signature would be drawn identically.
pub fn signature(outfit: u32, look: Look) -> u64 {
    let look = look.clamped();
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    let mut eat = |v: u64| h = (h ^ v).wrapping_mul(0x100_0000_01B3);
    eat(outfit as u64 % 4);
    let q = |c: Rgb| ((c[0] * 255.0) as u64) << 16 | ((c[1] * 255.0) as u64) << 8 | (c[2] * 255.0) as u64;
    eat(q(SKIN[look.skin as usize].1));
    eat(q(EYES[look.eyes as usize].1));
    eat(q(DYE[look.dye as usize].1) ^ ((look.dye as u64) << 32));
    let s = body_scale(look);
    eat((s.x * 1000.0) as u64);
    eat((s.y * 1000.0) as u64);
    eat((s.z * 1000.0) as u64);
    eat(geom::face_fingerprint(look.face));
    eat(geom::marking_fingerprint(look.marking));
    if look.marking != 0 && !geom::is_scar(look.marking) {
        eat(q(MARK_COLOR[look.mark_color as usize].1));
    }
    // Hair colour also tints the rig's own hair, so it always counts.
    eat(q(HAIR_COLOR[look.hair_color as usize].1));
    eat(geom::head_graft(look).map(|m| crate::formations::geom_hash(&m)).unwrap_or(1));
    eat(geom::mantle_graft(look).map(|m| crate::formations::geom_hash(&m)).unwrap_or(2));
    eat(look.mantle as u64);
    h
}

/// Art-review sheet (`ANTEDILUVIA_CHARSHEET_OUT`): two rows of eight random
/// looks over the four outfits, rendered through the same code as players.
pub fn spawn_look_sheet(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: &AssetServer,
) {
    use bevy::gltf::GltfAssetLabel;
    let seed: u64 = std::env::var("ANTEDILUVIA_CHARSHEET_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let cols = 8u64;
    const SPACING: f32 = 1.75;
    for i in 0..cols * 2 {
        let mut look = Look::random(seed * 1000 + i);
        // Guarantee the sheet shows beards, markings and mantles even though
        // a random look usually has none.
        if i % 3 == 0 {
            look.beard = 1 + ((seed + i) % 13) as u8;
        }
        if i % 2 == 0 {
            look.marking = 1 + ((seed * 7 + i) % 19) as u8;
        }
        look.mantle = (i % 6) as u8;
        let (c, r) = (i % cols, i / cols);
        let pos = Vec3::new((c as f32 - (cols - 1) as f32 * 0.5) * SPACING, r as f32 * 2.5, -(r as f32) * 2.0);
        commands.spawn((
            SceneRoot(server.load(GltfAssetLabel::Scene(0).from_asset(OUTFIT_FILES[(i % 4) as usize]))),
            Transform::from_translation(pos).with_scale(body_scale(look)),
            apply::LookRig { look },
            crate::equipment::Loadout::default(),
        ));
    }
    let ground = meshes.add(Plane3d::default().mesh().size(60.0, 60.0));
    let gmat = materials.add(StandardMaterial { base_color: Color::srgb(0.30, 0.33, 0.26), ..default() });
    commands.spawn((Mesh3d(ground), MeshMaterial3d(gmat.clone())));
    // The back row stands on a step.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(16.0, 2.5, 2.4))),
        MeshMaterial3d(gmat),
        Transform::from_xyz(0.0, 1.25, -2.2),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 12_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(3.0, 9.0, 12.0).looking_at(Vec3::new(0.0, 2.0, 0.0), Vec3::Y),
    ));
    let envf = |k: &str, d: f32| -> f32 { std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d) };
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, envf("ANTEDILUVIA_CHARSHEET_CAMH", 3.2), envf("ANTEDILUVIA_CHARSHEET_CAMD", 11.5))
            .looking_at(Vec3::new(0.0, envf("ANTEDILUVIA_CHARSHEET_LOOKY", 2.5), 0.0), Vec3::Y),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use antediluvia_protocol::look::LOOK_FIELDS;
    use std::collections::HashSet;

    #[test]
    fn tables_match_the_wire_field_counts() {
        let lens = [
            24,
            HAIR.len(),
            HAIR_COLOR.len(),
            BEARD.len(),
            SKIN.len(),
            BUILD.len(),
            HEIGHT.len(),
            MARKING.len(),
            MARK_COLOR.len(),
            EYES.len(),
            MANTLE.len(),
            DYE.len(),
        ];
        for (i, f) in LOOK_FIELDS.iter().enumerate() {
            assert_eq!(lens[i], f.count as usize, "{} table length", f.name);
        }
        assert_eq!(OUTFIT.len() as u32, antediluvia_protocol::look::OUTFITS);
    }

    /// Every single option changes what is drawn: stepping one field through
    /// all its values, from several base looks, never repeats a signature.
    #[test]
    fn every_option_of_every_field_renders_differently() {
        let mut total = 0;
        for base_seed in [0u64, 5, 11] {
            let mut base = Look::random(base_seed);
            // A marking colour only shows with a painted marking.
            base.marking = 2;
            for (i, f) in LOOK_FIELDS.iter().enumerate() {
                let mut seen = HashSet::new();
                let mut names = HashSet::new();
                for v in 0..f.count {
                    let mut a = base.to_array();
                    a[i] = v;
                    if i == F_MARK_COLOR {
                        a[F_MARKING] = 2;
                    }
                    let l = Look::from_array(a);
                    assert!(seen.insert(signature(0, l)), "{} option {v} draws like another", f.name);
                    assert!(names.insert(option_name(i, v)), "{} option {v} shares a name", f.name);
                }
                total += seen.len();
            }
            let outfits: HashSet<u64> = (0..4).map(|o| signature(o, base)).collect();
            assert_eq!(outfits.len(), 4);
            total += 4;
        }
        // Per base look: the sum of all option counts.
        assert_eq!(total as u32, 3 * Look::option_count());
        assert!(Look::option_count() >= 200, "{} options", Look::option_count());
    }

    /// Whole looks: thousands of random characters, all drawn differently.
    #[test]
    fn thousands_of_random_looks_are_all_distinct() {
        let mut seen = HashSet::new();
        let n = 3000u64;
        for s in 0..n {
            seen.insert(signature((s % 4) as u32, Look::random(s * 7919 + 3)));
        }
        println!("distinct rendered looks: {} of {n}", seen.len());
        assert!(seen.len() as u64 >= n - 3, "only {} distinct looks in {n}", seen.len());
    }

    #[test]
    fn body_scale_is_sane_and_monotonic_in_height() {
        let mut last = 0.0;
        for h in 0..HEIGHT.len() as u8 {
            let s = body_scale(Look { height: h, build: 2, ..Look::default() });
            assert!(s.y > last);
            last = s.y;
            assert!(s.min_element() > 0.8 && s.max_element() < 1.2);
        }
        for b in 0..BUILD.len() as u8 {
            for h in [0u8, 8] {
                let s = body_scale(Look { height: h, build: b, ..Look::default() });
                assert!(s.min_element() > 0.7 && s.max_element() < 1.4, "{s:?}");
            }
        }
        assert_eq!(body_scale(Look { height: 4, build: 2, ..Look::default() }), Vec3::ONE);
    }

    #[test]
    fn colour_tables_have_no_duplicates() {
        fn distinct(t: &[(&str, Rgb)]) -> bool {
            let s: HashSet<[u32; 3]> = t.iter().map(|(_, c)| [(c[0] * 100.0) as u32, (c[1] * 100.0) as u32, (c[2] * 100.0) as u32]).collect();
            s.len() == t.len()
        }
        assert!(distinct(&SKIN) && distinct(&HAIR_COLOR) && distinct(&EYES) && distinct(&DYE) && distinct(&MARK_COLOR));
    }
}
