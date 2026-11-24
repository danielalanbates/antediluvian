// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Binary delta snapshots for a crowd (B11).
//!
//! The JSON path sends each client its nearest 150 entities in full, ten
//! times a second. This module is what lets one client see 1,000 players:
//!
//! * **Grid-driven.** Each snapshot tick the cells some client can see are
//!   marked; only entities filed in those cells are read from the sim, once,
//!   into a compact per-zone table. Nothing is read or encoded for a part of
//!   the map nobody is looking at.
//! * **Delta.** Every table row remembers the snapshot number at which each
//!   of its fields last changed. A client remembers one number per row: when
//!   it was last sent. "What changed for this client" is then five integer
//!   compares per entity, with no per-client copy of the world.
//! * **Rate tiers.** The nearest `NEAR_PLAYERS` players and `NEAR_OTHERS`
//!   other entities inside `NEAR_RADIUS` are checked every snapshot (10 Hz);
//!   everything else in view is checked every `FAR_PHASES`-th snapshot
//!   (2.5 Hz), spread over the phases so the bytes are level.
//! * **Enter / leave** are worked out by a grid scan every `SCAN_EVERY`
//!   snapshots per client (staggered by connection id), which also re-picks
//!   the tiers and applies the per-client caps, nearest first.
//! * **Keyframes** go out on login, zone change, `Resync`, and every
//!   `KEYFRAME_EVERY` snapshots as a safety net.

use antediluvia_protocol::wire::{self, Dyn, FrameWriter};
use antediluvia_protocol::{Act, EntityId, EntityKind};
use antediluvia_sim::aoi::{self, GRID_CELLS};
use antediluvia_sim::world::{Entity, World, Zone};
use glam::Vec2;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Inside this distance an entity may be in the 10 Hz tier.
pub const NEAR_RADIUS: f32 = 450.0;
/// Most players updated at 10 Hz for one client (nearest first).
pub const NEAR_PLAYERS: usize = 64;
/// Most mobs / NPCs / nodes updated at 10 Hz for one client.
pub const NEAR_OTHERS: usize = 64;
/// Far entities are checked every this many snapshots (10 Hz / 4 = 2.5 Hz).
pub const FAR_PHASES: u32 = 4;
/// Snapshots between enter / leave / tier scans for one client.
pub const SCAN_EVERY: u32 = 4;
/// Most players one client is told about (was 150 entities of any kind).
pub const MAX_PLAYERS_PER_CLIENT: usize = 1200;
/// Most non-player entities one client is told about.
pub const MAX_OTHERS_PER_CLIENT: usize = 400;
/// Snapshots between unprompted keyframes (60 s).
pub const KEYFRAME_EVERY: u32 = 600;
/// A client's `Resync` is answered at most this often (2 s).
pub const RESYNC_MIN_GAP: u32 = 20;
/// A known entity is kept until it is this far outside the AoI radius, so
/// one pacing along the edge does not enter and leave over and over.
const LEAVE_MARGIN: f32 = 64.0;

/// One binary-wire client, as the game loop describes it each snapshot.
#[derive(Debug, Clone, Copy)]
pub struct ClientReq {
    pub conn: u64,
    pub act: Act,
    pub entity: EntityId,
    pub aoi: f32,
    /// The client asked for a keyframe since the last call.
    pub resync: bool,
}

/// One entity some client can see.
struct Slot {
    live: bool,
    player: bool,
    id: EntityId,
    pos: Vec2,
    d: Dyn,
    /// Snapshot number at which each field last changed.
    pos_seq: u32,
    rot_seq: u32,
    hp_seq: u32,
    maxhp_seq: u32,
    meta_seq: u32,
    /// Snapshot number of the last refresh that found the entity.
    touched: u32,
    meta_hash: u64,
    /// `wire::encode_meta` bytes, rebuilt only when `meta_hash` changes.
    meta: Vec<u8>,
}

/// Hash of everything `encode_meta` writes, without cloning any of it.
fn meta_hash(e: &Entity) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    e.tag.hash(&mut h);
    e.name.hash(&mut h);
    e.mounted.hash(&mut h);
    if let Some(s) = &e.sheet {
        for k in ["weapon", "chest", "back", "mount"] {
            s.equipment.get(k).hash(&mut h);
        }
        s.faction.hash(&mut h);
        s.appearance.hash(&mut h);
        s.look.hash(&mut h);
    }
    h.finish()
}

#[derive(Default)]
struct ZoneView {
    slots: Vec<Slot>,
    by_id: HashMap<EntityId, u32>,
    free: Vec<u32>,
    /// Slots freed last snapshot; reusable once every client has been told.
    pending_free: Vec<u32>,
    /// (slot, id) of entities that vanished this snapshot.
    removed: Vec<(u32, EntityId)>,
    /// Per grid cell: the slots filed there this snapshot.
    cells: Vec<Vec<u32>>,
    /// Per grid cell: some client's AoI touches it this snapshot.
    watched: Vec<bool>,
    tick: u64,
}

impl ZoneView {
    fn clear_watch(&mut self) {
        if self.watched.len() != GRID_CELLS {
            self.watched = vec![false; GRID_CELLS];
            self.cells = vec![Vec::new(); GRID_CELLS];
        }
        self.watched.iter_mut().for_each(|w| *w = false);
    }

    /// Read every entity in a watched cell out of the sim, noting what changed.
    fn refresh(&mut self, zone: &Zone, seq: u32) {
        self.tick = zone.tick;
        self.free.append(&mut self.pending_free);
        self.removed.clear();
        let Self { slots, by_id, free, cells, watched, .. } = self;
        for cell in 0..GRID_CELLS {
            let list = &mut cells[cell];
            list.clear();
            if !watched[cell] {
                continue;
            }
            for id in zone.grid.cell_entities(cell) {
                let Some(e) = zone.entities.get(id) else { continue };
                let d = Dyn::new(e.pos.x, e.pos.y, e.rot, e.health, e.max_health);
                let mh = meta_hash(e);
                let si = match by_id.get(id) {
                    Some(si) => *si,
                    None => {
                        let fresh = Slot {
                            live: true,
                            player: e.kind == EntityKind::Player,
                            id: *id,
                            pos: e.pos,
                            d,
                            pos_seq: seq,
                            rot_seq: seq,
                            hp_seq: seq,
                            maxhp_seq: seq,
                            meta_seq: seq,
                            touched: seq,
                            // Forces the meta bytes to be built below.
                            meta_hash: !mh,
                            meta: Vec::new(),
                        };
                        let si = match free.pop() {
                            Some(si) => {
                                slots[si as usize] = fresh;
                                si
                            }
                            None => {
                                slots.push(fresh);
                                slots.len() as u32 - 1
                            }
                        };
                        by_id.insert(*id, si);
                        si
                    }
                };
                let s = &mut slots[si as usize];
                s.pos = e.pos;
                s.touched = seq;
                let m = d.diff(&s.d);
                if m & wire::F_POS != 0 {
                    s.pos_seq = seq;
                }
                if m & wire::F_ROT != 0 {
                    s.rot_seq = seq;
                }
                if m & wire::F_HEALTH != 0 {
                    s.hp_seq = seq;
                }
                if m & wire::F_MAX_HEALTH != 0 {
                    s.maxhp_seq = seq;
                }
                s.d = d;
                if s.meta_hash != mh {
                    s.meta_hash = mh;
                    s.meta_seq = seq;
                    s.meta.clear();
                    wire::encode_meta(&e.to_state(), &mut s.meta);
                }
                list.push(si);
            }
        }
        // Whatever was not found this time died, despawned, or walked into a
        // cell nobody watches.
        for (i, s) in slots.iter_mut().enumerate() {
            if s.live && s.touched != seq {
                s.live = false;
                s.meta = Vec::new();
                by_id.remove(&s.id);
                self.removed.push((i as u32, s.id));
                self.pending_free.push(i as u32);
            }
        }
    }
}

struct ClientView {
    act: Act,
    entity: EntityId,
    /// Per slot: snapshot number of the last send, 0 = not known to the client.
    known: Vec<u32>,
    near: Vec<u32>,
    far: [Vec<u32>; FAR_PHASES as usize],
    frame_seq: u32,
    last_key: u32,
    want_key: bool,
    fresh: bool,
    /// Last snapshot number this client was served (stale views are dropped).
    served: u32,
}

impl ClientView {
    fn new(act: Act, entity: EntityId) -> Self {
        Self {
            act,
            entity,
            known: Vec::new(),
            near: Vec::new(),
            far: Default::default(),
            frame_seq: 0,
            last_key: 0,
            want_key: false,
            fresh: true,
            served: 0,
        }
    }
}

/// Counters since the last `take_stats`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SnapStats {
    pub frames: u64,
    pub bytes: u64,
    pub keyframes: u64,
    pub enters: u64,
    pub updates: u64,
    pub leaves: u64,
}

#[derive(Default)]
struct Scratch {
    players: Vec<(f32, u32)>,
    others: Vec<(f32, u32)>,
    mark: Vec<u32>,
    gen: u32,
}

/// Owner of every binary client's view. Lives in the game loop.
#[derive(Default)]
pub struct SnapHub {
    seq: u32,
    zones: HashMap<Act, ZoneView>,
    clients: HashMap<u64, ClientView>,
    scratch: Scratch,
    stats: SnapStats,
    centers: Vec<Option<Vec2>>,
}

impl SnapHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_stats(&mut self) -> SnapStats {
        std::mem::take(&mut self.stats)
    }

    /// Build one frame per client and hand it to `emit`.
    pub fn run(
        &mut self,
        world: &World,
        reqs: &[ClientReq],
        time_of_day: f32,
        mut emit: impl FnMut(u64, Vec<u8>),
    ) {
        self.seq += 1;
        let seq = self.seq;

        // 1. Which cells can anyone see?
        for zv in self.zones.values_mut() {
            zv.clear_watch();
        }
        self.centers.clear();
        for r in reqs {
            let center = world.zones.get(&r.act).and_then(|z| z.entities.get(&r.entity)).map(|e| e.pos);
            if let Some(c) = center {
                let zv = self.zones.entry(r.act).or_default();
                if zv.watched.len() != GRID_CELLS {
                    zv.clear_watch();
                }
                for cell in aoi::cells_in_radius(c, r.aoi + LEAVE_MARGIN) {
                    zv.watched[cell] = true;
                }
            }
            self.centers.push(center);
        }

        // 2. Read those cells out of the sim once.
        for (act, zv) in self.zones.iter_mut() {
            if let Some(z) = world.zones.get(act) {
                zv.refresh(z, seq);
            }
        }

        // 3. One frame per client.
        for (r, center) in reqs.iter().zip(self.centers.iter()) {
            let Some(center) = *center else { continue };
            let Some(zv) = self.zones.get(&r.act) else { continue };
            let cv = self.clients.entry(r.conn).or_insert_with(|| ClientView::new(r.act, r.entity));
            if cv.act != r.act {
                *cv = ClientView::new(r.act, r.entity);
            }
            cv.served = seq;
            let frame = build_frame(zv, cv, r, center, seq, time_of_day, &mut self.scratch, &mut self.stats);
            self.stats.frames += 1;
            self.stats.bytes += frame.len() as u64;
            emit(r.conn, frame);
        }
        // A client that was not served missed this snapshot's removals; it
        // starts over with a keyframe when it comes back.
        self.clients.retain(|_, c| c.served == seq);
    }
}

#[allow(clippy::too_many_arguments)]
fn build_frame(
    zv: &ZoneView,
    cv: &mut ClientView,
    r: &ClientReq,
    center: Vec2,
    seq: u32,
    time_of_day: f32,
    sc: &mut Scratch,
    stats: &mut SnapStats,
) -> Vec<u8> {
    if cv.known.len() < zv.slots.len() {
        cv.known.resize(zv.slots.len(), 0);
    }
    cv.want_key |= r.resync;
    let since_key = seq.wrapping_sub(cv.last_key);
    let key = cv.fresh || since_key >= KEYFRAME_EVERY || (cv.want_key && since_key >= RESYNC_MIN_GAP);
    let respawned = cv.entity != r.entity;
    cv.entity = r.entity;
    if key {
        cv.known.iter_mut().for_each(|k| *k = 0);
        cv.near.clear();
        cv.far.iter_mut().for_each(Vec::clear);
        cv.last_key = seq;
        cv.want_key = false;
        cv.fresh = false;
        stats.keyframes += 1;
    }
    cv.frame_seq = cv.frame_seq.wrapping_add(1);
    let mut w = FrameWriter::begin(key, r.act, cv.frame_seq, zv.tick, time_of_day);

    // Entities that died or despawned (a keyframe already dropped them all).
    if !key {
        for &(si, id) in &zv.removed {
            let k = &mut cv.known[si as usize];
            if *k != 0 {
                *k = 0;
                w.leave(id);
                stats.leaves += 1;
            }
        }
    }

    let phase = seq.wrapping_add(r.conn as u32) % FAR_PHASES;
    if key || respawned || seq.wrapping_add(r.conn as u32) % SCAN_EVERY == 0 {
        scan(zv, cv, r, center, seq, sc, &mut w, stats);
    }

    // Changed fields of what the client already holds.
    let ClientView { known, near, far, .. } = cv;
    for &si in near.iter().chain(far[phase as usize].iter()) {
        let k = known[si as usize];
        if k == 0 {
            continue; // removed since the last scan
        }
        let s = &zv.slots[si as usize];
        let mut mask = 0u8;
        if s.pos_seq > k {
            mask |= wire::F_POS;
        }
        if s.rot_seq > k {
            mask |= wire::F_ROT;
        }
        if s.hp_seq > k {
            mask |= wire::F_HEALTH;
        }
        if s.maxhp_seq > k {
            mask |= wire::F_MAX_HEALTH;
        }
        if s.meta_seq > k {
            mask |= wire::F_META;
        }
        if mask != 0 {
            w.update(s.id, mask, s.d, &s.meta);
            known[si as usize] = seq;
            stats.updates += 1;
        }
    }
    w.finish()
}

/// Keep the nearest `cap` of `v`.
fn nearest(v: &mut Vec<(f32, u32)>, cap: usize) {
    if v.len() > cap {
        v.select_nth_unstable_by(cap, |a, b| a.0.total_cmp(&b.0));
        v.truncate(cap);
    }
}

/// Enter / leave / tier pass: walk the grid cells under the client's AoI.
#[allow(clippy::too_many_arguments)]
fn scan(
    zv: &ZoneView,
    cv: &mut ClientView,
    r: &ClientReq,
    center: Vec2,
    seq: u32,
    sc: &mut Scratch,
    w: &mut FrameWriter,
    stats: &mut SnapStats,
) {
    let r_in2 = r.aoi * r.aoi;
    let r_out = r.aoi + LEAVE_MARGIN;
    let r_out2 = r_out * r_out;
    sc.players.clear();
    sc.others.clear();
    for cell in aoi::cells_in_radius(center, r_out) {
        for &si in &zv.cells[cell] {
            let s = &zv.slots[si as usize];
            let d2 = s.pos.distance_squared(center);
            if d2 > r_out2 || (d2 > r_in2 && cv.known[si as usize] == 0) {
                continue;
            }
            // The client's own character sorts first whatever happens.
            let d2 = if s.id == r.entity { -1.0 } else { d2 };
            if s.player {
                sc.players.push((d2, si));
            } else {
                sc.others.push((d2, si));
            }
        }
    }
    nearest(&mut sc.players, MAX_PLAYERS_PER_CLIENT);
    nearest(&mut sc.others, MAX_OTHERS_PER_CLIENT);

    // Leave: known before, not kept now.
    sc.gen = sc.gen.wrapping_add(1);
    if sc.gen == 0 {
        sc.mark.iter_mut().for_each(|m| *m = 0);
        sc.gen = 1;
    }
    if sc.mark.len() < zv.slots.len() {
        sc.mark.resize(zv.slots.len(), 0);
    }
    for &(_, si) in sc.players.iter().chain(sc.others.iter()) {
        sc.mark[si as usize] = sc.gen;
    }
    for &si in cv.near.iter().chain(cv.far.iter().flatten()) {
        let k = &mut cv.known[si as usize];
        if *k != 0 && sc.mark[si as usize] != sc.gen {
            *k = 0;
            w.leave(zv.slots[si as usize].id);
            stats.leaves += 1;
        }
    }

    // Enter the new ones and deal everything into its tier.
    cv.near.clear();
    cv.far.iter_mut().for_each(Vec::clear);
    let near2 = NEAR_RADIUS * NEAR_RADIUS;
    for (list, budget) in [(&mut sc.players, NEAR_PLAYERS), (&mut sc.others, NEAR_OTHERS)] {
        if list.len() > budget {
            list.select_nth_unstable_by(budget, |a, b| a.0.total_cmp(&b.0));
        }
        for (i, &(d2, si)) in list.iter().enumerate() {
            let s = &zv.slots[si as usize];
            let k = &mut cv.known[si as usize];
            if *k == 0 {
                *k = seq;
                w.enter(s.id, s.d, &s.meta);
                stats.enters += 1;
            }
            if i < budget && d2 <= near2 {
                cv.near.push(si);
            } else {
                cv.far[(si % FAR_PHASES) as usize].push(si);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antediluvia_protocol::wire::SnapshotDecoder;
    use antediluvia_sim::world::new_character;

    /// A world, a hub and one decoder per client, checked against the sim.
    struct Rig {
        world: World,
        hub: SnapHub,
        reqs: Vec<ClientReq>,
        dec: HashMap<u64, SnapshotDecoder>,
        bytes: HashMap<u64, u64>,
        last: HashMap<u64, wire::FrameStats>,
    }

    impl Rig {
        fn new() -> Self {
            Self {
                world: World::new(11),
                hub: SnapHub::new(),
                reqs: Vec::new(),
                dec: HashMap::new(),
                bytes: HashMap::new(),
                last: HashMap::new(),
            }
        }

        /// Spawn a player at (x, y) in Eden; `client` gives it a connection.
        fn player(&mut self, name: &str, x: f32, y: f32, aoi: f32, client: bool) -> EntityId {
            let conn = 1000 + self.world.zones[&Act::Eden].entities.len() as u64;
            let mut s = new_character(name);
            s.x = x;
            s.y = y;
            let id = self.world.spawn_player(conn, s);
            if client {
                self.reqs.push(ClientReq { conn, act: Act::Eden, entity: id, aoi, resync: false });
                self.dec.insert(conn, SnapshotDecoder::new());
            }
            id
        }

        fn conn_of(&self, entity: EntityId) -> u64 {
            self.reqs.iter().find(|r| r.entity == entity).unwrap().conn
        }

        /// One snapshot: every frame must decode.
        fn snap(&mut self) {
            let Self { world, hub, reqs, dec, bytes, last } = self;
            hub.run(world, reqs, 0.5, |conn, frame| {
                *bytes.entry(conn).or_default() += frame.len() as u64;
                let st = dec.get_mut(&conn).unwrap().apply(&frame).expect("frame decodes");
                last.insert(conn, st);
            });
            for r in reqs.iter_mut() {
                r.resync = false;
            }
        }

        /// Sim ticks with a snapshot every other one, like the server.
        fn run(&mut self, ticks: u32) {
            for t in 0..ticks {
                self.world.step();
                if t % 2 == 0 {
                    self.snap();
                }
            }
        }

        /// Snapshots until every client has scanned and every tier was sent.
        fn settle(&mut self) {
            for _ in 0..(SCAN_EVERY + FAR_PHASES) {
                self.snap();
            }
        }

        /// The decoder of `entity`'s client holds exactly what the sim has
        /// inside the AoI (to quantisation), and nothing outside the margin.
        fn assert_matches_sim(&self, entity: EntityId) {
            let r = self.reqs.iter().find(|r| r.entity == entity).unwrap();
            let d = &self.dec[&r.conn];
            let z = &self.world.zones[&Act::Eden];
            let center = z.entities[&entity].pos;
            let inside = |e: &&Entity| e.pos.distance(center) <= r.aoi - 1.0;
            let players_in = z.entities.values().filter(inside).filter(|e| e.kind == EntityKind::Player).count();
            let others_in = z.entities.values().filter(inside).count() - players_in;
            let mut expect = 0;
            for e in z.entities.values() {
                let dist = e.pos.distance(center);
                let got = d.get(e.id);
                // Over a cap only the nearest are sent: presence is then not
                // required, but whatever is held must still be right.
                let capped = if e.kind == EntityKind::Player {
                    players_in > MAX_PLAYERS_PER_CLIENT
                } else {
                    others_in > MAX_OTHERS_PER_CLIENT
                };
                if dist <= r.aoi - 1.0 {
                    if got.is_none() && capped {
                        continue;
                    }
                    expect += 1;
                    let got = got.unwrap_or_else(|| panic!("entity {} at {dist:.0} missing", e.id));
                    let want = e.to_state();
                    assert!((got.x - want.x).abs() <= 0.07 && (got.y - want.y).abs() <= 0.07, "pos of {}", e.id);
                    assert_eq!((got.health, got.max_health), (want.health, want.max_health), "health of {}", e.id);
                    assert_eq!(
                        (&got.name, &got.tag, &got.weapon, &got.look, got.kind, got.mounted),
                        (&want.name, &want.tag, &want.weapon, &want.look, want.kind, want.mounted)
                    );
                } else if dist > r.aoi + LEAVE_MARGIN + 1.0 {
                    assert!(got.is_none(), "entity {} at {dist:.0} should have left", e.id);
                }
            }
            assert!(d.len() >= expect, "decoder holds {} of {expect}", d.len());
            // Nothing the sim no longer has.
            for e in d.entities() {
                assert!(z.entities.contains_key(&e.id), "ghost entity {}", e.id);
            }
        }
    }

    #[test]
    fn first_frame_is_a_keyframe_holding_the_view() {
        let mut rig = Rig::new();
        let me = rig.player("Asa", 0.0, 0.0, 600.0, true);
        let near = rig.player("Boaz", 50.0, 0.0, 600.0, false);
        let far = rig.player("Cush", 2500.0, 0.0, 600.0, false);
        rig.snap();
        let conn = rig.conn_of(me);
        assert!(rig.last[&conn].key);
        let d = &rig.dec[&conn];
        assert!(d.get(me).is_some() && d.get(near).is_some());
        assert!(d.get(far).is_none(), "outside the AoI");
        assert_eq!(d.get(near).unwrap().name.as_deref(), Some("Boaz"));
        rig.assert_matches_sim(me);
        // Nothing moved: the next frames are bare headers.
        rig.snap();
        let st = rig.last[&conn];
        assert_eq!((st.key, st.enters, st.updates, st.leaves), (false, 0, 0, 0));
    }

    #[test]
    fn delta_follows_enter_leave_and_teleport() {
        let mut rig = Rig::new();
        let me = rig.player("Asa", 0.0, 0.0, 500.0, true);
        let walker = rig.player("Boaz", 2000.0, 0.0, 500.0, false);
        rig.settle();
        let conn = rig.conn_of(me);
        assert!(rig.dec[&conn].get(walker).is_none());

        // Teleport in: enters within one scan period.
        rig.world.dev_teleport(Act::Eden, walker, 100.0, 0.0);
        rig.settle();
        let got = rig.dec[&conn].get(walker).expect("entered").clone();
        assert!((got.x - 100.0).abs() < 0.1);
        rig.assert_matches_sim(me);

        // Walk about: position follows.
        rig.world.set_intent(Act::Eden, walker, Vec2::new(0.0, 1.0));
        rig.run(40);
        rig.world.set_intent(Act::Eden, walker, Vec2::ZERO);
        rig.run(8);
        assert!(rig.dec[&conn].get(walker).unwrap().y > 100.0, "moved north");
        rig.assert_matches_sim(me);

        // Teleport out: leaves.
        rig.world.dev_teleport(Act::Eden, walker, -3000.0, -3000.0);
        rig.settle();
        assert!(rig.dec[&conn].get(walker).is_none(), "left");
        rig.assert_matches_sim(me);

        // The watcher teleports: old surroundings leave, new ones enter.
        rig.world.dev_teleport(Act::Eden, me, -3000.0, -2900.0);
        rig.settle();
        assert!(rig.dec[&conn].get(walker).is_some(), "walker is near again");
        rig.assert_matches_sim(me);

        // Logout of the walker: leave on the very next snapshot.
        rig.world.remove_player(Act::Eden, walker);
        rig.snap();
        assert!(rig.dec[&conn].get(walker).is_none());
        assert_eq!(rig.last[&conn].leaves, 1);
        rig.assert_matches_sim(me);
    }

    #[test]
    fn a_live_world_stays_in_step_for_every_client() {
        let mut rig = Rig::new();
        let mut ids = Vec::new();
        for i in 0..40 {
            let a = i as f32 * 0.7;
            let id = rig.player(&format!("P{i}"), a.cos() * 30.0 * i as f32 % 900.0, a.sin() * 400.0, 700.0, i % 4 == 0);
            ids.push(id);
        }
        // Everyone walks, turning every second; mobs fight back.
        for round in 0..12 {
            for (i, id) in ids.iter().enumerate() {
                let a = (i * 7 + round * 3) as f32;
                rig.world.set_intent(Act::Eden, *id, Vec2::new(a.cos(), a.sin()));
                if (i + round) % 5 == 0 {
                    rig.world.queue_attack(Act::Eden, *id);
                }
            }
            rig.run(20);
        }
        for id in &ids {
            rig.world.set_intent(Act::Eden, *id, Vec2::ZERO);
        }
        // Let far-tier entities catch up, then compare everything.
        rig.run(2 * (SCAN_EVERY + FAR_PHASES) + 2);
        let mobs = rig.world.zones[&Act::Eden].entities.values().filter(|e| e.kind != EntityKind::Player).count();
        assert!(mobs > 0, "Eden has mobs");
        // Mobs keep wandering, so compare right after a snapshot with the
        // sim paused: settle() takes snapshots without stepping.
        rig.settle();
        let clients: Vec<EntityId> = rig.reqs.iter().map(|r| r.entity).collect();
        assert_eq!(clients.len(), 10);
        for c in clients {
            rig.assert_matches_sim(c);
        }
    }

    #[test]
    fn resync_gets_a_keyframe_and_is_rate_limited() {
        let mut rig = Rig::new();
        let me = rig.player("Asa", 0.0, 0.0, 600.0, true);
        let other = rig.player("Boaz", 40.0, 0.0, 600.0, false);
        rig.settle();
        let conn = rig.conn_of(me);
        // The client loses its baseline (a frame that did not decode).
        rig.dec.insert(conn, SnapshotDecoder::new());
        rig.reqs[0].resync = true;
        // Too soon after the login keyframe: deltas keep coming, which the
        // fresh decoder refuses; the request is remembered, not dropped.
        let mut refused = 0;
        let mut keyed_at = None;
        for i in 0..(RESYNC_MIN_GAP + 2) {
            let Rig { world, hub, reqs, dec, .. } = &mut rig;
            hub.run(world, reqs, 0.5, |c, frame| {
                match dec.get_mut(&c).unwrap().apply(&frame) {
                    Ok(st) => {
                        if st.key && keyed_at.is_none() {
                            keyed_at = Some(i);
                        }
                    }
                    Err(_) => refused += 1,
                }
            });
            reqs[0].resync = false;
        }
        assert!(refused > 0, "no keyframe inside the rate limit");
        assert!(keyed_at.is_some(), "keyframe arrived once the limit passed");
        assert!(rig.dec[&conn].synced());
        assert!(rig.dec[&conn].get(other).is_some());
        rig.assert_matches_sim(me);
    }

    #[test]
    fn periodic_keyframe_and_zone_change() {
        let mut rig = Rig::new();
        let me = rig.player("Asa", 0.0, 0.0, 600.0, true);
        let conn = rig.conn_of(me);
        let mut keys = 0;
        for _ in 0..(KEYFRAME_EVERY + 5) {
            rig.snap();
            if rig.last[&conn].key {
                keys += 1;
            }
        }
        assert_eq!(keys, 2, "login keyframe plus one periodic keyframe");

        // Travel: the client's entity moves to another act under a new id.
        let mut sheet = rig.world.remove_player(Act::Eden, me).unwrap();
        sheet.act = Act::Hermon;
        sheet.x = 0.0;
        sheet.y = 0.0;
        let me2 = rig.world.spawn_player(conn, sheet);
        rig.reqs[0].act = Act::Hermon;
        rig.reqs[0].entity = me2;
        rig.snap();
        assert!(rig.last[&conn].key, "new zone starts with a keyframe");
        let d = &rig.dec[&conn];
        assert_eq!(d.act(), Act::Hermon);
        assert!(d.get(me2).is_some() && d.get(me).is_none());
        let hermon = &rig.world.zones[&Act::Hermon];
        for e in d.entities() {
            assert!(hermon.entities.contains_key(&e.id), "entity from the old zone survived");
        }
    }

    #[test]
    fn nothing_is_read_where_nobody_looks() {
        let mut rig = Rig::new();
        let me = rig.player("Asa", 0.0, 0.0, 300.0, true);
        rig.settle();
        let zv = &rig.hub.zones[&Act::Eden];
        let live = zv.slots.iter().filter(|s| s.live).count();
        let total = rig.world.zones[&Act::Eden].entities.len();
        assert!(live >= 1 && live < total, "{live} of {total} entities read for one small AoI");
        eprintln!("one AoI of 300: {live} of {total} Eden entities read");
        assert!(!rig.hub.zones.contains_key(&Act::Flood), "no client, no view");
        // The client leaves: its view is dropped and the table empties.
        let _ = me;
        rig.reqs.clear();
        rig.snap();
        assert!(rig.hub.clients.is_empty());
        assert_eq!(rig.hub.zones[&Act::Eden].slots.iter().filter(|s| s.live).count(), 0);
    }

    #[test]
    fn caps_keep_the_nearest_and_tiers_slow_the_far() {
        let mut rig = Rig::new();
        let me = rig.player("Asa", 0.0, 0.0, 1400.0, true);
        // A ring of players inside the near radius and a crowd beyond it.
        let mut ring = Vec::new();
        for i in 0..(NEAR_PLAYERS + 40) {
            let a = i as f32 * 0.31;
            ring.push(rig.player(&format!("N{i}"), a.cos() * (60.0 + i as f32), a.sin() * (60.0 + i as f32), 0.0, false));
        }
        let mut crowd = Vec::new();
        for i in 0..200 {
            let a = i as f32 * 0.17;
            crowd.push(rig.player(&format!("F{i}"), a.cos() * 900.0, a.sin() * 900.0, 0.0, false));
        }
        rig.settle();
        let conn = rig.conn_of(me);
        let cv = &rig.hub.clients[&conn];
        let near_players = cv.near.iter().filter(|si| rig.hub.zones[&Act::Eden].slots[**si as usize].player).count();
        assert_eq!(near_players, NEAR_PLAYERS, "10 Hz tier is a budget, not a radius");
        rig.assert_matches_sim(me);
        // Everyone in the far crowd steps; over one snapshot only a phase
        // of them is sent, over FAR_PHASES snapshots all of them once.
        for (i, id) in crowd.iter().enumerate() {
            rig.world.dev_teleport(Act::Eden, *id, (i as f32 * 0.17).cos() * 905.0, (i as f32 * 0.17).sin() * 905.0);
        }
        // Align to just after a scan so the tier lists are stable.
        let mut sent = Vec::new();
        for _ in 0..FAR_PHASES {
            rig.snap();
            sent.push(rig.last[&conn].updates);
        }
        let total: u32 = sent.iter().sum();
        assert_eq!(total, 200, "each far player once per cycle: {sent:?}");
        assert!(sent.iter().all(|n| *n < 120), "spread over the phases: {sent:?}");
        rig.assert_matches_sim(me);
    }

    /// Byte budget for the goal: 1,000 players gathered on one spot, all
    /// walking and turning, one client watching with the full AoI.
    #[test]
    fn byte_budget_one_client_in_a_crowd_of_1000() {
        let mut rig = Rig::new();
        let me = rig.player("Asa", 0.0, 0.0, 1400.0, true);
        let mut ids = Vec::new();
        for i in 0..1000 {
            let a = i as f32 * 2.399_963; // golden angle: an even disc
            let rad = 18.0 * (i as f32).sqrt();
            ids.push(rig.player(&format!("Bot{i:04}"), a.cos() * rad, a.sin() * rad, 0.0, false));
        }
        rig.settle();
        let conn = rig.conn_of(me);
        let login_bytes = rig.bytes[&conn];
        let seen = rig.dec[&conn].entities().filter(|e| e.kind == EntityKind::Player).count();
        assert_eq!(seen, 1001, "the whole crowd is in view (cap was 150)");
        assert!(login_bytes < 120_000, "first sight of 1000 players cost {login_bytes} bytes");

        // 10 seconds of everyone walking out and back, new heading each second.
        let before = rig.bytes[&conn];
        for sec in 0..10u32 {
            for (i, id) in ids.iter().enumerate() {
                let a = (i as u32 * 37 + (sec / 2) * 101) as f32 * 0.01;
                let dir = if sec % 2 == 0 { 1.0 } else { -1.0 };
                rig.world.set_intent(Act::Eden, *id, Vec2::new(a.cos() * dir, a.sin() * dir));
            }
            rig.run(20);
        }
        let per_sec = (rig.bytes[&conn] - before) / 10;
        eprintln!("crowd of 1000, all moving: {per_sec} bytes/s for one client (login burst {login_bytes})");
        assert!(per_sec < 30_000, "{per_sec} bytes/s is over the 30 KB/s budget");
        // The JSON path at its 150 cap: 150 x 170 x 10 Hz.
        assert!(per_sec * 8 < 150 * 170 * 10);
        for id in &ids {
            rig.world.set_intent(Act::Eden, *id, Vec2::ZERO);
        }
        rig.run(2 * (SCAN_EVERY + FAR_PHASES) + 2);
        rig.settle();
        rig.assert_matches_sim(me);
    }
}
