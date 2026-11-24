// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Loot window. The server puts drops straight into the bags and announces
//! each one, so this window is a record, not a pick list: it pops up when
//! something drops, lists the latest items in their rarity colour and closes
//! itself a few seconds later (or stays if opened by hand).

use antediluvia_sim::durability;
use bevy::prelude::*;

use super::{layout, pretty, text, Ctx, Win, WinState, TEXT_DIM};

/// Drops remembered.
pub const KEEP: usize = 8;
/// Seconds an auto-opened loot window stays up after the last drop.
pub const LINGER: f32 = 6.0;

/// The item id in a "You collected X." notice.
pub fn parse_loot(notice: &str) -> Option<&str> {
    let item = notice.strip_prefix("You collected ")?.strip_suffix('.')?;
    (!item.is_empty() && !item.contains(' ')).then_some(item)
}

pub(crate) fn build(p: &mut ChildBuilder, ctx: &Ctx) {
    if ctx.st.loot.is_empty() {
        text(p, "Nothing looted yet. Drops go straight to your bags (B).", 12.0, TEXT_DIM);
        return;
    }
    for item in ctx.st.loot.iter().rev() {
        let r = durability::rarity_of(item);
        text(p, format!("{}  ({})", pretty(item), r.as_str()), 13.0, layout::rarity_color(r));
    }
    text(p, "Already in your bags.", 11.0, TEXT_DIM);
}

/// Pop the window on a drop and retire it after [`LINGER`] seconds.
pub(crate) fn auto_close(time: Res<Time>, mut st: ResMut<WinState>) {
    if st.loot_fresh {
        st.loot_fresh = false;
        // A window the player opened by hand (timer at zero while open)
        // is theirs to close.
        if !st.is_open(Win::Loot) || st.loot_left > 0.0 {
            st.loot_left = LINGER;
            if !st.is_open(Win::Loot) {
                st.open.insert(0, Win::Loot); // underneath: never steals Escape order
                st.dirty = true;
            }
        }
        return;
    }
    if st.loot_left > 0.0 {
        let left = st.loot_left - time.delta_secs().min(0.1);
        st.loot_left = left.max(0.0);
        if left <= 0.0 {
            st.close(Win::Loot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loot_notices_parse() {
        assert_eq!(parse_loot("You collected wolf_pelt."), Some("wolf_pelt"));
        assert_eq!(parse_loot("You collected mount:dire_wolf."), Some("mount:dire_wolf"));
        assert_eq!(parse_loot("You collected ."), None);
        assert_eq!(parse_loot("You were slain."), None);
        assert_eq!(parse_loot("You collected a lot of things."), None);
    }
}
