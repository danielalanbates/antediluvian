// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Puts a look on a character rig.
//!
//! The adventurer rigs carry skin, hair, eyes and clothing in ONE texture, so
//! a material tint cannot change skin without changing the tunic. Instead,
//! each body-part mesh is copied once per character and given vertex colours
//! that multiply the texture: skin vertices toward the chosen tone, hair
//! toward the hair colour, the eyes toward the eye colour, clothing toward
//! the dye, and face paint or scars on top. The head copy is also reshaped
//! for the chosen face. Hair, beard and mantle are grafted meshes on the
//! head and chest bones (one mesh per bone, one shared material).
//!
//! Cost: six small mesh copies and at most two grafts per character that is
//! close enough to have a rig at all; distant players are the shared LOD
//! figure and never reach this system. Copies are freed with the character.

use antediluvia_protocol::look::Look;
use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;
use bevy::render::render_resource::TextureFormat;

use super::geom::{self, srgb_to_linear};
use super::{Rgb, DYE, EYES, HAIR_COLOR, MARK_COLOR, SKIN, SKIN_REF};

/// Ask for this rig (the `SceneRoot` entity) to wear a look.
#[derive(Component, Clone, Copy, PartialEq)]
pub struct LookRig {
    pub look: Look,
}

/// The look currently on the rig, and the grafts that belong to it.
#[derive(Component)]
pub struct LookApplied {
    look: Look,
    grafts: Vec<Entity>,
}

/// On a recoloured body-part mesh entity: the rig's original mesh.
#[derive(Component)]
pub struct Pristine(pub Handle<Mesh>);

/// The one material every graft shares (colour is per vertex).
#[derive(Resource, Default)]
pub struct LookAssets {
    graft: Option<Handle<StandardMaterial>>,
}

/// How much of an eye colour's brightness the eye beads keep.
pub const EYE_DEPTH: f32 = 0.7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyPart {
    Head,
    Body,
    Limb,
}

fn hsl(c: Rgb) -> (f32, f32, f32) {
    let (max, min) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    let l = (max + min) * 0.5;
    let d = max - min;
    if d < 1e-5 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-5);
    let h = if max == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if max == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    (h * 60.0, s.min(1.0), l)
}

/// Is this texture colour the rigs' painted skin?
pub fn is_skin(c: Rgb) -> bool {
    let (h, s, l) = hsl(c);
    (14.0..=34.0).contains(&h) && s >= 0.7 && l >= 0.55
}

fn luma(c: Rgb) -> f32 {
    c[0] * 0.30 + c[1] * 0.59 + c[2] * 0.11
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Vertex colour that turns texture colour `orig` into `want` (both sRGB).
fn multiplier(orig: Rgb, want: Rgb) -> [f32; 4] {
    let m = |o: f32, w: f32| (srgb_to_linear(w.clamp(0.0, 1.0)) / srgb_to_linear(o).max(0.002)).min(160.0);
    [m(orig[0], want[0]), m(orig[1], want[1]), m(orig[2], want[2]), 1.0]
}

/// The colour a skin vertex should end up, keeping the texture's shading.
fn skin_want(look: Look, orig: Rgb) -> Rgb {
    let tone = SKIN[look.skin as usize % SKIN.len()].1;
    [tone[0] * orig[0] / SKIN_REF[0], tone[1] * orig[1] / SKIN_REF[1], tone[2] * orig[2] / SKIN_REF[2]]
}

/// Vertex colours for one body-part mesh, and (for the head) its reshaped
/// positions. `texel[i]` is the texture colour under vertex `i`.
pub fn recolour(part: BodyPart, look: Look, pos: &mut [[f32; 3]], texel: &[Rgb]) -> Vec<[f32; 4]> {
    let look = look.clamped();
    let hair = HAIR_COLOR[look.hair_color as usize].1;
    let eye = EYES[look.eyes as usize].1;
    let dye = DYE[look.dye as usize].1;
    let paint = MARK_COLOR[look.mark_color as usize].1;
    let face = geom::face_shape(look.face);
    let in_eye = |p: [f32; 3], c: Rgb| hsl(c).2 < 0.25 && p[0].abs() < 0.30 && p[1] > 1.50 && p[1] < 1.70 && p[2] > 0.30;
    // Mean brightness of this head's own hair, so its shading carries over
    // whether it was painted blond or black.
    let mut hair_luma = (0.0f32, 0u32);
    if part == BodyPart::Head {
        for (p, c) in pos.iter().zip(texel) {
            if !is_skin(*c) && !in_eye(*p, *c) {
                hair_luma = (hair_luma.0 + luma(*c), hair_luma.1 + 1);
            }
        }
    }
    let hair_mean = if hair_luma.1 > 0 { (hair_luma.0 / hair_luma.1 as f32).max(0.02) } else { 0.3 };
    let mut out = Vec::with_capacity(pos.len());
    for (p, c) in pos.iter_mut().zip(texel) {
        let c = *c;
        let col = if is_skin(c) {
            let mut want = skin_want(look, c);
            if part == BodyPart::Head && look.marking != 0 {
                let w = geom::marking_weight(look.marking, Vec3::from(*p));
                if w > 0.0 {
                    let ink = if geom::is_scar(look.marking) {
                        // Scar tissue: paler and pinker than the skin around it.
                        mix(want, [0.86, 0.62, 0.60], 0.55)
                    } else {
                        paint
                    };
                    want = mix(want, ink, w * 0.88);
                }
            }
            multiplier(c, want)
        } else if part == BodyPart::Head {
            if in_eye(*p, c) {
                // The rigs' eyes are solid dark beads; a bright bead reads as
                // a sticker, so the colour is deepened a little.
                multiplier(c, [eye[0] * EYE_DEPTH, eye[1] * EYE_DEPTH, eye[2] * EYE_DEPTH])
            } else {
                let k = (luma(c) / hair_mean).clamp(0.72, 1.25);
                multiplier(c, [hair[0] * k, hair[1] * k, hair[2] * k])
            }
        } else if look.dye == 0 {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            let k = (luma(c) / 0.42).clamp(0.45, 1.6);
            multiplier(c, mix(c, [dye[0] * k, dye[1] * k, dye[2] * k], 0.78))
        };
        out.push(col);
        if part == BodyPart::Head {
            *p = geom::morph_face(face, Vec3::from(*p)).to_array();
        }
    }
    out
}

/// Nearest-texel lookup into an RGBA8 image; `None` for other formats.
fn texels(img: &Image, uv: &[[f32; 2]]) -> Option<Vec<Rgb>> {
    let srgb = match img.texture_descriptor.format {
        TextureFormat::Rgba8UnormSrgb => true,
        TextureFormat::Rgba8Unorm => false,
        _ => return None,
    };
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 || img.data.len() < w * h * 4 {
        return None;
    }
    Some(
        uv.iter()
            .map(|t| {
                let x = ((t[0].rem_euclid(1.0)) * w as f32) as usize % w;
                let y = ((t[1].rem_euclid(1.0)) * h as f32) as usize % h;
                let i = (y * w + x) * 4;
                let f = |b: u8| {
                    let v = b as f32 / 255.0;
                    if srgb {
                        v
                    } else {
                        // Linear data: bring it to sRGB so one classifier serves both.
                        if v <= 0.003_130_8 {
                            v * 12.92
                        } else {
                            1.055 * v.powf(1.0 / 2.4) - 0.055
                        }
                    }
                };
                [f(img.data[i]), f(img.data[i + 1]), f(img.data[i + 2])]
            })
            .collect(),
    )
}

fn part_of(node: &str) -> Option<BodyPart> {
    if node.ends_with("_Head") || node.ends_with("_Head_Hooded") {
        Some(BodyPart::Head)
    } else if node.ends_with("_Body") {
        Some(BodyPart::Body)
    } else if node.ends_with("_ArmLeft") || node.ends_with("_ArmRight") || node.ends_with("_LegLeft") || node.ends_with("_LegRight") {
        Some(BodyPart::Limb)
    } else {
        None
    }
}

/// Headgear and props that ship visible on the rigs and are not part of a look.
fn is_hidden_extra(node: &str) -> bool {
    node.ends_with("_Helmet") || node.ends_with("_Hat") || node == "Mug"
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn apply_looks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    images: Res<Assets<Image>>,
    mut assets: ResMut<LookAssets>,
    mut faces: ResMut<crate::face::RestyledFaces>,
    roots: Query<(Entity, &LookRig, Option<&LookApplied>)>,
    children_q: Query<&Children>,
    names: Query<&Name>,
    mesh_q: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>, Option<&Pristine>)>,
) {
    for (root, rig, applied) in &roots {
        if applied.is_some_and(|a| a.look == rig.look) {
            continue;
        }
        let look = rig.look.clamped();
        // Find the bones, the body-part meshes and the toggled nodes.
        let (mut head_bone, mut chest_bone) = (None, None);
        let mut parts: Vec<(Entity, BodyPart, String)> = Vec::new();
        let mut extras: Vec<Entity> = Vec::new();
        let mut capes: Vec<Entity> = Vec::new();
        let mut stack = vec![root];
        while let Some(ent) = stack.pop() {
            if let Ok(kids) = children_q.get(ent) {
                stack.extend(kids.iter().copied());
            }
            let Ok(name) = names.get(ent) else { continue };
            let n = name.as_str();
            if n == "head" {
                head_bone = Some(ent);
            } else if n == "chest" {
                chest_bone = Some(ent);
            } else if is_hidden_extra(n) {
                extras.push(ent);
            } else if n.ends_with("_Cape") {
                capes.push(ent);
            } else if let Some(part) = part_of(n) {
                if let Ok(kids) = children_q.get(ent) {
                    for k in kids.iter() {
                        if mesh_q.get(*k).is_ok() {
                            parts.push((*k, part, n.to_string()));
                        }
                    }
                }
            }
        }
        let (Some(head_bone), Some(chest_bone)) = (head_bone, chest_bone) else { continue };
        let Some((head_ent, _, head_node)) = parts.iter().find(|p| p.1 == BodyPart::Head).cloned() else { continue };
        // The texture, from the head (gear never replaces the head's material).
        let Ok((head_mesh, head_mat, head_pristine)) = mesh_q.get(head_ent) else { continue };
        let Some(tex) = materials.get(&head_mat.0).and_then(|m| m.base_color_texture.clone()) else { continue };
        let Some(img) = images.get(&tex) else { continue }; // still loading
        // Copy the head only after the face restyle (bigger nose, no mouth).
        let head_src = head_pristine.map(|p| p.0.clone()).unwrap_or_else(|| head_mesh.0.clone());
        if meshes.get(&head_src).is_none() {
            continue;
        }
        faces.ensure(&mut meshes, head_src.id(), &head_node);

        for (ent, part, _) in &parts {
            let Ok((cur, mat, pristine)) = mesh_q.get(*ent) else { continue };
            let src = pristine.map(|p| p.0.clone()).unwrap_or_else(|| cur.0.clone());
            // Gear that replaces the torso with a flat material (hide vest,
            // lineage mantle) must not be multiplied by texture-relative
            // colours: it keeps the rig's own mesh.
            let textured = materials.get(&mat.0).is_some_and(|m| m.base_color_texture.is_some());
            if !textured {
                commands.entity(*ent).try_insert((Mesh3d(src.clone()), Pristine(src)));
                continue;
            }
            let Some(orig) = meshes.get(&src) else { continue };
            let Some(VertexAttributeValues::Float32x2(uv)) = orig.attribute(Mesh::ATTRIBUTE_UV_0) else { continue };
            let Some(VertexAttributeValues::Float32x3(pos)) = orig.attribute(Mesh::ATTRIBUTE_POSITION) else { continue };
            let Some(tx) = texels(img, uv) else {
                continue; // unknown texture format: leave this part as painted
            };
            let mut pos = pos.clone();
            let colours = recolour(*part, look, &mut pos, &tx);
            let mut copy = orig.clone();
            copy.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
            if *part == BodyPart::Head {
                copy.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            }
            let handle = meshes.add(copy);
            if *part == BodyPart::Head {
                faces.mark(handle.id());
            }
            commands.entity(*ent).try_insert((Mesh3d(handle), Pristine(src)));
        }

        for e in extras {
            commands.entity(e).try_insert(Visibility::Hidden);
        }
        for e in capes {
            commands.entity(e).try_insert(if look.mantle == 1 { Visibility::Inherited } else { Visibility::Hidden });
        }

        // Grafts: replace whatever the previous look grew.
        if let Some(a) = applied {
            for g in &a.grafts {
                if let Some(ec) = commands.get_entity(*g) {
                    ec.despawn_recursive();
                }
            }
        }
        let mat = assets
            .graft
            .get_or_insert_with(|| {
                materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.9, ..default() })
            })
            .clone();
        let mut grafts = Vec::new();
        if let Some(m) = geom::head_graft(look) {
            let g = commands.spawn((Mesh3d(meshes.add(m)), MeshMaterial3d(mat.clone()), Transform::IDENTITY)).id();
            commands.entity(head_bone).add_child(g);
            grafts.push(g);
        }
        if let Some(m) = geom::mantle_graft(look) {
            let g = commands.spawn((Mesh3d(meshes.add(m)), MeshMaterial3d(mat.clone()), Transform::IDENTITY)).id();
            commands.entity(chest_bone).add_child(g);
            grafts.push(g);
        }
        commands.entity(root).try_insert(LookApplied { look: rig.look, grafts });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKIN_TEXEL: Rgb = [0.965, 0.757, 0.616];
    const CLOTH_TEXEL: Rgb = [0.0, 0.55, 0.33];
    const HAIR_TEXEL: Rgb = [0.51, 0.26, 0.19];
    const EYE_TEXEL: Rgb = [0.06, 0.07, 0.08];

    fn applied(orig: Rgb, m: [f32; 4]) -> Rgb {
        // What the shader shows: linear texture colour times vertex colour.
        [srgb_to_linear(orig[0]) * m[0], srgb_to_linear(orig[1]) * m[1], srgb_to_linear(orig[2]) * m[2]]
    }

    fn close(a: Rgb, want_srgb: Rgb) -> bool {
        (0..3).all(|i| (a[i] - srgb_to_linear(want_srgb[i])).abs() < 0.01)
    }

    #[test]
    fn classifier_knows_the_rigs_painted_colours() {
        // Sampled from the four GLBs (see the verification notes).
        for skin in [[0.969, 0.773, 0.635], [0.957, 0.710, 0.557], [0.953, 0.690, 0.533]] {
            assert!(is_skin(skin), "{skin:?}");
        }
        for not in [HAIR_TEXEL, CLOTH_TEXEL, EYE_TEXEL, [0.88, 0.72, 0.53], [0.54, 0.29, 0.22], [0.46, 0.49, 0.51]] {
            assert!(!is_skin(not), "{not:?}");
        }
    }

    #[test]
    fn skin_hair_eyes_and_dye_land_on_their_colours() {
        let look = Look { skin: 13, hair_color: 10, eyes: 10, dye: 14, ..Look::default() };
        // Head: one skin vertex on the cheek, one hair vertex, one eye vertex.
        let mut pos = vec![[0.3, 1.45, 0.35], [0.0, 2.0, -0.2], [0.17, 1.6, 0.4]];
        let tex = vec![SKIN_TEXEL, HAIR_TEXEL, EYE_TEXEL];
        let c = recolour(BodyPart::Head, look, &mut pos, &tex);
        assert!(close(applied(SKIN_TEXEL, c[0]), SKIN[13].1), "skin tone");
        assert!(close(applied(HAIR_TEXEL, c[1]), HAIR_COLOR[10].1), "hair colour");
        let e = EYES[10].1;
        assert!(close(applied(EYE_TEXEL, c[2]), [e[0] * EYE_DEPTH, e[1] * EYE_DEPTH, e[2] * EYE_DEPTH]), "eye colour");
        // Body: skin stays skin, cloth moves most of the way to the dye.
        let mut pos = vec![[0.0, 1.0, 0.3], [0.2, 0.9, 0.3]];
        let c = recolour(BodyPart::Body, look, &mut pos, &[SKIN_TEXEL, CLOTH_TEXEL]);
        assert!(close(applied(SKIN_TEXEL, c[0]), SKIN[13].1));
        let dyed = applied(CLOTH_TEXEL, c[1]);
        assert!(dyed[2] > dyed[1] && dyed[2] > dyed[0], "woad blue cloth is blue: {dyed:?}");
        assert_eq!(pos[1], [0.2, 0.9, 0.3], "only heads are reshaped");
        // Undyed cloth is left exactly as painted.
        let c = recolour(BodyPart::Limb, Look::default(), &mut pos, &[SKIN_TEXEL, CLOTH_TEXEL]);
        assert_eq!(c[1], [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn default_skin_is_the_painted_skin() {
        let mut pos = vec![[0.3, 1.45, 0.35]];
        let c = recolour(BodyPart::Limb, Look { skin: 2, ..Look::default() }, &mut pos, &[SKIN_TEXEL]);
        assert!((c[0][0] - 1.0).abs() < 0.02 && (c[0][1] - 1.0).abs() < 0.02 && (c[0][2] - 1.0).abs() < 0.02);
    }

    #[test]
    fn paint_and_scars_only_touch_their_region() {
        let look = Look { marking: 3, mark_color: 1, ..Look::default() }; // chin paint, woad
        let mut pos = vec![[0.0, 1.30, 0.40], [0.0, 1.80, 0.40]];
        let c = recolour(BodyPart::Head, look, &mut pos, &[SKIN_TEXEL, SKIN_TEXEL]);
        let chin = applied(SKIN_TEXEL, c[0]);
        let brow = applied(SKIN_TEXEL, c[1]);
        assert!(chin[2] > chin[0], "chin is painted blue: {chin:?}");
        assert!(brow[0] > brow[2], "forehead keeps its skin: {brow:?}");
        // A scar lightens dark skin instead of taking the paint colour.
        let scar = Look { marking: 18, mark_color: 1, skin: 14, ..Look::default() };
        let mut pos = vec![[-0.3, 1.40, 0.40], [0.3, 1.40, 0.40]];
        let c = recolour(BodyPart::Head, scar, &mut pos, &[SKIN_TEXEL, SKIN_TEXEL]);
        let (burnt, clear) = (applied(SKIN_TEXEL, c[0]), applied(SKIN_TEXEL, c[1]));
        assert!(burnt[0] > clear[0] * 1.5 && burnt[0] > burnt[2], "{burnt:?} vs {clear:?}");
    }

    #[test]
    fn part_names_match_the_rig_nodes() {
        assert_eq!(part_of("Knight_Head"), Some(BodyPart::Head));
        assert_eq!(part_of("Rogue_Head_Hooded"), Some(BodyPart::Head));
        assert_eq!(part_of("Mage_Body"), Some(BodyPart::Body));
        assert_eq!(part_of("Barbarian_ArmLeft"), Some(BodyPart::Limb));
        assert_eq!(part_of("Rogue_LegRight"), Some(BodyPart::Limb));
        assert_eq!(part_of("Knight_Cape"), None);
        assert_eq!(part_of("1H_Sword"), None);
        assert!(is_hidden_extra("Knight_Helmet") && is_hidden_extra("Mage_Hat") && is_hidden_extra("Mug"));
        assert!(!is_hidden_extra("Knight_Head"));
    }
}
