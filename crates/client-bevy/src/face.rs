//! Player-face restyle: a bigger rounded nose and no mouth.
//!
//! The KayKit adventurer faces are GEOMETRY, not texture — the atlas is a
//! palette of flat colour cells. The mouth is a V-fold of duplicated vertices
//! just under the nose (upper lip normals face down, lower face up), which
//! reads as a dark line. So the restyle edits the head mesh once per asset:
//! - vertices in the mouth box are pulled onto a smooth cheek profile fitted
//!   to the rows directly above and below the fold, with forward normals, so
//!   neither the groove nor its shading line remains;
//! - vertices of the nose are pushed out from the nose centre.
//!
//! Coordinates are head-node local and identical across the four bodies and
//! the hooded NPC (all built on the same "PrototypePete" base head).

use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;
use std::collections::HashSet;

/// Mouth fold: under the nose tip (y 1.44+) and above the chin row (y 1.29).
const MOUTH_Y: (f32, f32) = (1.33, 1.435);
const MOUTH_HALF_W: f32 = 0.245;
/// Nose volume and the point it grows from (its base on the face plane).
const NOSE_Y: (f32, f32) = (1.435, 1.535);
const NOSE_HALF_W: f32 = 0.15;
const NOSE_MIN_Z: f32 = 0.415;
const NOSE_CENTRE: Vec3 = Vec3::new(0.0, 1.475, 0.42);
pub const NOSE_GROW: f32 = 1.55;

#[derive(Resource, Default)]
pub struct RestyledFaces(HashSet<AssetId<Mesh>>);

/// Cheek surface depth at (x, y), fitted to the chin row (y 1.29: z .395 at
/// centre, .352 at x .16) and the under-eye row (y 1.50: .444 / .385 at .22).
fn cheek_z(x: f32, y: f32) -> f32 {
    let t = ((y - 1.29) / 0.21).clamp(0.0, 1.0);
    let zc = 0.395 + (0.444 - 0.395) * t;
    let k = 1.68 + (1.22 - 1.68) * t;
    zc - k * x * x
}

/// Returns true when the mesh was a face and got restyled.
pub fn restyle_head(mesh: &mut Mesh, bearded: bool) -> bool {
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).cloned() else {
        return false;
    };
    let mut pos = pos;
    let mut nrm = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(n)) => n.clone(),
        _ => return false,
    };
    let mut touched = false;
    for (p, n) in pos.iter_mut().zip(nrm.iter_mut()) {
        let [x, y, z] = *p;
        let in_mouth = y > MOUTH_Y.0 && y <= MOUTH_Y.1 && x.abs() < MOUTH_HALF_W && z > 0.33;
        let in_nose = y > NOSE_Y.0 && y < NOSE_Y.1 && x.abs() < NOSE_HALF_W && z > NOSE_MIN_Z;
        if in_mouth {
            // The beard covers the Barbarian's mouth with its own volume;
            // moving those vertices would dent the beard, so only the
            // shading crease is removed there.
            if !bearded {
                p[2] = cheek_z(x, y);
            }
            let f = Vec3::new(x * 3.2, 0.12, 1.0).normalize();
            *n = f.to_array();
            touched = true;
        } else if in_nose && !bearded {
            // The Barbarian's nose is already the big rounded one; its box
            // also overlaps his moustache.
            let v = Vec3::from(*p);
            let grown = NOSE_CENTRE + (v - NOSE_CENTRE) * NOSE_GROW;
            *p = grown.to_array();
            touched = true;
        }
    }
    if touched {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    }
    touched
}

pub fn head_is_bearded(node: &str) -> bool {
    node.to_ascii_lowercase().starts_with("barbarian")
}

/// Head primitives are children of a node named `<Body>_Head` (or
/// `Rogue_Head_Hooded`). Each mesh asset is edited once and shared by every
/// instance of that body.
pub fn restyle_faces(
    mut done: ResMut<RestyledFaces>,
    mut meshes: ResMut<Assets<Mesh>>,
    prims: Query<(&Mesh3d, &Parent), Added<Mesh3d>>,
    names: Query<&Name>,
) {
    for (m, parent) in &prims {
        let id = m.0.id();
        if done.0.contains(&id) {
            continue;
        }
        let Ok(name) = names.get(parent.get()) else { continue };
        let n = name.as_str();
        if !(n.ends_with("_Head") || n.ends_with("_Head_Hooded")) {
            continue;
        }
        let Some(mesh) = meshes.get_mut(id) else { continue };
        restyle_head(mesh, head_is_bearded(n));
        done.0.insert(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::mesh::PrimitiveTopology;
    use bevy::render::render_asset::RenderAssetUsages;

    fn mesh(points: Vec<[f32; 3]>) -> Mesh {
        let n = vec![[0.0, 0.0, 1.0]; points.len()];
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, n)
    }

    fn positions(m: &Mesh) -> Vec<[f32; 3]> {
        m.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap().to_vec()
    }

    #[test]
    fn mouth_fold_is_flattened_and_nose_grows() {
        // Real Rogue vertices: a mouth fold vertex, the nose tip, an eye.
        let mut m = mesh(vec![[0.05, 1.42, 0.417], [0.02, 1.52, 0.519], [0.17, 1.54, 0.388]]);
        assert!(restyle_head(&mut m, false));
        let p = positions(&m);
        assert!((p[0][2] - cheek_z(0.05, 1.42)).abs() < 1e-5, "mouth not flattened");
        assert!(p[1][2] > 0.519 + 0.03, "nose did not grow: {}", p[1][2]);
        assert_eq!(p[2], [0.17, 1.54, 0.388], "eye must be untouched");
    }

    #[test]
    fn beard_keeps_its_shape() {
        let mut m = mesh(vec![[0.1, 1.41, 0.46]]);
        restyle_head(&mut m, true);
        assert_eq!(positions(&m)[0], [0.1, 1.41, 0.46]);
    }
}
