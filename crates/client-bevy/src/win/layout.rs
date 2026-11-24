// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Pure layout maths for the windows: where each one sits, colour parsing,
//! tooltip placement. No Bevy systems here, so all of it is unit-tested.

use super::Win;
use antediluvia_protocol::Rarity;
use bevy::prelude::Color;

/// Screen rectangle of a window: `h` is the maximum height (content clips).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// HUD the windows must stay clear of: unit + target frames (top-left), the
/// minimap column (top-right), the action bar and chat (bottom).
const TOP: f32 = 196.0;
const BOTTOM: f32 = 132.0;
const MARGIN: f32 = 14.0;

/// Preferred (width, height, column) of a window. Columns: 0 = left edge,
/// 1 = left of centre, 2 = centred, 3 = right edge, 4 = right of the quest log.
fn pref(win: Win) -> (f32, f32, u8) {
    match win {
        Win::Character => (340.0, 560.0, 0),
        Win::Vendor => (340.0, 560.0, 0),
        Win::QuestLog => (400.0, 560.0, 1),
        Win::Spellbook => (430.0, 700.0, 4),
        Win::Board => (460.0, 560.0, 1),
        Win::Flight => (360.0, 520.0, 1),
        Win::Map => (604.0, 700.0, 2),
        Win::Loot => (260.0, 260.0, 2),
        Win::Social => (380.0, 520.0, 3),
        Win::Bags => (336.0, 420.0, 3),
    }
}

/// Windows too tall for the band between the unit frame and the action bar.
pub fn tall(win: Win) -> bool {
    matches!(win, Win::Map | Win::Spellbook)
}

/// Where a window sits on a `sw` x `sh` screen. Always fully on screen for
/// any screen at least 640x480; shrinks rather than overflowing.
pub fn rect(win: Win, sw: f32, sh: f32) -> Rect {
    let (pw, ph, col) = pref(win);
    let w = pw.min(sw - 2.0 * MARGIN).max(120.0);
    // The map and spellbook are tall: they sit right of the unit frame and
    // may use the band beside it as well.
    let top = if tall(win) { 60.0 } else { TOP.min(sh * 0.25) };
    let avail = (sh - top - BOTTOM.min(sh * 0.2)).max(120.0);
    let h = ph.min(avail);
    let x = match col {
        0 => MARGIN,
        1 => MARGIN + 356.0,
        2 => (sw - w) / 2.0,
        4 => MARGIN + 356.0 + 414.0,
        _ => sw - w - MARGIN,
    };
    // Bags hug the bottom-right corner like a classic backpack; the social
    // window takes the space to their left.
    let (x, y) = match win {
        Win::Bags => (x, (sh - BOTTOM.min(sh * 0.2) - h).max(top)),
        Win::Social => (x - 350.0, top),
        Win::Loot => (x, (sh * 0.5).min(sh - h - MARGIN)),
        _ => (x, top),
    };
    let x = x.min(sw - w - MARGIN).max(MARGIN.min((sw - w) / 2.0));
    Rect { x, y, w, h }
}

/// Parse `#rrggbb` into linear-free sRGB components in 0..=1.
pub fn hex_rgb(hex: &str) -> Option<[f32; 3]> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|v| v as f32 / 255.0);
    Some([p(0)?, p(2)?, p(4)?])
}

/// Classic quality colour for a rarity tier (from the protocol's table, so
/// the client can never disagree with the server's `ItemInfo.color`).
pub fn rarity_color(r: Rarity) -> Color {
    let [red, green, blue] = hex_rgb(r.color()).unwrap_or([1.0, 1.0, 1.0]);
    Color::srgb(red, green, blue)
}

/// Durability colour: red when broken, amber below a quarter, else plain.
pub fn wear_color(cur: u32, max: u32) -> Color {
    if max == 0 {
        super::TEXT_DIM
    } else if cur == 0 {
        super::BAD
    } else if cur * 4 <= max {
        Color::srgb(0.95, 0.75, 0.25)
    } else {
        super::TEXT
    }
}

/// Top-left corner of the tooltip for a cursor at (`cx`, `cy`): below and to
/// the right, flipped to stay on screen.
pub fn tip_pos(cx: f32, cy: f32, sw: f32, sh: f32) -> (f32, f32) {
    const W: f32 = 290.0;
    const H: f32 = 130.0;
    let x = if cx + 18.0 + W > sw { cx - 12.0 - W } else { cx + 18.0 };
    let y = if cy + 18.0 + H > sh { cy - 12.0 - H } else { cy + 18.0 };
    (x.max(0.0), y.max(0.0))
}

/// Bag grid: how many slots to draw for `stacks` item stacks, in rows of
/// `cols`, never fewer than `min_rows` rows.
pub fn bag_slots(stacks: usize, cols: usize, min_rows: usize) -> usize {
    let rows = stacks.div_ceil(cols.max(1)).max(min_rows);
    rows * cols.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_stay_on_screen() {
        for (sw, sh) in [(1600.0, 900.0), (1280.0, 720.0), (1024.0, 768.0), (800.0, 600.0), (640.0, 480.0), (2560.0, 1440.0)] {
            for w in Win::ALL {
                let r = rect(w, sw, sh);
                assert!(r.x >= 0.0 && r.y >= 0.0, "{w:?} at {sw}x{sh}: {r:?}");
                assert!(r.x + r.w <= sw + 0.5, "{w:?} at {sw}x{sh} overflows right: {r:?}");
                assert!(r.y + r.h <= sh + 0.5, "{w:?} at {sw}x{sh} overflows bottom: {r:?}");
                assert!(r.w >= 120.0 && r.h >= 120.0);
            }
        }
    }

    #[test]
    fn default_screen_keeps_the_hud_clear() {
        // 1600x900 is the windowless shot size and the default window.
        for w in Win::ALL {
            let r = rect(w, 1600.0, 900.0);
            if tall(w) {
                assert!(r.x >= 262.0, "{w:?} covers the unit frame: {r:?}");
            } else {
                assert!(r.y >= 190.0, "{w:?} covers the unit frame: {r:?}");
            }
            assert!(r.y + r.h <= 900.0 - 100.0, "{w:?} covers the action bar: {r:?}");
        }
        // Bags and the social window can be open together.
        let (b, s) = (rect(Win::Bags, 1600.0, 900.0), rect(Win::Social, 1600.0, 900.0));
        assert!(s.x + s.w <= b.x, "social {s:?} overlaps bags {b:?}");
        // Character sheet and quest log side by side.
        let (c, q) = (rect(Win::Character, 1600.0, 900.0), rect(Win::QuestLog, 1600.0, 900.0));
        assert!(c.x + c.w <= q.x);
        // ...and the spellbook beside the quest log, clear of the bags.
        let p = rect(Win::Spellbook, 1600.0, 900.0);
        assert!(q.x + q.w <= p.x && p.x + p.w <= b.x, "{q:?} {p:?} {b:?}");
    }

    #[test]
    fn hex_parsing() {
        assert_eq!(hex_rgb("#ffffff"), Some([1.0, 1.0, 1.0]));
        assert_eq!(hex_rgb("#000000"), Some([0.0, 0.0, 0.0]));
        let [r, g, b] = hex_rgb("#ff8000").unwrap();
        assert!((r - 1.0).abs() < 1e-6 && (g - 128.0 / 255.0).abs() < 1e-6 && b == 0.0);
        assert_eq!(hex_rgb("ff8000"), None);
        assert_eq!(hex_rgb("#ff80"), None);
        assert_eq!(hex_rgb("#gg0000"), None);
    }

    #[test]
    fn every_rarity_has_its_own_colour() {
        let cols: Vec<[u8; 3]> = Rarity::ALL
            .iter()
            .map(|r| {
                let c = rarity_color(*r).to_srgba();
                [(c.red * 255.0).round() as u8, (c.green * 255.0).round() as u8, (c.blue * 255.0).round() as u8]
            })
            .collect();
        for (i, a) in cols.iter().enumerate() {
            for b in &cols[i + 1..] {
                assert_ne!(a, b);
            }
        }
        // Spot checks against the classic palette.
        assert_eq!(cols[0], [0x9d, 0x9d, 0x9d]); // poor: grey
        assert_eq!(cols[2], [0x1e, 0xff, 0x00]); // uncommon: green
        assert_eq!(cols[4], [0xa3, 0x35, 0xee]); // epic: purple
        assert_eq!(cols[5], [0xff, 0x80, 0x00]); // legendary: orange
    }

    #[test]
    fn wear_colours() {
        assert_eq!(wear_color(0, 60), crate::win::BAD);
        assert_eq!(wear_color(60, 60), crate::win::TEXT);
        assert_ne!(wear_color(15, 60), crate::win::TEXT);
        assert_eq!(wear_color(0, 0), crate::win::TEXT_DIM);
    }

    #[test]
    fn tooltip_flips_at_the_edges() {
        assert_eq!(tip_pos(100.0, 100.0, 1600.0, 900.0), (118.0, 118.0));
        let (x, y) = tip_pos(1590.0, 890.0, 1600.0, 900.0);
        assert!(x + 290.0 <= 1600.0 && y + 130.0 <= 900.0);
    }

    #[test]
    fn bag_grid() {
        assert_eq!(bag_slots(0, 6, 4), 24);
        assert_eq!(bag_slots(24, 6, 4), 24);
        assert_eq!(bag_slots(25, 6, 4), 30);
    }
}
