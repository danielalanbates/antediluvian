//! Caves & mines (CHUNK C09).
//!
//! `assets/data/caves.json` (from `scripts/gen_caves.py`, 6 per act) is
//! embedded at compile time. A cave is a surface pocket: inside
//! `CAVE_RADIUS` of its center the zone seeds rich ore nodes
//! (`ore_<resource>`, double yield), tougher mobs from one act tier up and
//! a `<resource>_dweller` mini-boss. Entering an undiscovered cave counts
//! as a POI-style discovery.

use antediluvia_protocol::Act;
use std::sync::OnceLock;

#[derive(serde::Deserialize)]
pub struct CaveDef {
    pub id: u32,
    pub name: String,
    pub act: String,
    pub x: f32,
    pub y: f32,
    pub resources: Vec<String>,
}

pub const CAVE_RADIUS: f32 = 260.0;

static CAVES: OnceLock<Vec<CaveDef>> = OnceLock::new();

pub fn all_caves() -> &'static [CaveDef] {
    CAVES.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/data/caves.json"))
            .expect("caves.json parses")
    })
}

pub fn caves_for_act(act: Act) -> impl Iterator<Item = &'static CaveDef> {
    let key = act.as_str();
    all_caves().iter().filter(move |c| c.act == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caves_load_and_cover_every_act() {
        assert_eq!(all_caves().len(), 30);
        for act in Act::ALL {
            let n = caves_for_act(act).count();
            assert_eq!(n, 6, "{act:?} cave count");
            for c in caves_for_act(act) {
                assert!(c.x.abs() <= 3200.0 && c.y.abs() <= 3200.0, "{} out of bounds", c.name);
                assert!(!c.resources.is_empty(), "{} has no resources", c.name);
                assert!(c.x.hypot(c.y) > 650.0, "{} overlaps the inn safe area", c.name);
            }
        }
    }
}
