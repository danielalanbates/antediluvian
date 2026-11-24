// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Binary delta snapshots (B11).
//!
//! The JSON `ServerMsg::Snapshot` repeats every field of every entity ten
//! times a second (about 170 bytes each). This module is the compact wire
//! the game client asks for at login (`ClientMsg::Login { wire: WIRE_BINARY }`):
//! one WebSocket **binary** message per snapshot, holding only what changed
//! since the last frame the same client was sent.
//!
//! ```text
//! frame  = magic:u8  flags:u8  act:u8  seq:varint  tick:varint  time_of_day:u16  op*
//! op     = ENTER  id:varint  dyn  meta          entity came into view
//!        | UPDATE|mask  id:varint  fields       only the fields named in mask
//!        | LEAVE  id:varint                     entity left view or died away
//! dyn    = x:u16  y:u16  rot:u8  health:zigzag  max_health:zigzag
//! meta   = kind:u8  present:u16  (strings, appearance, look ...)
//! ```
//!
//! Positions are quantised to an eighth of a world unit, facing to 1/256 of a
//! turn. A keyframe (`FLAG_KEY`) carries every entity as ENTER and replaces
//! the receiver's whole baseline; the receiver asks for one with
//! `ClientMsg::Resync` whenever a frame does not decode or arrives out of
//! sequence. Everything here is plain `std`, so it builds for wasm32 too.

use crate::{Act, EntityId, EntityKind, EntityState, ServerMsg, WORLD_BOUNDS};
use std::collections::HashMap;

/// `Login::wire` value: JSON text snapshots (test tools, old clients).
pub const WIRE_JSON: u8 = 0;
/// `Login::wire` value: binary delta snapshots as described in this module.
pub const WIRE_BINARY: u8 = 1;

/// First byte of every binary frame.
pub const FRAME_MAGIC: u8 = 0xAD;
/// Header flag: the frame is a keyframe (drop the baseline first).
pub const FLAG_KEY: u8 = 1;

/// World units per position step.
pub const POS_STEP: f32 = 0.125;

const OP_KIND: u8 = 0xC0;
const OP_UPDATE: u8 = 0x00;
const OP_ENTER: u8 = 0x40;
const OP_LEAVE: u8 = 0x80;

/// Update-mask bits: which fields follow.
pub const F_POS: u8 = 1;
pub const F_ROT: u8 = 2;
pub const F_HEALTH: u8 = 4;
pub const F_MAX_HEALTH: u8 = 8;
pub const F_META: u8 = 16;

// Meta "present" bits.
const M_TAG: u16 = 1;
const M_NAME: u16 = 2;
const M_WEAPON: u16 = 4;
const M_CHEST: u16 = 8;
const M_BACK: u16 = 16;
const M_FACTION: u16 = 32;
const M_APPEARANCE: u16 = 64;
const M_LOOK: u16 = 128;
const M_MOUNTED: u16 = 256;
const M_MOUNT_SPECIES: u16 = 512;

pub fn quant_pos(v: f32) -> u16 {
    let v = if v.is_finite() { v.clamp(-WORLD_BOUNDS, WORLD_BOUNDS) } else { 0.0 };
    ((v + WORLD_BOUNDS) / POS_STEP).round().min(65535.0) as u16
}

pub fn dequant_pos(q: u16) -> f32 {
    q as f32 * POS_STEP - WORLD_BOUNDS
}

pub fn quant_rot(r: f32) -> u8 {
    let r = if r.is_finite() { r } else { 0.0 };
    let turn = r.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
    ((turn * 256.0).round() as u32 & 255) as u8
}

/// Back to radians in (-pi, pi], the range the sim produces.
pub fn dequant_rot(q: u8) -> f32 {
    let r = q as f32 / 256.0 * std::f32::consts::TAU;
    if r > std::f32::consts::PI {
        r - std::f32::consts::TAU
    } else {
        r
    }
}

fn quant_tod(t: f32) -> u16 {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    (t * 65535.0).round() as u16
}

/// The fields of an entity that change from tick to tick, already quantised.
/// Two entities compare equal here exactly when a receiver could not tell
/// their frames apart, which is what the server uses to decide "changed".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Dyn {
    pub qx: u16,
    pub qy: u16,
    pub qrot: u8,
    pub health: i32,
    pub max_health: i32,
}

impl Dyn {
    pub fn new(x: f32, y: f32, rot: f32, health: i32, max_health: i32) -> Self {
        Self { qx: quant_pos(x), qy: quant_pos(y), qrot: quant_rot(rot), health, max_health }
    }
    pub fn of(e: &EntityState) -> Self {
        Self::new(e.x, e.y, e.rot, e.health, e.max_health)
    }
    /// Mask of the dynamic fields that differ from `old`.
    pub fn diff(&self, old: &Dyn) -> u8 {
        let mut m = 0;
        if (self.qx, self.qy) != (old.qx, old.qy) {
            m |= F_POS;
        }
        if self.qrot != old.qrot {
            m |= F_ROT;
        }
        if self.health != old.health {
            m |= F_HEALTH;
        }
        if self.max_health != old.max_health {
            m |= F_MAX_HEALTH;
        }
        m
    }
}

fn put_varint(buf: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        buf.push((v as u8) | 0x80);
        v >>= 7;
    }
    buf.push(v as u8);
}

fn zigzag(v: i32) -> u64 {
    ((v << 1) ^ (v >> 31)) as u32 as u64
}

fn unzigzag(v: u64) -> i32 {
    let v = v as u32;
    ((v >> 1) as i32) ^ -((v & 1) as i32)
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    // One length byte: cut over-long text on a character boundary.
    let mut n = s.len().min(255);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    buf.push(n as u8);
    buf.extend_from_slice(&s.as_bytes()[..n]);
}

fn kind_byte(k: EntityKind) -> u8 {
    match k {
        EntityKind::Player => 0,
        EntityKind::Enemy => 1,
        EntityKind::Wildlife => 2,
        EntityKind::Resource => 3,
        EntityKind::Npc => 4,
    }
}

fn kind_of(b: u8) -> Option<EntityKind> {
    Some(match b {
        0 => EntityKind::Player,
        1 => EntityKind::Enemy,
        2 => EntityKind::Wildlife,
        3 => EntityKind::Resource,
        4 => EntityKind::Npc,
        _ => return None,
    })
}

fn act_byte(a: Act) -> u8 {
    Act::ALL.iter().position(|x| *x == a).unwrap_or(0) as u8
}

/// Append the rarely-changing part of an entity (kind, names, gear, look).
/// The server caches these bytes per entity and copies them into every
/// ENTER, so a crowd coming into view is a memcpy per player.
pub fn encode_meta(e: &EntityState, buf: &mut Vec<u8>) {
    buf.push(kind_byte(e.kind));
    let mut present = 0u16;
    let opt = |o: &Option<String>, bit: u16| if o.is_some() { bit } else { 0 };
    present |= opt(&e.tag, M_TAG);
    present |= opt(&e.name, M_NAME);
    present |= opt(&e.weapon, M_WEAPON);
    present |= opt(&e.chest, M_CHEST);
    present |= opt(&e.back, M_BACK);
    present |= opt(&e.faction, M_FACTION);
    if e.appearance.is_some() {
        present |= M_APPEARANCE;
    }
    if e.look.is_some() {
        present |= M_LOOK;
    }
    if e.mounted {
        present |= M_MOUNTED;
    }
    present |= opt(&e.mount_species, M_MOUNT_SPECIES);
    buf.extend_from_slice(&present.to_le_bytes());
    for s in [&e.tag, &e.name, &e.weapon, &e.chest, &e.back, &e.faction] {
        if let Some(s) = s {
            put_str(buf, s);
        }
    }
    if let Some(a) = e.appearance {
        for v in a {
            put_varint(buf, v as u64);
        }
    }
    if let Some(l) = e.look {
        put_varint(buf, l);
    }
    if let Some(s) = &e.mount_species {
        put_str(buf, s);
    }
}

/// Builds one binary frame.
pub struct FrameWriter {
    buf: Vec<u8>,
    ops: u32,
}

impl FrameWriter {
    pub fn begin(key: bool, act: Act, seq: u32, tick: u64, time_of_day: f32) -> Self {
        let mut buf = Vec::with_capacity(256);
        buf.push(FRAME_MAGIC);
        buf.push(if key { FLAG_KEY } else { 0 });
        buf.push(act_byte(act));
        put_varint(&mut buf, seq as u64);
        put_varint(&mut buf, tick);
        buf.extend_from_slice(&quant_tod(time_of_day).to_le_bytes());
        Self { buf, ops: 0 }
    }

    fn put_dyn(&mut self, d: Dyn) {
        self.buf.extend_from_slice(&d.qx.to_le_bytes());
        self.buf.extend_from_slice(&d.qy.to_le_bytes());
        self.buf.push(d.qrot);
        put_varint(&mut self.buf, zigzag(d.health));
        put_varint(&mut self.buf, zigzag(d.max_health));
    }

    /// Entity came into view. `meta` is `encode_meta` output.
    pub fn enter(&mut self, id: EntityId, d: Dyn, meta: &[u8]) {
        self.buf.push(OP_ENTER);
        put_varint(&mut self.buf, id);
        self.put_dyn(d);
        self.buf.extend_from_slice(meta);
        self.ops += 1;
    }

    /// Convenience for tools and tests: ENTER straight from an `EntityState`.
    pub fn enter_state(&mut self, e: &EntityState) {
        let mut meta = Vec::with_capacity(48);
        encode_meta(e, &mut meta);
        self.enter(e.id, Dyn::of(e), &meta);
    }

    /// Changed fields of a known entity. `meta` is read only with `F_META`.
    pub fn update(&mut self, id: EntityId, mask: u8, d: Dyn, meta: &[u8]) {
        let mask = mask & !OP_KIND;
        if mask == 0 {
            return;
        }
        self.buf.push(OP_UPDATE | mask);
        put_varint(&mut self.buf, id);
        if mask & F_POS != 0 {
            self.buf.extend_from_slice(&d.qx.to_le_bytes());
            self.buf.extend_from_slice(&d.qy.to_le_bytes());
        }
        if mask & F_ROT != 0 {
            self.buf.push(d.qrot);
        }
        if mask & F_HEALTH != 0 {
            put_varint(&mut self.buf, zigzag(d.health));
        }
        if mask & F_MAX_HEALTH != 0 {
            put_varint(&mut self.buf, zigzag(d.max_health));
        }
        if mask & F_META != 0 {
            self.buf.extend_from_slice(meta);
        }
        self.ops += 1;
    }

    pub fn leave(&mut self, id: EntityId) {
        self.buf.push(OP_LEAVE);
        put_varint(&mut self.buf, id);
        self.ops += 1;
    }

    /// Enter, update and leave records written so far.
    pub fn ops(&self) -> u32 {
        self.ops
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops == 0
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

/// Why a frame could not be applied. After any of these the decoder is out
/// of sync and ignores deltas until the next keyframe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    Truncated,
    BadMagic,
    BadAct(u8),
    BadKind(u8),
    BadOp(u8),
    BadText,
    /// A delta arrived while no baseline is held (or after an earlier error).
    NotSynced,
    /// Frame sequence skipped: a delta was lost.
    SeqGap { want: u32, got: u32 },
    /// UPDATE for an entity the baseline does not hold.
    UnknownEntity(EntityId),
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn done(&self) -> bool {
        self.i >= self.b.len()
    }
    fn u8(&mut self) -> Result<u8, WireError> {
        let v = *self.b.get(self.i).ok_or(WireError::Truncated)?;
        self.i += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, WireError> {
        let lo = self.u8()?;
        let hi = self.u8()?;
        Ok(u16::from_le_bytes([lo, hi]))
    }
    fn varint(&mut self) -> Result<u64, WireError> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.u8()?;
            v |= ((b & 0x7F) as u64) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(WireError::Truncated)
    }
    fn string(&mut self) -> Result<String, WireError> {
        let n = self.u8()? as usize;
        let end = self.i + n;
        let s = self.b.get(self.i..end).ok_or(WireError::Truncated)?;
        self.i = end;
        std::str::from_utf8(s).map(str::to_string).map_err(|_| WireError::BadText)
    }
    fn dynamic(&mut self) -> Result<Dyn, WireError> {
        Ok(Dyn {
            qx: self.u16()?,
            qy: self.u16()?,
            qrot: self.u8()?,
            health: unzigzag(self.varint()?),
            max_health: unzigzag(self.varint()?),
        })
    }
    /// Read a meta block into `e` (every meta field is overwritten).
    fn meta(&mut self, e: &mut EntityState) -> Result<(), WireError> {
        let k = self.u8()?;
        e.kind = kind_of(k).ok_or(WireError::BadKind(k))?;
        let present = self.u16()?;
        let text = |r: &mut Self, bit: u16| -> Result<Option<String>, WireError> {
            if present & bit != 0 {
                r.string().map(Some)
            } else {
                Ok(None)
            }
        };
        e.tag = text(self, M_TAG)?;
        e.name = text(self, M_NAME)?;
        e.weapon = text(self, M_WEAPON)?;
        e.chest = text(self, M_CHEST)?;
        e.back = text(self, M_BACK)?;
        e.faction = text(self, M_FACTION)?;
        e.appearance = if present & M_APPEARANCE != 0 {
            Some([self.varint()? as u32, self.varint()? as u32, self.varint()? as u32])
        } else {
            None
        };
        e.look = if present & M_LOOK != 0 { Some(self.varint()?) } else { None };
        e.mounted = present & M_MOUNTED != 0;
        e.mount_species = text(self, M_MOUNT_SPECIES)?;
        Ok(())
    }
}

/// What one applied frame did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameStats {
    pub key: bool,
    pub enters: u32,
    pub updates: u32,
    pub leaves: u32,
}

/// Receiver side: holds the baseline and applies frames to it.
pub struct SnapshotDecoder {
    ents: HashMap<EntityId, EntityState>,
    act: Act,
    tick: u64,
    time_of_day: f32,
    next_seq: u32,
    synced: bool,
}

impl Default for SnapshotDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotDecoder {
    pub fn new() -> Self {
        Self {
            ents: HashMap::new(),
            act: Act::Eden,
            tick: 0,
            time_of_day: 0.0,
            next_seq: 0,
            synced: false,
        }
    }

    /// False until the first keyframe, and again after any error: the owner
    /// should send `ClientMsg::Resync`.
    pub fn synced(&self) -> bool {
        self.synced
    }

    pub fn len(&self) -> usize {
        self.ents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ents.is_empty()
    }

    pub fn get(&self, id: EntityId) -> Option<&EntityState> {
        self.ents.get(&id)
    }

    pub fn entities(&self) -> impl Iterator<Item = &EntityState> {
        self.ents.values()
    }

    pub fn act(&self) -> Act {
        self.act
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// The whole view as the JSON path would have sent it.
    pub fn snapshot(&self) -> ServerMsg {
        ServerMsg::Snapshot {
            act: self.act,
            tick: self.tick,
            time_of_day: self.time_of_day,
            entities: self.ents.values().cloned().collect(),
        }
    }

    /// Apply one binary frame to the baseline.
    pub fn apply(&mut self, frame: &[u8]) -> Result<FrameStats, WireError> {
        let res = self.apply_inner(frame);
        if res.is_err() {
            // The baseline may be half-updated: only a keyframe mends it.
            self.synced = false;
        }
        res
    }

    fn apply_inner(&mut self, frame: &[u8]) -> Result<FrameStats, WireError> {
        let mut r = Reader { b: frame, i: 0 };
        if r.u8()? != FRAME_MAGIC {
            return Err(WireError::BadMagic);
        }
        let flags = r.u8()?;
        let act_b = r.u8()?;
        let act = *Act::ALL.get(act_b as usize).ok_or(WireError::BadAct(act_b))?;
        let seq = r.varint()? as u32;
        let tick = r.varint()?;
        let tod = r.u16()? as f32 / 65535.0;
        let key = flags & FLAG_KEY != 0;
        if key {
            self.ents.clear();
            self.synced = true;
        } else if !self.synced {
            return Err(WireError::NotSynced);
        } else if seq != self.next_seq {
            return Err(WireError::SeqGap { want: self.next_seq, got: seq });
        }
        self.next_seq = seq.wrapping_add(1);
        self.act = act;
        self.tick = tick;
        self.time_of_day = tod;
        let mut st = FrameStats { key, ..Default::default() };
        while !r.done() {
            let op = r.u8()?;
            let id = r.varint()?;
            match op & OP_KIND {
                OP_ENTER => {
                    let d = r.dynamic()?;
                    let mut e = EntityState {
                        id,
                        kind: EntityKind::Npc,
                        x: dequant_pos(d.qx),
                        y: dequant_pos(d.qy),
                        rot: dequant_rot(d.qrot),
                        health: d.health,
                        max_health: d.max_health,
                        tag: None,
                        name: None,
                        weapon: None,
                        chest: None,
                        back: None,
                        faction: None,
                        appearance: None,
                        look: None,
                        mounted: false,
                        mount_species: None,
                    };
                    r.meta(&mut e)?;
                    self.ents.insert(id, e);
                    st.enters += 1;
                }
                OP_UPDATE => {
                    let e = self.ents.get_mut(&id).ok_or(WireError::UnknownEntity(id))?;
                    if op & F_POS != 0 {
                        e.x = dequant_pos(r.u16()?);
                        e.y = dequant_pos(r.u16()?);
                    }
                    if op & F_ROT != 0 {
                        e.rot = dequant_rot(r.u8()?);
                    }
                    if op & F_HEALTH != 0 {
                        e.health = unzigzag(r.varint()?);
                    }
                    if op & F_MAX_HEALTH != 0 {
                        e.max_health = unzigzag(r.varint()?);
                    }
                    if op & F_META != 0 {
                        r.meta(e)?;
                    }
                    st.updates += 1;
                }
                OP_LEAVE => {
                    // Leaving something already gone is harmless.
                    self.ents.remove(&id);
                    st.leaves += 1;
                }
                _ => return Err(WireError::BadOp(op)),
            }
        }
        Ok(st)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(id: EntityId, x: f32, y: f32) -> EntityState {
        EntityState {
            id,
            kind: EntityKind::Player,
            x,
            y,
            rot: 1.0,
            health: 100,
            max_health: 100,
            tag: Some("warrior".into()),
            name: Some(format!("Bot{id:04}")),
            weapon: Some("bronze_sword".into()),
            chest: Some("skin_tunic".into()),
            back: None,
            faction: Some("sethite".into()),
            appearance: Some([2, 7, 3]),
            look: Some(crate::Look::random(id).pack()),
            mounted: false,
            mount_species: None,
        }
    }

    fn mob(id: EntityId) -> EntityState {
        EntityState {
            id,
            kind: EntityKind::Enemy,
            x: -3600.0,
            y: 3600.0,
            rot: -3.0,
            health: -5,
            max_health: 40,
            tag: Some("starving_smilodon".into()),
            name: None,
            weapon: None,
            chest: None,
            back: None,
            faction: None,
            appearance: None,
            look: None,
            mounted: false,
            mount_species: None,
        }
    }

    /// What a receiver must end up holding for `e`.
    fn quantised(e: &EntityState) -> EntityState {
        let d = Dyn::of(e);
        EntityState { x: dequant_pos(d.qx), y: dequant_pos(d.qy), rot: dequant_rot(d.qrot), ..e.clone() }
    }

    #[test]
    fn quantisation_stays_within_half_a_step() {
        for v in [-3600.0f32, -3599.94, -1.0, 0.0, 0.06, 17.3, 1234.567, 3600.0] {
            let back = dequant_pos(quant_pos(v));
            assert!((back - v).abs() <= POS_STEP / 2.0 + 1e-3, "{v} -> {back}");
        }
        // Out-of-range and non-finite input cannot wrap or panic.
        assert_eq!(dequant_pos(quant_pos(99999.0)), WORLD_BOUNDS);
        assert_eq!(dequant_pos(quant_pos(-99999.0)), -WORLD_BOUNDS);
        assert_eq!(quant_pos(f32::NAN), quant_pos(0.0));
        for r in [-3.14f32, -1.0, 0.0, 0.5, 3.14, 7.0, -9.0] {
            let back = dequant_rot(quant_rot(r));
            let err = (back - r).rem_euclid(std::f32::consts::TAU);
            let err = err.min(std::f32::consts::TAU - err);
            assert!(err <= std::f32::consts::TAU / 512.0 + 1e-4, "{r} -> {back}");
            assert!(back > -std::f32::consts::PI - 1e-4 && back <= std::f32::consts::PI + 1e-4);
        }
        for v in [0, 1, -1, 100, -100, i32::MAX, i32::MIN] {
            assert_eq!(unzigzag(zigzag(v)), v);
        }
    }

    #[test]
    fn keyframe_round_trips_every_field() {
        let mut rider = player(70_000, 12.5, -900.25);
        rider.mounted = true;
        rider.mount_species = Some("dire_wolf".into());
        rider.back = Some("lineage_mantle".into());
        let ents = vec![player(1, 0.0, 0.0), rider, mob(300)];
        let mut w = FrameWriter::begin(true, Act::Hermon, 9, 123_456, 0.25);
        for e in &ents {
            w.enter_state(e);
        }
        let mut d = SnapshotDecoder::new();
        let st = d.apply(&w.finish()).unwrap();
        assert_eq!(st, FrameStats { key: true, enters: 3, updates: 0, leaves: 0 });
        assert_eq!((d.act(), d.tick(), d.len()), (Act::Hermon, 123_456, 3));
        for e in &ents {
            assert_eq!(d.get(e.id), Some(&quantised(e)));
        }
        match d.snapshot() {
            ServerMsg::Snapshot { act, tick, time_of_day, entities } => {
                assert_eq!((act, tick, entities.len()), (Act::Hermon, 123_456, 3));
                assert!((time_of_day - 0.25).abs() < 1e-4);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn delta_touches_only_the_masked_fields() {
        let a = player(5, 100.0, 200.0);
        let mut w = FrameWriter::begin(true, Act::Eden, 0, 1, 0.5);
        w.enter_state(&a);
        w.enter_state(&mob(6));
        let mut d = SnapshotDecoder::new();
        d.apply(&w.finish()).unwrap();

        // Move only: name, gear and health must survive untouched.
        let mut b = a.clone();
        b.x = 110.0;
        b.y = 190.0;
        let mut w = FrameWriter::begin(false, Act::Eden, 1, 2, 0.5);
        w.update(5, Dyn::of(&b).diff(&Dyn::of(&a)), Dyn::of(&b), &[]);
        assert_eq!(Dyn::of(&b).diff(&Dyn::of(&a)), F_POS);
        d.apply(&w.finish()).unwrap();
        assert_eq!(d.get(5), Some(&quantised(&b)));

        // Health and gear change, position does not.
        let mut c = b.clone();
        c.health = 37;
        c.weapon = None;
        c.mounted = true;
        c.mount_species = Some("ox".into());
        let mut meta = Vec::new();
        encode_meta(&c, &mut meta);
        let mut w = FrameWriter::begin(false, Act::Eden, 2, 3, 0.5);
        w.update(5, Dyn::of(&c).diff(&Dyn::of(&b)) | F_META, Dyn::of(&c), &meta);
        w.leave(6);
        let st = d.apply(&w.finish()).unwrap();
        assert_eq!((st.updates, st.leaves), (1, 1));
        assert_eq!(d.get(5), Some(&quantised(&c)));
        assert!(d.get(6).is_none());
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn a_lost_or_broken_frame_desyncs_until_the_next_keyframe() {
        let a = player(5, 0.0, 0.0);
        let key = |seq| {
            let mut w = FrameWriter::begin(true, Act::Eden, seq, 1, 0.0);
            w.enter_state(&a);
            w.finish()
        };
        let delta = |seq, x: f32| {
            let mut w = FrameWriter::begin(false, Act::Eden, seq, 2, 0.0);
            w.update(5, F_POS, Dyn::new(x, 0.0, 0.0, 1, 1), &[]);
            w.finish()
        };
        let mut d = SnapshotDecoder::new();
        // Deltas before any keyframe are refused.
        assert_eq!(d.apply(&delta(0, 1.0)), Err(WireError::NotSynced));
        assert!(!d.synced());
        d.apply(&key(10)).unwrap();
        d.apply(&delta(11, 5.0)).unwrap();
        // Frame 12 is lost.
        assert_eq!(d.apply(&delta(13, 9.0)), Err(WireError::SeqGap { want: 12, got: 13 }));
        assert!(!d.synced());
        assert_eq!(d.apply(&delta(14, 9.0)), Err(WireError::NotSynced));
        assert_eq!(d.get(5).unwrap().x, 5.0, "a refused frame changes nothing");
        // The keyframe that answers Resync mends it.
        d.apply(&key(15)).unwrap();
        assert!(d.synced());
        d.apply(&delta(16, 9.0)).unwrap();
        assert_eq!(d.get(5).unwrap().x, 9.0);

        // An update for an entity never entered is a desync, not a panic.
        let mut w = FrameWriter::begin(false, Act::Eden, 17, 3, 0.0);
        w.update(99, F_HEALTH, Dyn::default(), &[]);
        assert_eq!(d.apply(&w.finish()), Err(WireError::UnknownEntity(99)));
        assert!(!d.synced());
        // Garbage and cut-off frames are errors too.
        assert_eq!(d.apply(&[1, 2, 3]), Err(WireError::BadMagic));
        let k = key(20);
        assert_eq!(d.apply(&k[..k.len() - 3]), Err(WireError::Truncated));
        assert_eq!(d.apply(&[]), Err(WireError::Truncated));
    }

    #[test]
    fn over_long_text_is_cut_on_a_character_boundary() {
        let mut e = mob(1);
        e.name = Some("\u{00e9}".repeat(200)); // 400 bytes of two-byte characters
        let mut w = FrameWriter::begin(true, Act::Eden, 0, 0, 0.0);
        w.enter_state(&e);
        let mut d = SnapshotDecoder::new();
        d.apply(&w.finish()).unwrap();
        assert_eq!(d.get(1).unwrap().name.as_deref().unwrap().chars().count(), 127);
    }

    /// Byte budget, per entity. The JSON path costs about 170 bytes for the
    /// same player every snapshot (`sim::snapshot_size_tests`).
    #[test]
    fn byte_budget_per_entity() {
        let header = FrameWriter::begin(false, Act::Eden, 60_000, 2_000_000, 0.5).len();
        assert!(header <= 12, "header {header}");
        let a = player(20_000, 10.0, 10.0);
        // A walking player: position and facing.
        let mut w = FrameWriter::begin(false, Act::Eden, 60_000, 2_000_000, 0.5);
        w.update(a.id, F_POS | F_ROT, Dyn::of(&a), &[]);
        let moving = w.len() - header;
        assert!(moving <= 9, "moving update {moving} bytes");
        // Position only.
        let mut w = FrameWriter::begin(false, Act::Eden, 60_000, 2_000_000, 0.5);
        w.update(a.id, F_POS, Dyn::of(&a), &[]);
        assert!(w.len() - header <= 8);
        // First sight of a fully dressed player.
        let mut w = FrameWriter::begin(false, Act::Eden, 60_000, 2_000_000, 0.5);
        w.enter_state(&a);
        let enter = w.len() - header;
        assert!(enter <= 80, "enter {enter} bytes");
        let json = serde_json_len(&a);
        assert!(enter * 2 < json, "enter {enter} vs json {json}");
        assert!(moving * 15 < json, "moving {moving} vs json {json}");
        eprintln!("wire bytes: header {header}, moving update {moving}, enter {enter}, json {json}");
    }

    /// Length of the JSON form without pulling serde_json into this crate:
    /// field names and punctuation as the server writes them.
    fn serde_json_len(e: &EntityState) -> usize {
        format!(
            "{{\"id\":{},\"kind\":\"player\",\"x\":{},\"y\":{},\"rot\":{},\"health\":{},\"max_health\":{},\"tag\":\"{}\",\"name\":\"{}\",\"weapon\":\"{}\",\"chest\":\"{}\",\"faction\":\"{}\",\"appearance\":[2,7,3],\"look\":{}}}",
            e.id, e.x, e.y, e.rot, e.health, e.max_health,
            e.tag.as_deref().unwrap(), e.name.as_deref().unwrap(), e.weapon.as_deref().unwrap(),
            e.chest.as_deref().unwrap(), e.faction.as_deref().unwrap(), e.look.unwrap()
        )
        .len()
    }
}
