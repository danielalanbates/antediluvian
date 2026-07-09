//! Authoritative world model and fixed-tick simulation.
//!
//! The world is the single source of truth. Clients send *intents*
//! (`Move`, `Attack`); the server integrates them here. One `World` owns every
//! zone; each zone owns its entities. The whole sim runs single-threaded on the
//! game loop, so there are no locks in the hot path.

use antediluvia_protocol::{Act, CharacterSheet, EntityId, EntityKind, EntityState};
use glam::Vec2;
use std::collections::HashMap;

pub const TICK_HZ: u64 = 20;
pub const DT: f32 = 1.0 / TICK_HZ as f32;
pub const WORLD_BOUNDS: f32 = 1800.0;

const PLAYER_SPEED: f32 = 260.0;
const ENEMY_SPEED: f32 = 120.0;
const WILDLIFE_SPEED: f32 = 190.0;
const MELEE_RANGE: f32 = 80.0;
const MELEE_ARC_DOT: f32 = 0.35; // cos of half-arc; target must be roughly in front
const ENEMY_MELEE_RANGE: f32 = 46.0;
const PLAYER_ATTACK_DMG: i32 = 12;
const ATTACK_COOLDOWN: f32 = 0.8;
const RESPAWN_SECS: f32 = 6.0;
const HEALTH_REGEN_PER_SEC: f32 = 2.0;
const MANA_REGEN_PER_SEC: f32 = 4.0;

/// A tiny xorshift RNG so the sim has no external rand dependency and stays
/// deterministic given a seed (useful for reproducible tests / replays).
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        let f = (self.next_u64() >> 11) as f32 / (1u64 << 53) as f32;
        lo + f * (hi - lo)
    }
    fn point(&mut self, bound: f32) -> Vec2 {
        Vec2::new(self.range(-bound, bound), self.range(-bound, bound))
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum AiState {
    Patrol,
    Chase,
    Return,
    Graze,
    Flee,
    Static,
}

pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub pos: Vec2,
    pub rot: f32,
    pub health: i32,
    pub max_health: i32,
    pub tag: Option<String>,
    pub name: Option<String>,
    pub speed: f32,
    // AI / behavior
    pub origin: Vec2,
    pub aggro_range: f32,
    pub patrol_radius: f32,
    pub state: AiState,
    pub state_timer: f32,
    pub attack_cooldown: f32,
    pub damage: i32,
    pub xp_value: u32,
    pub wander_target: Vec2,
    // player-only
    pub intent: Vec2,
    pub attack_queued: bool,
    pub attack_timer: f32,
    pub dead_timer: f32,
    /// For a player entity, the owning connection id.
    pub owner: Option<u64>,
    /// For a player, its persistent sheet (kept in sync so we can save it).
    pub sheet: Option<CharacterSheet>,
}

impl Entity {
    pub fn to_state(&self) -> EntityState {
        EntityState {
            id: self.id,
            kind: self.kind,
            x: self.pos.x,
            y: self.pos.y,
            rot: self.rot,
            health: self.health,
            max_health: self.max_health,
            tag: self.tag.clone(),
            name: self.name.clone(),
        }
    }
}

pub struct Zone {
    pub act: Act,
    pub entities: HashMap<EntityId, Entity>,
    pub tick: u64,
    /// Where players arriving in this zone spawn.
    pub entry: Vec2,
}

pub struct World {
    pub zones: HashMap<Act, Zone>,
    next_id: EntityId,
    rng: Rng,
}

/// Something that happened this tick and needs to be reported outside the sim
/// (e.g. pushed to a specific player's connection).
pub enum SimEvent {
    LevelUp { owner: u64, level: u32 },
    Died { owner: u64 },
    Loot { owner: u64, item: String },
}

impl World {
    pub fn new(seed: u64) -> Self {
        let mut w = World {
            zones: HashMap::new(),
            next_id: 1,
            rng: Rng::new(seed),
        };
        for act in Act::ALL {
            let mut zone = Zone {
                act,
                entities: HashMap::new(),
                tick: 0,
                entry: Vec2::ZERO,
            };
            w.populate_zone(&mut zone);
            w.zones.insert(act, zone);
        }
        w
    }

    fn alloc_id(&mut self) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Seed a zone with enemies, wildlife and resource nodes. Counts vary by act
    /// so the later, more dangerous acts feel different.
    fn populate_zone(&mut self, zone: &mut Zone) {
        let (n_enemies, enemy_tag, wildlife_tag) = match zone.act {
            Act::Eden => (6, "serpent", "deer"),
            Act::Hermon => (10, "watcher", "goat"),
            Act::Nephilim => (14, "giant", "boar"),
            Act::Enoch => (12, "shade", "raven"),
            Act::Flood => (18, "leviathan", "fish"),
        };
        let act = zone.act;
        for _ in 0..n_enemies {
            let pos = self.rng.point(WORLD_BOUNDS * 0.9);
            let id = self.alloc_id();
            zone.entities.insert(id, make_enemy(id, pos, enemy_tag, act));
        }
        for _ in 0..8 {
            let pos = self.rng.point(WORLD_BOUNDS * 0.9);
            let id = self.alloc_id();
            zone.entities.insert(id, make_wildlife(id, pos, wildlife_tag));
        }
        for _ in 0..14 {
            let pos = self.rng.point(WORLD_BOUNDS * 0.9);
            let id = self.alloc_id();
            let tag = if self.rng.range(0.0, 1.0) < 0.6 { "tree" } else { "rock" };
            zone.entities.insert(id, make_resource(id, pos, tag));
        }
    }

    /// Spawn a player entity into its zone from a character sheet. Returns the
    /// new entity id.
    pub fn spawn_player(&mut self, owner: u64, sheet: CharacterSheet) -> EntityId {
        let id = self.alloc_id();
        let act = sheet.act;
        let pos = Vec2::new(sheet.x, sheet.y);
        let ent = Entity {
            id,
            kind: EntityKind::Player,
            pos,
            rot: 0.0,
            health: sheet.health,
            max_health: sheet.max_health,
            tag: None,
            name: Some(sheet.name.clone()),
            speed: PLAYER_SPEED,
            origin: pos,
            aggro_range: 0.0,
            patrol_radius: 0.0,
            state: AiState::Static,
            state_timer: 0.0,
            attack_cooldown: 0.0,
            damage: PLAYER_ATTACK_DMG,
            xp_value: 0,
            wander_target: pos,
            intent: Vec2::ZERO,
            attack_queued: false,
            attack_timer: 0.0,
            dead_timer: 0.0,
            owner: Some(owner),
            sheet: Some(sheet),
        };
        self.zones.get_mut(&act).unwrap().entities.insert(id, ent);
        id
    }

    pub fn remove_player(&mut self, act: Act, id: EntityId) -> Option<CharacterSheet> {
        self.zones
            .get_mut(&act)
            .and_then(|z| z.entities.remove(&id))
            .and_then(|e| e.sheet)
    }

    /// Advance every zone one tick. Returns per-tick events keyed by owner
    /// connection so the caller can notify the right players.
    pub fn step(&mut self) -> Vec<SimEvent> {
        let mut events = Vec::new();
        let acts: Vec<Act> = self.zones.keys().copied().collect();
        for act in acts {
            self.step_zone(act, &mut events);
        }
        events
    }

    fn step_zone(&mut self, act: Act, events: &mut Vec<SimEvent>) {
        // The tick needs the zone, the rng, and the id counter simultaneously.
        // Move rng + counter into locals so the only borrow of `self` in the
        // hot loop is `self.zones`; restore them at the end.
        let mut rng = std::mem::replace(&mut self.rng, Rng::new(1));
        let mut next_id = self.next_id;
        let rng = &mut rng;

        let zone = self.zones.get_mut(&act).unwrap();
        zone.tick += 1;

        // Snapshots of live players and enemies (id, pos) for cross-entity AI/combat.
        let players: Vec<(EntityId, Vec2)> = zone
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Player && e.health > 0)
            .map(|e| (e.id, e.pos))
            .collect();
        let enemies: Vec<(EntityId, Vec2)> = zone
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Enemy && e.health > 0)
            .map(|e| (e.id, e.pos))
            .collect();

        // (target_id, damage, attacker_owner_entity_if_player_source)
        let mut damage: Vec<(EntityId, i32, Option<EntityId>)> = Vec::new();
        let entry = zone.entry;

        for e in zone.entities.values_mut() {
            if e.attack_cooldown > 0.0 {
                e.attack_cooldown -= DT;
            }
            match e.kind {
                EntityKind::Player => {
                    if e.health <= 0 {
                        e.dead_timer -= DT;
                        if e.dead_timer <= 0.0 {
                            e.health = e.max_health;
                            e.pos = entry;
                            if let Some(o) = e.owner {
                                events.push(SimEvent::Died { owner: o });
                            }
                        }
                        continue;
                    }
                    if e.intent.length_squared() > 0.0001 {
                        let dir = e.intent.normalize_or_zero();
                        e.pos += dir * e.speed * DT;
                        e.pos = e.pos.clamp(Vec2::splat(-WORLD_BOUNDS), Vec2::splat(WORLD_BOUNDS));
                        e.rot = dir.y.atan2(dir.x);
                    }
                    if let Some(s) = e.sheet.as_mut() {
                        let regen = (HEALTH_REGEN_PER_SEC * DT).round() as i32;
                        if e.health < e.max_health {
                            e.health = (e.health + regen).min(e.max_health);
                        }
                        s.health = e.health;
                        if s.mana < s.max_mana {
                            s.mana = (s.mana + (MANA_REGEN_PER_SEC * DT).round() as i32).min(s.max_mana);
                        }
                    }
                    if e.attack_queued && e.attack_cooldown <= 0.0 {
                        e.attack_cooldown = ATTACK_COOLDOWN;
                        let facing = Vec2::new(e.rot.cos(), e.rot.sin());
                        for (eid, epos) in &enemies {
                            let to = *epos - e.pos;
                            let d = to.length();
                            if d <= MELEE_RANGE && d > 0.01 && to.normalize().dot(facing) >= MELEE_ARC_DOT {
                                damage.push((*eid, e.damage, Some(e.id)));
                            }
                        }
                    }
                    e.attack_queued = false;
                }
                EntityKind::Enemy => {
                    if e.health <= 0 {
                        continue;
                    }
                    let nearest = nearest_of(&players, e.pos);
                    match nearest {
                        Some((pid, ppos, dist)) if dist <= e.aggro_range => {
                            e.state = AiState::Chase;
                            let to = ppos - e.pos;
                            if dist > ENEMY_MELEE_RANGE {
                                let dir = to.normalize_or_zero();
                                e.pos += dir * e.speed * DT;
                                e.rot = dir.y.atan2(dir.x);
                            } else if e.attack_cooldown <= 0.0 {
                                e.attack_cooldown = ATTACK_COOLDOWN;
                                damage.push((pid, e.damage, None));
                            }
                        }
                        _ => {
                            e.state = AiState::Patrol;
                            e.state_timer -= DT;
                            if e.state_timer <= 0.0 || e.pos.distance(e.wander_target) < 20.0 {
                                e.state_timer = rng.range(1.5, 4.0);
                                let ang = rng.range(0.0, std::f32::consts::TAU);
                                let r = rng.range(0.0, e.patrol_radius);
                                e.wander_target = e.origin + Vec2::new(ang.cos(), ang.sin()) * r;
                            }
                            let dir = (e.wander_target - e.pos).normalize_or_zero();
                            e.pos += dir * (e.speed * 0.5) * DT;
                            if dir.length_squared() > 0.0 {
                                e.rot = dir.y.atan2(dir.x);
                            }
                        }
                    }
                }
                EntityKind::Wildlife => {
                    let nearest = nearest_of(&players, e.pos);
                    match nearest {
                        Some((_, ppos, dist)) if dist <= e.aggro_range => {
                            e.state = AiState::Flee;
                            let dir = (e.pos - ppos).normalize_or_zero();
                            e.pos += dir * e.speed * DT;
                            e.rot = dir.y.atan2(dir.x);
                        }
                        _ => {
                            e.state = AiState::Graze;
                            e.state_timer -= DT;
                            if e.state_timer <= 0.0 {
                                e.state_timer = rng.range(2.0, 5.0);
                                let ang = rng.range(0.0, std::f32::consts::TAU);
                                let r = rng.range(0.0, e.patrol_radius);
                                e.wander_target = e.origin + Vec2::new(ang.cos(), ang.sin()) * r;
                            }
                            let dir = (e.wander_target - e.pos).normalize_or_zero();
                            e.pos += dir * (e.speed * 0.35) * DT;
                        }
                    }
                    e.pos = e.pos.clamp(Vec2::splat(-WORLD_BOUNDS), Vec2::splat(WORLD_BOUNDS));
                }
                EntityKind::Resource | EntityKind::Npc => {}
            }
        }

        // Apply damage; collect kills.
        let mut killed: Vec<(EntityId, Option<EntityId>)> = Vec::new();
        for (target, dmg, attacker) in damage {
            if let Some(t) = zone.entities.get_mut(&target) {
                if t.health <= 0 {
                    continue;
                }
                t.health -= dmg;
                if t.health <= 0 {
                    killed.push((target, attacker));
                    if t.kind == EntityKind::Player {
                        t.dead_timer = RESPAWN_SECS;
                    }
                }
            }
        }

        // Resolve kills: enemies grant xp to their killer and respawn a replacement.
        for (target_id, attacker_ent) in killed {
            let (is_enemy, xp_value, etag) = match zone.entities.get(&target_id) {
                Some(t) if t.kind == EntityKind::Enemy => (true, t.xp_value, t.tag.clone()),
                _ => (false, 0, None),
            };
            if !is_enemy {
                continue;
            }
            zone.entities.remove(&target_id);

            // Replacement enemy elsewhere so the zone stays populated.
            let pos = rng.point(WORLD_BOUNDS * 0.9);
            let tag = etag.as_deref().unwrap_or("serpent");
            let nid = self_next_id(&mut next_id);
            zone.entities.insert(nid, make_enemy(nid, pos, tag, act));

            // Award xp + loot to the killer.
            if let Some(owner_ent) = attacker_ent {
                if let Some(p) = zone.entities.get_mut(&owner_ent) {
                    if let Some(o) = p.owner {
                        if award_xp(p, xp_value) {
                            let lvl = p.sheet.as_ref().map(|s| s.level).unwrap_or(1);
                            events.push(SimEvent::LevelUp { owner: o, level: lvl });
                        }
                        if let Some(s) = p.sheet.as_mut() {
                            if s.inventory.len() < 40 {
                                let item = format!("{}_trophy", tag);
                                s.inventory.push(item.clone());
                                events.push(SimEvent::Loot { owner: o, item });
                            }
                        }
                    }
                }
            }
        }

        // Restore rng + counter into self.
        self.rng = std::mem::replace(rng, Rng::new(1));
        self.next_id = next_id;
    }

    pub fn set_intent(&mut self, act: Act, id: EntityId, dir: Vec2) {
        if let Some(z) = self.zones.get_mut(&act) {
            if let Some(e) = z.entities.get_mut(&id) {
                e.intent = dir;
            }
        }
    }

    pub fn queue_attack(&mut self, act: Act, id: EntityId) {
        if let Some(z) = self.zones.get_mut(&act) {
            if let Some(e) = z.entities.get_mut(&id) {
                e.attack_queued = true;
            }
        }
    }

    /// Read back a player's current sheet (synced position + stats) for saving.
    pub fn player_sheet(&self, act: Act, id: EntityId) -> Option<CharacterSheet> {
        let e = self.zones.get(&act)?.entities.get(&id)?;
        let mut s = e.sheet.clone()?;
        s.x = e.pos.x;
        s.y = e.pos.y;
        s.act = act;
        s.health = e.health;
        Some(s)
    }

    pub fn zone_snapshot(&self, act: Act) -> (u64, Vec<EntityState>) {
        let z = &self.zones[&act];
        (z.tick, z.entities.values().map(|e| e.to_state()).collect())
    }
}

/// Bump and return the next entity id given a direct borrow of the counter
/// (used inside `step_zone` where `self` is partially borrowed).
fn self_next_id(counter: &mut EntityId) -> EntityId {
    let id = *counter;
    *counter += 1;
    id
}

fn make_enemy(id: EntityId, pos: Vec2, tag: &str, act: Act) -> Entity {
    let tier = Act::ALL.iter().position(|a| *a == act).unwrap_or(0) as i32;
    let max_health = 40 + tier * 20;
    Entity {
        id,
        kind: EntityKind::Enemy,
        pos,
        rot: 0.0,
        health: max_health,
        max_health,
        tag: Some(tag.to_string()),
        name: None,
        speed: ENEMY_SPEED,
        origin: pos,
        aggro_range: 380.0,
        patrol_radius: 260.0,
        state: AiState::Patrol,
        state_timer: 0.0,
        attack_cooldown: 0.0,
        damage: 6 + tier * 3,
        xp_value: (15 + tier * 10) as u32,
        wander_target: pos,
        intent: Vec2::ZERO,
        attack_queued: false,
        attack_timer: 0.0,
        dead_timer: 0.0,
        owner: None,
        sheet: None,
    }
}

fn make_wildlife(id: EntityId, pos: Vec2, tag: &str) -> Entity {
    Entity {
        id,
        kind: EntityKind::Wildlife,
        pos,
        rot: 0.0,
        health: 20,
        max_health: 20,
        tag: Some(tag.to_string()),
        name: None,
        speed: WILDLIFE_SPEED,
        origin: pos,
        aggro_range: 240.0, // used as flee range
        patrol_radius: 200.0,
        state: AiState::Graze,
        state_timer: 0.0,
        attack_cooldown: 0.0,
        damage: 0,
        xp_value: 5,
        wander_target: pos,
        intent: Vec2::ZERO,
        attack_queued: false,
        attack_timer: 0.0,
        dead_timer: 0.0,
        owner: None,
        sheet: None,
    }
}

fn make_resource(id: EntityId, pos: Vec2, tag: &str) -> Entity {
    Entity {
        id,
        kind: EntityKind::Resource,
        pos,
        rot: 0.0,
        health: 1,
        max_health: 1,
        tag: Some(tag.to_string()),
        name: None,
        speed: 0.0,
        origin: pos,
        aggro_range: 0.0,
        patrol_radius: 0.0,
        state: AiState::Static,
        state_timer: 0.0,
        attack_cooldown: 0.0,
        damage: 0,
        xp_value: 0,
        wander_target: pos,
        intent: Vec2::ZERO,
        attack_queued: false,
        attack_timer: 0.0,
        dead_timer: 0.0,
        owner: None,
        sheet: None,
    }
}

fn nearest_of(players: &[(EntityId, Vec2)], from: Vec2) -> Option<(EntityId, Vec2, f32)> {
    players
        .iter()
        .map(|(id, p)| (*id, *p, from.distance(*p)))
        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
}

/// Grant xp, applying level-ups. Returns true if the player leveled at least once.
fn award_xp(p: &mut Entity, xp: u32) -> bool {
    let Some(s) = p.sheet.as_mut() else { return false };
    s.xp += xp;
    let mut leveled = false;
    while s.xp >= s.max_xp {
        s.xp -= s.max_xp;
        s.level += 1;
        s.max_xp = (s.max_xp as f32 * 1.35) as u32;
        s.max_health += 15;
        s.max_mana += 8;
        s.health = s.max_health;
        s.mana = s.max_mana;
        leveled = true;
    }
    if leveled {
        p.max_health = s.max_health;
        p.health = s.health;
    }
    leveled
}

/// A fresh level-1 character at Eden's entry point.
pub fn new_character(name: &str) -> CharacterSheet {
    CharacterSheet {
        name: name.to_string(),
        act: Act::Eden,
        x: 0.0,
        y: 0.0,
        level: 1,
        xp: 0,
        max_xp: 100,
        health: 100,
        max_health: 100,
        mana: 50,
        max_mana: 50,
        inventory: Vec::new(),
    }
}
