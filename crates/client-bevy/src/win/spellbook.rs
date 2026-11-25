// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Spellbook (P): every ability of the character's class in bar order, with
//! its key, unlock level, cost, cooldown, cast time and description. Locked
//! abilities are listed dimmed with the level that unlocks them.

use antediluvia_sim::abilities::{class_blurb, class_resource, kit, Ability};
use bevy::prelude::*;

use super::{card, row, text, Ctx, GOLD, TEXT, TEXT_DIM};

/// Bar key for the n-th kit ability (0-based): 1..9 then 0.
pub fn key_label(i: usize) -> String {
    match i {
        0..=8 => (i + 1).to_string(),
        9 => "0".to_string(),
        _ => "-".to_string(),
    }
}

/// "12 stamina  8s cooldown  1.5s cast" - parts that are zero are left out.
pub fn cost_line(a: &Ability) -> String {
    let mut parts = Vec::new();
    if a.cost > 0 {
        parts.push(format!("{} {}", a.cost, class_resource(a.class).as_str()));
    }
    if a.cooldown > 0.0 {
        parts.push(format!("{:.0}s cooldown", a.cooldown));
    }
    parts.push(if a.cast > 0.0 { format!("{:.1}s cast", a.cast) } else { "instant".to_string() });
    if a.range > 0.0 {
        parts.push(format!("range {:.0}", a.range));
    }
    parts.join("   ")
}

pub(crate) fn build(p: &mut ChildBuilder, ctx: &Ctx) {
    let s = ctx.sheet;
    let Some(class) = s.class else {
        text(p, "No class chosen. Type /class warrior | hunter | priest | mage | wanderer | herald | warden | occultist.", 12.0, TEXT);
        return;
    };
    let (name, pitch) = class_blurb(class);
    text(p, format!("{name} - {pitch}"), 12.0, TEXT_DIM);
    let abilities = kit(class);
    let known = abilities.iter().filter(|a| s.level >= a.min_level).count();
    text(p, format!("{known} of {} abilities learned (level {})", abilities.len(), s.level), 12.0, TEXT_DIM);
    for (i, a) in abilities.iter().enumerate() {
        let unlocked = s.level >= a.min_level;
        card(p, |c| {
            row(c, |r| {
                text(r, format!("[{}] {}", key_label(i), a.name), 13.0, if unlocked { GOLD } else { TEXT_DIM });
                text(
                    r,
                    if unlocked { format!("Level {}", a.min_level) } else { format!("Unlocks at level {}", a.min_level) },
                    11.0,
                    TEXT_DIM,
                );
            });
            text(c, cost_line(a), 11.0, if unlocked { TEXT } else { TEXT_DIM });
            text(c, a.desc, 11.0, TEXT_DIM);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antediluvia_protocol::Class;
    use antediluvia_sim::abilities::BAR_SLOTS;

    #[test]
    fn keys_follow_the_bar() {
        assert_eq!(key_label(0), "1");
        assert_eq!(key_label(8), "9");
        assert_eq!(key_label(9), "0");
        assert_eq!(key_label(BAR_SLOTS), "-");
    }

    #[test]
    fn every_class_lists_its_whole_kit_in_unlock_order() {
        for class in Class::ALL {
            let k = kit(class);
            assert!(!k.is_empty() && k.len() <= BAR_SLOTS, "{class:?}");
            assert!(k.windows(2).all(|w| w[0].min_level <= w[1].min_level));
            for a in k {
                let line = cost_line(a);
                assert!(line.contains("cast") || line.contains("instant"), "{line}");
                assert!(!a.desc.is_empty());
            }
        }
    }
}
