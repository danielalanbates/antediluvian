// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Character look: every character-creation choice beyond class, lineage and
//! outfit (the outfit is `appearance[0]`, kept where it always was).
//!
//! On the wire, in the sheet and in the database a look is ONE `u64`: twelve
//! small indices packed into 51 bits. That keeps the per-player cost in a
//! snapshot to a single number, and the three-place rule to a single field.
//! The server never trusts the number: [`Look::clamped`] folds every index
//! into its valid range before a character is saved.

/// One field of the packed look: its name, bit width and number of options.
#[derive(Debug, Clone, Copy)]
pub struct LookField {
    pub name: &'static str,
    pub bits: u32,
    pub count: u8,
}

/// Packing order (low bits first). Appending a field is compatible; changing
/// a width or reordering is not.
pub const LOOK_FIELDS: [LookField; 12] = [
    LookField { name: "face", bits: 5, count: 24 },
    LookField { name: "hair", bits: 5, count: 32 },
    LookField { name: "hair_color", bits: 5, count: 24 },
    LookField { name: "beard", bits: 4, count: 14 },
    LookField { name: "skin", bits: 5, count: 20 },
    LookField { name: "build", bits: 3, count: 8 },
    LookField { name: "height", bits: 4, count: 9 },
    LookField { name: "marking", bits: 5, count: 20 },
    LookField { name: "mark_color", bits: 3, count: 8 },
    LookField { name: "eyes", bits: 4, count: 14 },
    LookField { name: "mantle", bits: 3, count: 6 },
    LookField { name: "dye", bits: 5, count: 24 },
];

/// Outfits (`appearance[0]`): the four starting garments.
pub const OUTFITS: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Look {
    pub face: u8,
    pub hair: u8,
    pub hair_color: u8,
    pub beard: u8,
    pub skin: u8,
    pub build: u8,
    pub height: u8,
    pub marking: u8,
    pub mark_color: u8,
    pub eyes: u8,
    pub mantle: u8,
    pub dye: u8,
}

fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

impl Look {
    pub fn to_array(self) -> [u8; 12] {
        [
            self.face, self.hair, self.hair_color, self.beard, self.skin, self.build,
            self.height, self.marking, self.mark_color, self.eyes, self.mantle, self.dye,
        ]
    }

    pub fn from_array(a: [u8; 12]) -> Self {
        Self {
            face: a[0],
            hair: a[1],
            hair_color: a[2],
            beard: a[3],
            skin: a[4],
            build: a[5],
            height: a[6],
            marking: a[7],
            mark_color: a[8],
            eyes: a[9],
            mantle: a[10],
            dye: a[11],
        }
    }

    /// Every index folded into its valid range (out-of-range wraps).
    pub fn clamped(self) -> Self {
        let mut a = self.to_array();
        for (v, f) in a.iter_mut().zip(LOOK_FIELDS.iter()) {
            *v %= f.count;
        }
        Self::from_array(a)
    }

    pub fn pack(self) -> u64 {
        let mut out = 0u64;
        let mut shift = 0;
        for (v, f) in self.to_array().iter().zip(LOOK_FIELDS.iter()) {
            out |= ((*v as u64) & ((1u64 << f.bits) - 1)) << shift;
            shift += f.bits;
        }
        out
    }

    /// Decode a packed look. Always valid: unknown high bits are ignored and
    /// every index is clamped, so a hostile or stale number cannot produce an
    /// out-of-range option.
    pub fn unpack(packed: u64) -> Self {
        let mut a = [0u8; 12];
        let mut shift = 0;
        for (v, f) in a.iter_mut().zip(LOOK_FIELDS.iter()) {
            *v = ((packed >> shift) & ((1u64 << f.bits) - 1)) as u8;
            shift += f.bits;
        }
        Self::from_array(a).clamped()
    }

    /// The server-side gate: whatever a client sent, this is what is stored.
    pub fn sanitize(packed: u64) -> u64 {
        Self::unpack(packed).pack()
    }

    /// A stable pseudo-random look for a seed (the builder's Randomise
    /// button, and characters made without the builder).
    pub fn random(seed: u64) -> Self {
        let mut a = [0u8; 12];
        for (i, (v, f)) in a.iter_mut().zip(LOOK_FIELDS.iter()).enumerate() {
            let r = mix(seed ^ ((i as u64 + 1) << 40));
            *v = (r % f.count as u64) as u8;
        }
        let mut l = Self::from_array(a);
        // Most people have no facial hair or war paint; a uniform roll made
        // nearly every random character bearded and painted.
        if mix(seed ^ 0xBEA2D) % 5 < 3 {
            l.beard = 0;
        }
        if mix(seed ^ 0x3A2C) % 5 < 3 {
            l.marking = 0;
        }
        l
    }

    /// Looks for characters that predate this field: the old skin and hair
    /// indices carry over, everything else stays at the first option.
    pub fn from_legacy(appearance: [u32; 3]) -> Self {
        Self {
            skin: (appearance[1] % LOOK_FIELDS[4].count as u32) as u8,
            hair: (appearance[2] % LOOK_FIELDS[1].count as u32) as u8,
            hair_color: (appearance[2] % LOOK_FIELDS[2].count as u32) as u8,
            ..Self::default()
        }
    }

    /// Stable seed for a character name (characters made without the builder).
    pub fn seed_for_name(name: &str) -> u64 {
        name.to_lowercase()
            .bytes()
            .fold(0xCBF2_9CE4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01B3))
    }

    /// Number of individually selectable options (sum over fields, plus outfits).
    pub fn option_count() -> u32 {
        LOOK_FIELDS.iter().map(|f| f.count as u32).sum::<u32>() + OUTFITS
    }

    /// Number of distinct combinations (product over fields, times outfits).
    pub fn combinations() -> u128 {
        LOOK_FIELDS.iter().map(|f| f.count as u128).product::<u128>() * OUTFITS as u128
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_hold_every_option_and_fit_in_53_bits() {
        let mut bits = 0;
        for f in LOOK_FIELDS {
            assert!((f.count as u64) <= (1u64 << f.bits), "{} does not fit its bits", f.name);
            assert!(f.count >= 2, "{} has no choice", f.name);
            bits += f.bits;
        }
        // JSON numbers are exact up to 2^53 in every client (browsers included).
        assert!(bits <= 53, "packed look is {bits} bits");
    }

    #[test]
    fn pack_round_trips_every_option() {
        for (i, f) in LOOK_FIELDS.iter().enumerate() {
            for v in 0..f.count {
                let mut a = [0u8; 12];
                a[i] = v;
                let l = Look::from_array(a);
                assert_eq!(Look::unpack(l.pack()), l, "{} = {v}", f.name);
            }
        }
        for s in 0..2000u64 {
            let l = Look::random(s);
            assert_eq!(l, l.clamped());
            assert_eq!(Look::unpack(l.pack()), l);
        }
    }

    #[test]
    fn hostile_numbers_are_clamped() {
        for packed in [u64::MAX, 0xFFFF_FFFF_FFFF, 1 << 60, 0x1F, 31 << 5] {
            let l = Look::unpack(packed);
            for (v, f) in l.to_array().iter().zip(LOOK_FIELDS.iter()) {
                assert!(*v < f.count, "{} = {v} escaped the clamp", f.name);
            }
            // Sanitising twice changes nothing.
            assert_eq!(Look::sanitize(Look::sanitize(packed)), Look::sanitize(packed));
        }
    }

    #[test]
    fn hundreds_of_options() {
        assert!(Look::option_count() >= 200, "only {} options", Look::option_count());
        assert!(Look::combinations() > 1_000_000_000_000);
    }

    #[test]
    fn random_looks_are_spread() {
        let mut seen = std::collections::HashSet::new();
        for s in 0..5000u64 {
            seen.insert(Look::random(s).pack());
        }
        assert!(seen.len() >= 4990, "only {} distinct random looks in 5000", seen.len());
        // Every option of every field is reachable by the randomiser.
        for (i, f) in LOOK_FIELDS.iter().enumerate() {
            let hit: std::collections::HashSet<u8> =
                (0..20_000u64).map(|s| Look::random(s).to_array()[i]).collect();
            assert_eq!(hit.len(), f.count as usize, "{} has unreachable options", f.name);
        }
    }

    #[test]
    fn legacy_appearance_maps_into_range() {
        let l = Look::from_legacy([3, 15, 11]);
        assert_eq!(l, l.clamped());
        assert_eq!(l.skin, 15);
        assert_eq!(l.hair, 11);
    }
}
