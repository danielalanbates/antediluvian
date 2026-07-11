//! Points of interest (CHUNK C04).
//!
//! `assets/data/pois.json` (from `scripts/gen_pois.py`, 40 per act) is
//! embedded at compile time. Walking within `POI_RADIUS` of an undiscovered
//! POI announces it and grants discovery XP.

use antediluvia_protocol::Act;
use std::sync::OnceLock;

#[derive(serde::Deserialize)]
pub struct PoiDef {
    pub id: u32,
    pub name: String,
    pub act: String,
    pub x: f32,
    pub y: f32,
}

pub const POI_RADIUS: f32 = 120.0;

static POIS: OnceLock<Vec<PoiDef>> = OnceLock::new();

pub fn all_pois() -> &'static [PoiDef] {
    POIS.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/data/pois.json"))
            .expect("pois.json parses")
    })
}

pub fn pois_for_act(act: Act) -> impl Iterator<Item = &'static PoiDef> {
    let key = act.as_str();
    all_pois().iter().filter(move |p| p.act == key)
}

/// Deterministic anchor point for later systems (quest camps, cave mouths).
#[allow(dead_code)] // consumed by C09+
pub fn poi_near(act: Act, seed: u64) -> Option<&'static PoiDef> {
    let list: Vec<_> = pois_for_act(act).collect();
    if list.is_empty() {
        return None;
    }
    Some(list[(seed as usize) % list.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pois_load_and_cover_every_act() {
        assert_eq!(all_pois().len(), 200);
        for act in Act::ALL {
            let n = pois_for_act(act).count();
            assert_eq!(n, 40, "{act:?} POI count");
            for p in pois_for_act(act) {
                assert!(p.x.abs() <= 3200.0 && p.y.abs() <= 3200.0, "{} out of bounds", p.name);
            }
        }
        assert!(poi_near(Act::Eden, 7).is_some());
    }
}
