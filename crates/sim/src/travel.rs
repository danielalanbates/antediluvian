// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! B08 travel: flight masters, offline rested XP, graveyards with spirit
//! release, and the zone-in banner regions.
//!
//! * **Flight** - every act has a flight master at its inn plus four at
//!   major POIs (one per quarter of the map). A node is *known* once the
//!   character has been there: POI nodes ride the existing POI discovery
//!   list, inn nodes are added to it the first time the character asks the
//!   flight master for routes. Flying needs both ends known and costs gold,
//!   which leaves the economy.
//! * **Rested XP** - `accrue_offline_rest` banks rest for time spent logged
//!   out (full rate if the character logged out at an inn, a quarter in the
//!   wild). Live accrual while standing in an inn already exists in the sim.
//! * **Graveyards** - a dead character may `release_spirit` to rise at the
//!   nearest graveyard at half health instead of waiting for the inn respawn.
//! * **Regions** - `region_at` names where a character stands so the server
//!   can send `ZoneBanner` on entry.
//!
//! No new sheet fields: everything here reuses `discovered`, `rested_xp`,
//! `last_logout` and `gold`.

use crate::world::{pvp_zones, World, INN_RADIUS};
use antediluvia_protocol::{Act, CharacterSheet, EntityId, FlightNode};
use glam::Vec2;
use std::sync::OnceLock;

/// Stand this close to a POI flight master to use it (inns use INN_RADIUS).
pub const FLIGHT_RADIUS: f32 = 160.0;
/// Rested XP per hour logged out at an inn, as a percentage of `max_xp`.
pub const OFFLINE_INN_PCT_PER_HOUR: u64 = 5;
/// Logging out in the wild rests at this fraction of the inn rate.
pub const OFFLINE_WILD_DIVISOR: u64 = 4;
/// Offline time shorter than this banks nothing (relog spam).
pub const OFFLINE_MIN_SECS: u64 = 60;

pub struct FlightNodeDef {
    pub id: &'static str,
    pub name: &'static str,
    pub act: Act,
    pub pos: Vec2,
    pub inn: bool,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn act_index(act: Act) -> usize {
    Act::ALL.iter().position(|a| *a == act).unwrap_or(0)
}

fn act_title(act: Act) -> &'static str {
    match act {
        Act::Eden => "Eden and Nod",
        Act::Hermon => "The Hermon Range",
        Act::Nephilim => "The Nephilim Wastes",
        Act::Enoch => "The Sprawl of Enoch",
        Act::Flood => "The Abyssal Basins",
    }
}

/// Display (and discovery-list) name of an act's inn.
pub fn inn_name(act: Act) -> &'static str {
    match act {
        Act::Eden => "Inn of Eden",
        Act::Hermon => "Inn of Hermon",
        Act::Nephilim => "Inn of the Wastes",
        Act::Enoch => "Inn of Enoch",
        Act::Flood => "Inn of the Flood",
    }
}

const QUARTERS: [(f32, f32); 4] = [(1900.0, -1900.0), (-1900.0, -1900.0), (-1900.0, 1900.0), (1900.0, 1900.0)];

/// All flight masters: 5 inns + 4 major POIs per act.
pub fn flight_nodes() -> &'static [FlightNodeDef] {
    static NODES: OnceLock<Vec<FlightNodeDef>> = OnceLock::new();
    NODES.get_or_init(|| {
        let mut out = Vec::new();
        for act in Act::ALL {
            out.push(FlightNodeDef {
                id: leak(format!("inn_{}", act.as_str())),
                name: inn_name(act),
                act,
                pos: Vec2::ZERO,
                inn: true,
            });
            let pois: Vec<_> = crate::pois::pois_for_act(act).collect();
            let mut used: Vec<u32> = Vec::new();
            for (qx, qy) in QUARTERS {
                let q = Vec2::new(qx, qy);
                // A node's name is its discovery key, so it must be unique
                // within the act (POI names repeat).
                let pick = pois
                    .iter()
                    .filter(|p| !used.contains(&p.id))
                    .filter(|p| pois.iter().filter(|o| o.name == p.name).count() == 1)
                    // Review fix: discovery is recorded by NAME with no act,
                    // so a node name must be unique across every act's POIs
                    // and caves, or visiting a namesake elsewhere would
                    // "discover" a flight master the character never reached.
                    .filter(|p| {
                        crate::pois::all_pois().iter().filter(|o| o.name == p.name).count() == 1
                            && !crate::caves::all_caves().iter().any(|c| c.name == p.name)
                    })
                    .min_by(|a, b| Vec2::new(a.x, a.y).distance(q).total_cmp(&Vec2::new(b.x, b.y).distance(q)));
                if let Some(p) = pick {
                    used.push(p.id);
                    out.push(FlightNodeDef {
                        id: leak(format!("fp_{}", p.id)),
                        name: p.name.as_str(),
                        act,
                        pos: Vec2::new(p.x, p.y),
                        inn: false,
                    });
                }
            }
        }
        out
    })
}

pub fn flight_node(id: &str) -> Option<&'static FlightNodeDef> {
    flight_nodes().iter().find(|n| n.id == id)
}

fn node_radius(n: &FlightNodeDef) -> f32 {
    if n.inn { INN_RADIUS } else { FLIGHT_RADIUS }
}

/// The flight master the position stands at, if any.
pub fn node_at(act: Act, pos: Vec2) -> Option<&'static FlightNodeDef> {
    flight_nodes()
        .iter()
        .filter(|n| n.act == act && n.pos.distance(pos) <= node_radius(n))
        .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos)))
}

pub fn node_known(sheet: &CharacterSheet, n: &FlightNodeDef) -> bool {
    sheet.discovered.iter().any(|d| d == n.name)
}

/// Gold fare between two nodes: distance within an act, plus a flat toll
/// per act crossed.
pub fn flight_cost(from: &FlightNodeDef, to: &FlightNodeDef) -> u32 {
    if from.id == to.id {
        return 0;
    }
    let hops = (act_index(from.act) as i32 - act_index(to.act) as i32).unsigned_abs();
    if hops == 0 {
        2 + (from.pos.distance(to.pos) / 400.0).round() as u32
    } else {
        10 + 15 * hops + ((from.pos.length() + to.pos.length()) / 400.0).round() as u32
    }
}

/// Most rested XP a character can hold from being offline.
pub fn rested_cap(sheet: &CharacterSheet) -> u32 {
    ((sheet.max_xp as u64 * 3 / 2).min(u32::MAX as u64) as u32).max(2000)
}

/// Bank rested XP for the time since `last_logout`. Call once at login,
/// before the sheet is spawned. Returns the amount gained.
pub fn accrue_offline_rest(sheet: &mut CharacterSheet, now_secs: u64) -> u32 {
    let Some(last) = sheet.last_logout else { return 0 };
    let secs = now_secs.saturating_sub(last);
    if secs < OFFLINE_MIN_SECS {
        return 0;
    }
    let cap = rested_cap(sheet);
    if sheet.rested_xp >= cap {
        return 0;
    }
    let at_inn = Vec2::new(sheet.x, sheet.y).length() <= INN_RADIUS;
    let divisor = if at_inn { 1 } else { OFFLINE_WILD_DIVISOR };
    // max_xp * pct/100 per hour, in u128 so month-long absences cannot overflow.
    let gain = (sheet.max_xp as u128 * OFFLINE_INN_PCT_PER_HOUR as u128 * secs as u128) / (100 * 3600 * divisor as u128);
    let room = (cap - sheet.rested_xp) as u128;
    let gain = gain.min(room) as u32;
    sheet.rested_xp += gain;
    gain
}

/// Graveyards of an act: the inn shrine plus one in each quarter.
pub fn graveyards(act: Act) -> [(&'static str, Vec2); 5] {
    let shrine = match act {
        Act::Eden => "the shrine at the Inn of Eden",
        Act::Hermon => "the shrine at the Inn of Hermon",
        Act::Nephilim => "the shrine at the Inn of the Wastes",
        Act::Enoch => "the shrine at the Inn of Enoch",
        Act::Flood => "the shrine at the Inn of the Flood",
    };
    [
        (shrine, Vec2::ZERO),
        ("the eastern cairns", Vec2::new(1700.0, -1700.0)),
        ("the western cairns", Vec2::new(-1700.0, 1700.0)),
        ("the Sethite burial ground", Vec2::new(-1500.0, -1100.0)),
        ("the Cainite barrows", Vec2::new(1500.0, 1100.0)),
    ]
}

pub fn nearest_graveyard(act: Act, pos: Vec2) -> (&'static str, Vec2) {
    let g = graveyards(act);
    *g.iter().min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos))).unwrap()
}

/// A named place, for the zone-in banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    /// Stable identity ("act:eden", "inn:eden", "poi:12", "cave:3", "pvp:...").
    pub key: String,
    pub name: String,
    pub subtitle: String,
    pub level_min: u32,
    pub level_max: u32,
    /// Standing here force-flags this character for PvP.
    pub pvp: bool,
}

/// Where a character of the given lineage stands. Capitals win over inns,
/// caves and POIs; open country is the act itself.
pub fn region_at(act: Act, pos: Vec2, faction: Option<&str>) -> Region {
    let (lo, hi) = crate::mobs::act_band(act);
    let mk = |key: String, name: &str, subtitle: String, pvp: bool| Region {
        key,
        name: name.to_string(),
        subtitle,
        level_min: lo,
        level_max: hi,
        pvp,
    };
    if let Some(z) = pvp_zones(act).find(|z| Vec2::new(z.x, z.y).distance(pos) <= z.radius) {
        let hostile = faction != Some(z.faction.as_str());
        let subtitle = if hostile {
            format!("{} capital - hostile ground, you are flagged for PvP", crate::questgen_title(&z.faction))
        } else {
            format!("{} capital - your own people; no flag here", crate::questgen_title(&z.faction))
        };
        return mk(format!("pvp:{}:{}", act.as_str(), z.faction), &z.name, subtitle, hostile);
    }
    if pos.length() <= INN_RADIUS {
        return mk(format!("inn:{}", act.as_str()), inn_name(act), "Sanctuary - rest here to bank rested XP".into(), false);
    }
    if let Some(i) = crate::questgen::anchor_at(act, pos) {
        let a = &crate::questgen::anchors()[i];
        let subtitle = if a.cave { "Cave - rich ore, and a Dweller below" } else { "Point of interest - a notice board stands here" };
        return mk(a.key.to_string(), a.name, subtitle.into(), false);
    }
    mk(format!("act:{}", act.as_str()), act_title(act), format!("Levels {lo}-{hi}"), false)
}

/// Result of a flight: where the character now is (the entity id changes
/// when the flight crosses acts, exactly like `Travel`).
#[derive(Debug, Clone, PartialEq)]
pub struct Flight {
    pub act: Act,
    pub entity: EntityId,
    pub cost: u32,
    pub text: String,
}

impl World {
    /// Routes as this character sees them. Asking at an inn discovers that
    /// inn's flight master.
    pub fn flight_list(&mut self, act: Act, id: EntityId) -> Result<(Option<String>, Vec<FlightNode>), String> {
        let e = self.zones.get_mut(&act).and_then(|z| z.entities.get_mut(&id)).ok_or("no such player")?;
        let pos = e.pos;
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        let here = node_at(act, pos);
        if let Some(h) = here {
            if h.inn && !node_known(s, h) {
                s.discovered.push(h.name.to_string());
            }
        }
        let nodes = flight_nodes()
            .iter()
            .map(|n| FlightNode {
                id: n.id.to_string(),
                name: n.name.to_string(),
                act: n.act,
                x: n.pos.x,
                y: n.pos.y,
                known: node_known(s, n),
                cost: here.map_or(0, |h| flight_cost(h, n)),
            })
            .collect();
        Ok((here.map(|h| h.id.to_string()), nodes))
    }

    /// Fly from the flight master the character stands at to a known node.
    pub fn flight_take(&mut self, act: Act, id: EntityId, node: &str) -> Result<Flight, String> {
        let dest = flight_node(node).ok_or("The flight master knows no such place.")?;
        let e = self.zones.get_mut(&act).and_then(|z| z.entities.get_mut(&id)).ok_or("no such player")?;
        if e.health <= 0 {
            return Err("You are dead.".into());
        }
        if e.combat_timer > 0.0 {
            return Err("You cannot take flight while in combat.".into());
        }
        let here = node_at(act, e.pos).ok_or("There is no flight master here. Find one at an inn or a major landmark.")?;
        // Review fix: everything that can refuse the flight is checked
        // before the fare is taken (this used to fail after charging).
        if dest.act != act && e.owner.is_none() {
            return Err("no owner".into());
        }
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        if here.inn && !node_known(s, here) {
            s.discovered.push(here.name.to_string());
        }
        if !node_known(s, here) {
            return Err("You have not discovered this flight master yet.".into());
        }
        if dest.id == here.id {
            return Err("You are already there.".into());
        }
        if !node_known(s, dest) {
            return Err(format!("You have not discovered {} - walk there first.", dest.name));
        }
        let cost = flight_cost(here, dest);
        if s.gold < cost {
            return Err(format!("The flight costs {cost} gold; you have {}.", s.gold));
        }
        s.gold -= cost;
        // Land just beside the master, inside its radius.
        let land = dest.pos + Vec2::new(24.0, 18.0);
        e.intent = Vec2::ZERO;
        e.mounted = false;
        let text = format!("You fly to {} for {cost}g.", dest.name);
        let entity = if dest.act == act {
            e.pos = land;
            e.origin = land;
            id
        } else {
            let owner = e.owner.ok_or("no owner")?;
            let mut sheet = self.player_sheet(act, id).ok_or("no sheet")?;
            self.remove_player(act, id);
            sheet.act = dest.act;
            sheet.x = land.x;
            sheet.y = land.y;
            self.spawn_player(owner, sheet)
        };
        // A flight jumps many AoI cells at once: refile now, not a tick later.
        if let Some(z) = self.zones.get_mut(&dest.act) {
            z.refile_entity(entity);
        }
        self.gold_sunk += cost as u64;
        Ok(Flight { act: dest.act, entity, cost, text })
    }

    /// While dead: rise now at the nearest graveyard at half health rather
    /// than waiting for the automatic inn/home respawn.
    pub fn release_spirit(&mut self, act: Act, id: EntityId) -> Result<String, String> {
        let e = self.zones.get_mut(&act).and_then(|z| z.entities.get_mut(&id)).ok_or("no such player")?;
        if e.health > 0 {
            return Err("You are not dead.".into());
        }
        let (name, pos) = nearest_graveyard(act, e.pos);
        e.pos = pos;
        e.origin = pos;
        e.health = (e.max_health / 2).max(1);
        e.dead_timer = 0.0;
        e.combat_timer = 0.0;
        e.intent = Vec2::ZERO;
        e.attack_queued = false;
        e.cast_queued = None;
        if let Some(s) = e.sheet.as_mut() {
            s.health = e.health;
        }
        // The graveyard may be cells away from the corpse: refile now.
        if let Some(z) = self.zones.get_mut(&act) {
            z.refile_entity(id);
        }
        Ok(format!("You release your spirit and rise at {name}, weakened."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{new_character, SimEvent};
    use antediluvia_protocol::EntityKind;

    fn spawn(w: &mut World, act: Act, pos: Vec2, gold: u32) -> EntityId {
        let mut s = new_character("Flyer");
        s.act = act;
        s.x = pos.x;
        s.y = pos.y;
        s.gold = gold;
        s.max_health = 1_000_000;
        s.health = 1_000_000;
        w.spawn_player(3, s)
    }

    fn sheet(w: &World, act: Act, id: EntityId) -> CharacterSheet {
        w.player_sheet(act, id).unwrap()
    }

    #[test]
    fn every_act_has_an_inn_and_four_landmark_masters() {
        assert_eq!(flight_nodes().len(), 25);
        let mut ids = std::collections::HashSet::new();
        for act in Act::ALL {
            let here: Vec<_> = flight_nodes().iter().filter(|n| n.act == act).collect();
            assert_eq!(here.iter().filter(|n| n.inn).count(), 1, "{act:?}");
            assert_eq!(here.iter().filter(|n| !n.inn).count(), 4, "{act:?}");
            let names: std::collections::HashSet<_> = here.iter().map(|n| n.name).collect();
            assert_eq!(names.len(), 5, "{act:?}: node names must be unique discovery keys");
            for n in &here {
                assert!(ids.insert(n.id));
                if !n.inn {
                    assert!(n.pos.length() > 900.0, "{}: a landmark master should be a real journey from the inn", n.id);
                    assert_eq!(crate::pois::pois_for_act(act).filter(|p| p.name == n.name).count(), 1);
                }
                assert_eq!(node_at(act, n.pos).map(|x| x.id), Some(n.id));
            }
        }
        // Fares: free to stay, dearer with distance, dearest across acts.
        let inn = flight_node("inn_eden").unwrap();
        let far = flight_nodes().iter().find(|n| n.act == Act::Eden && !n.inn).unwrap();
        let other = flight_node("inn_flood").unwrap();
        assert_eq!(flight_cost(inn, inn), 0);
        assert!(flight_cost(inn, far) >= 4 && flight_cost(inn, far) <= 30);
        assert_eq!(flight_cost(inn, far), flight_cost(far, inn));
        assert!(flight_cost(inn, other) > flight_cost(inn, far));
        assert_eq!(flight_cost(inn, other), 10 + 15 * 4);
    }

    #[test]
    fn discover_then_fly_within_an_act() {
        let mut w = World::new(21);
        let dest = flight_nodes().iter().find(|n| n.act == Act::Eden && !n.inn).unwrap();
        let id = spawn(&mut w, Act::Eden, Vec2::new(10.0, 10.0), 100);
        // At the inn: the inn's master becomes known; the landmark is not.
        let (here, nodes) = w.flight_list(Act::Eden, id).unwrap();
        assert_eq!(here.as_deref(), Some("inn_eden"));
        assert!(nodes.iter().find(|n| n.id == "inn_eden").unwrap().known);
        let row = nodes.iter().find(|n| n.id == dest.id).unwrap();
        assert!(!row.known && row.cost == flight_cost(flight_node("inn_eden").unwrap(), dest));
        let err = w.flight_take(Act::Eden, id, dest.id).unwrap_err();
        assert!(err.contains("not discovered"), "{err}");
        assert_eq!(sheet(&w, Act::Eden, id).gold, 100, "a refused flight is free");
        assert!(w.flight_take(Act::Eden, id, "inn_eden").unwrap_err().contains("already"));
        assert!(w.flight_take(Act::Eden, id, "nowhere").is_err());
        // Walk there (teleport for the test): the sim's POI discovery fires.
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().pos = dest.pos;
        for _ in 0..5 {
            w.step();
        }
        assert!(node_known(&sheet(&w, Act::Eden, id), dest), "POI discovery makes the node known");
        // Fly back to the inn, then out again: gold falls by the fare each way.
        let fare = flight_cost(dest, flight_node("inn_eden").unwrap());
        let sunk0 = w.gold_sunk;
        let f = w.flight_take(Act::Eden, id, "inn_eden").unwrap();
        assert_eq!((f.act, f.entity, f.cost), (Act::Eden, id, fare));
        assert!(w.player_pos(Act::Eden, id).unwrap().length() <= INN_RADIUS);
        assert_eq!(sheet(&w, Act::Eden, id).gold, 100 - fare);
        let f2 = w.flight_take(Act::Eden, id, dest.id).unwrap();
        assert_eq!(f2.cost, fare);
        assert!(w.player_pos(Act::Eden, id).unwrap().distance(dest.pos) <= FLIGHT_RADIUS);
        assert_eq!(sheet(&w, Act::Eden, id).gold, 100 - 2 * fare);
        assert_eq!(w.gold_sunk, sunk0 + 2 * fare as u64, "fares are a gold sink");
    }

    /// Review regression (merge of the AoI grid with the travel track): a
    /// flight or a spirit release moves the character outside the tick, so
    /// the grid must be refiled at once or the character is missing from
    /// its own visibility set until the next step.
    #[test]
    fn flight_and_spirit_release_refile_the_aoi_grid_at_once() {
        let mut w = World::new(23);
        let dest = flight_nodes()
            .iter()
            .filter(|n| n.act == Act::Eden && !n.inn)
            .max_by(|a, b| a.pos.length().total_cmp(&b.pos.length()))
            .unwrap();
        assert_ne!(
            crate::aoi::cell_of(dest.pos.x, dest.pos.y),
            crate::aoi::cell_of(10.0, 10.0),
            "the test needs a destination in another cell"
        );
        let id = spawn(&mut w, Act::Eden, Vec2::new(10.0, 10.0), 5000);
        {
            let s = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().sheet.as_mut().unwrap();
            s.discovered.push(dest.name.to_string());
        }
        w.flight_take(Act::Eden, id, dest.id).unwrap();
        assert!(w.visible_entities(Act::Eden, id, 60.0).contains(&id), "stale cell after a flight");
        // Die out there, release: the graveyard is wherever it is; the
        // character must be findable around its new position immediately.
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().health = 0;
        w.release_spirit(Act::Eden, id).unwrap();
        assert!(w.visible_entities(Act::Eden, id, 60.0).contains(&id), "stale cell after a spirit release");
    }

    #[test]
    fn flight_refusals_gold_combat_and_wilderness() {
        let mut w = World::new(22);
        let dest = flight_nodes().iter().find(|n| n.act == Act::Eden && !n.inn).unwrap();
        let id = spawn(&mut w, Act::Eden, Vec2::new(10.0, 10.0), 0);
        {
            let s = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().sheet.as_mut().unwrap();
            s.discovered.push(dest.name.to_string());
        }
        let err = w.flight_take(Act::Eden, id, dest.id).unwrap_err();
        assert!(err.contains("costs"), "{err}");
        assert!(w.player_pos(Act::Eden, id).unwrap().length() < 50.0, "no gold, no flight");
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().sheet.as_mut().unwrap().gold = 500;
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().combat_timer = 3.0;
        assert!(w.flight_take(Act::Eden, id, dest.id).unwrap_err().contains("combat"));
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().combat_timer = 0.0;
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().pos = Vec2::new(700.0, 40.0);
        assert!(w.flight_take(Act::Eden, id, dest.id).unwrap_err().contains("no flight master"));
        assert_eq!(sheet(&w, Act::Eden, id).gold, 500);
        let (here, _) = w.flight_list(Act::Eden, id).unwrap();
        assert!(here.is_none());
    }

    #[test]
    fn flying_between_acts_moves_the_character_once() {
        let mut w = World::new(23);
        let id = spawn(&mut w, Act::Eden, Vec2::new(0.0, 20.0), 200);
        // Unknown far inn: refused.
        assert!(w.flight_take(Act::Eden, id, "inn_hermon").is_err());
        {
            let s = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().sheet.as_mut().unwrap();
            s.discovered.push(inn_name(Act::Hermon).to_string());
            s.inventory.push("bread".into());
        }
        let before = sheet(&w, Act::Eden, id);
        let f = w.flight_take(Act::Eden, id, "inn_hermon").unwrap();
        assert_eq!(f.act, Act::Hermon);
        assert_ne!(f.entity, id, "crossing acts respawns under a new entity id");
        assert!(w.zones[&Act::Eden].entities.get(&id).is_none(), "left Eden");
        let players = |w: &World, act: Act| w.zones[&act].entities.values().filter(|e| e.kind == EntityKind::Player).count();
        assert_eq!((players(&w, Act::Eden), players(&w, Act::Hermon)), (0, 1), "exactly one copy of the character");
        let after = sheet(&w, Act::Hermon, f.entity);
        assert_eq!(after.gold, before.gold - f.cost);
        assert_eq!(f.cost, 25);
        assert_eq!(after.inventory, before.inventory);
        assert_eq!(after.act, Act::Hermon);
        assert!(Vec2::new(after.x, after.y).length() <= INN_RADIUS);
    }

    #[test]
    fn offline_rest_accrues_faster_at_an_inn_and_caps() {
        let mut inn = new_character("Sleeper");
        inn.x = 0.0;
        inn.y = 0.0;
        inn.max_xp = 1000;
        inn.last_logout = Some(1_000_000);
        let mut wild = inn.clone();
        wild.x = 2000.0;
        // 8 hours: inn 5%/h of 1000 = 400; wild a quarter of that.
        assert_eq!(accrue_offline_rest(&mut inn, 1_000_000 + 8 * 3600), 400);
        assert_eq!(accrue_offline_rest(&mut wild, 1_000_000 + 8 * 3600), 100);
        assert_eq!((inn.rested_xp, wild.rested_xp), (400, 100));
        // Short absences, clock skew and never-logged-out bank nothing.
        let mut s = new_character("Blink");
        assert_eq!(accrue_offline_rest(&mut s, 5_000_000), 0);
        s.last_logout = Some(5_000_000);
        assert_eq!(accrue_offline_rest(&mut s, 5_000_030), 0);
        assert_eq!(accrue_offline_rest(&mut s, 4_000_000), 0);
        // A year away stops at the cap, exactly, and a second login adds nothing.
        let cap = rested_cap(&inn);
        assert_eq!(cap, 2000);
        accrue_offline_rest(&mut inn, 1_000_000 + 365 * 86_400);
        assert_eq!(inn.rested_xp, cap);
        assert_eq!(accrue_offline_rest(&mut inn, 1_000_000 + 366 * 86_400), 0);
        // High levels get a proportionally deeper pool; huge spans don't overflow.
        let mut big = new_character("Elder");
        big.x = 0.0;
        big.y = 0.0;
        big.max_xp = 3_000_000_000;
        big.last_logout = Some(0);
        accrue_offline_rest(&mut big, u64::MAX / 2);
        assert_eq!(big.rested_xp, rested_cap(&big));
        assert!(rested_cap(&big) > 4_000_000_000);
    }

    #[test]
    fn rested_xp_is_spent_as_bonus_on_the_next_gains() {
        let mut w = World::new(24);
        let mut s = new_character("Rested");
        s.x = 2000.0;
        s.y = 0.0;
        s.level = 54; // B10 curve: 10,117 xp to the next level, no level-up below
        s.max_xp = crate::progression::xp_to_next(54);
        s.last_logout = Some(0);
        let gained = accrue_offline_rest(&mut s, 3600 * 40);
        assert_eq!(gained, crate::progression::xp_to_next(54) * 5 * 40 / 100 / 4);
        let id = w.spawn_player(1, s);
        w.dev_give_xp(Act::Eden, id, 300);
        let after = sheet(&w, Act::Eden, id);
        assert_eq!(after.xp, 600, "rested doubles the gain");
        assert_eq!(after.rested_xp, gained - 300);
    }

    #[test]
    fn spirit_release_rises_at_the_nearest_graveyard() {
        let mut w = World::new(25);
        let id = spawn(&mut w, Act::Eden, Vec2::new(1650.0, -1500.0), 0);
        assert!(w.release_spirit(Act::Eden, id).unwrap_err().contains("not dead"));
        {
            let e = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap();
            e.health = 0;
            e.dead_timer = 6.0;
        }
        let line = w.release_spirit(Act::Eden, id).unwrap();
        assert!(line.contains("eastern cairns"), "{line}");
        let e = &w.zones[&Act::Eden].entities[&id];
        assert_eq!(e.pos, Vec2::new(1700.0, -1700.0));
        assert_eq!(e.health, e.max_health / 2);
        // Alive again: no automatic inn respawn or Died event follows.
        let mut died = false;
        for _ in 0..200 {
            for ev in w.step() {
                died |= matches!(ev, SimEvent::Died { .. });
            }
        }
        assert!(!died, "a released spirit must not also be sent to the inn");
        assert!(w.zones[&Act::Eden].entities[&id].pos.distance(Vec2::new(1700.0, -1700.0)) < 400.0);
        assert!(w.release_spirit(Act::Eden, id).is_err(), "only the dead release");
        // Without release the old path still returns the dead to the inn.
        {
            let e = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap();
            e.health = 0;
            e.dead_timer = 0.2;
        }
        let mut died = false;
        for _ in 0..20 {
            for ev in w.step() {
                died |= matches!(ev, SimEvent::Died { .. });
            }
        }
        assert!(died);
        assert!(w.zones[&Act::Eden].entities[&id].pos.length() < 50.0);
        // Every graveyard is reachable and sits outside the capitals.
        for act in Act::ALL {
            for (_, p) in graveyards(act) {
                assert!(p.x.abs() < 3200.0 && p.y.abs() < 3200.0);
                assert!(pvp_zones(act).all(|z| Vec2::new(z.x, z.y).distance(p) > z.radius), "graveyard inside a capital");
            }
        }
    }

    #[test]
    fn regions_name_capitals_inns_caves_pois_and_open_country() {
        let r = region_at(Act::Eden, Vec2::new(5.0, 5.0), None);
        assert_eq!((r.key.as_str(), r.name.as_str(), r.pvp), ("inn:eden", "Inn of Eden", false));
        let wild = region_at(Act::Hermon, Vec2::new(3550.0, -3550.0), None);
        assert_eq!(wild.key, "act:hermon");
        assert_eq!(wild.name, "The Hermon Range");
        assert_eq!((wild.level_min, wild.level_max), (13, 24));
        assert!(wild.subtitle.contains("13-24"));
        // Enemy capital flags; your own does not; the undecided are hostile everywhere.
        let nod = Vec2::new(2400.0, 2400.0);
        let as_sethite = region_at(Act::Eden, nod, Some("sethite"));
        assert!(as_sethite.pvp && as_sethite.name.contains("Nod") && as_sethite.subtitle.contains("PvP"));
        assert!(!region_at(Act::Eden, nod, Some("cainite")).pvp);
        assert!(region_at(Act::Eden, nod, None).pvp);
        assert_eq!(as_sethite.key, region_at(Act::Eden, nod, Some("cainite")).key);
        // A cave and a POI away from the capitals and the inn.
        let cave = crate::questgen::anchors().iter().find(|a| a.cave && pvp_zones(a.act).all(|z| Vec2::new(z.x, z.y).distance(a.pos) > z.radius)).unwrap();
        let r = region_at(cave.act, cave.pos, None);
        assert_eq!((r.key.as_str(), r.name.as_str()), (cave.key, cave.name));
        assert!(r.subtitle.starts_with("Cave"));
        let poi = crate::questgen::anchors().iter().find(|a| !a.cave && a.pos.length() > 400.0 && pvp_zones(a.act).all(|z| Vec2::new(z.x, z.y).distance(a.pos) > z.radius)
            && crate::questgen::anchors().iter().all(|c| !c.cave || c.act != a.act || c.pos.distance(a.pos) > 500.0)).unwrap();
        let r = region_at(poi.act, poi.pos, Some("sethite"));
        assert_eq!(r.key, poi.key);
        assert!(!r.pvp);
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;

    /// Discovery is by name, so a flight master's name may not be shared
    /// with any other POI or cave in ANY act.
    #[test]
    fn flight_master_names_are_unique_across_every_act() {
        for n in flight_nodes().iter().filter(|n| !n.inn) {
            assert_eq!(crate::pois::all_pois().iter().filter(|p| p.name == n.name).count(), 1, "{}", n.name);
            assert!(!crate::caves::all_caves().iter().any(|c| c.name == n.name), "{}", n.name);
        }
        let mut names: Vec<&str> = flight_nodes().iter().map(|n| n.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), flight_nodes().len());
    }

    /// A refused flight costs nothing and moves nobody, whatever the reason.
    #[test]
    fn every_refused_flight_is_free() {
        let mut w = World::new(9);
        let mut s = crate::world::new_character("Miser");
        s.x = 0.0;
        s.y = 0.0;
        s.gold = 500;
        s.max_health = 1_000_000;
        s.health = 1_000_000;
        s.discovered.push(inn_name(Act::Eden).to_string());
        s.discovered.push(inn_name(Act::Flood).to_string());
        let id = w.spawn_player(4, s);
        let sunk = w.gold_sunk;
        let e = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap();
        e.combat_timer = 3.0;
        assert!(w.flight_take(Act::Eden, id, "inn_flood").is_err(), "in combat");
        let e = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap();
        e.combat_timer = 0.0;
        e.owner = None;
        assert!(w.flight_take(Act::Eden, id, "inn_flood").is_err(), "an unowned entity cannot change acts");
        assert!(w.flight_take(Act::Eden, id, "inn_eden").is_err(), "already there");
        assert!(w.flight_take(Act::Eden, id, &"x".repeat(100_000)).is_err(), "unknown node");
        assert_eq!(w.player_sheet(Act::Eden, id).unwrap().gold, 500, "no refusal may charge");
        assert_eq!(w.gold_sunk, sunk);
        assert!(w.player_pos(Act::Eden, id).unwrap().length() < 1.0);
        // And the same character, owned again, does pay exactly once.
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().owner = Some(4);
        let f = w.flight_take(Act::Eden, id, "inn_flood").unwrap();
        assert_eq!(w.player_sheet(Act::Flood, f.entity).unwrap().gold, 500 - f.cost);
        assert!(w.zones[&Act::Eden].entities.get(&id).is_none(), "no copy left behind");
        assert_eq!(w.gold_sunk, sunk + f.cost as u64);
    }
}
