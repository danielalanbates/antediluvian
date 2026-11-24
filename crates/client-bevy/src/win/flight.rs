// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Flight-master window (B08): the routes this character has discovered,
//! with fares and a Fly button. Opens when the server answers `FlightList`
//! (F at a flight master, or `/flights`).

use antediluvia_protocol::{Act, ClientMsg, FlightNode};
use bevy::prelude::*;

use super::{button, card, pretty, row, text, Action, Ctx, BAD, GOLD, TEXT, TEXT_DIM};

/// Known destinations, this act first, then by fare. `here` is left out.
pub fn destinations<'a>(nodes: &'a [FlightNode], here: Option<&str>, act: Act) -> Vec<&'a FlightNode> {
    let mut v: Vec<&FlightNode> = nodes.iter().filter(|n| n.known && Some(n.id.as_str()) != here).collect();
    v.sort_by_key(|n| (n.act != act, n.cost));
    v
}

pub(crate) fn build(p: &mut ChildBuilder, ctx: &Ctx) {
    let Some((here, nodes)) = ctx.st.flights.as_ref() else {
        text(p, "No flight master asked yet. Stand at an inn or a major landmark and press F.", 12.0, TEXT);
        return;
    };
    let here_name = here.as_deref().and_then(|h| nodes.iter().find(|n| n.id == h)).map(|n| n.name.as_str());
    row(p, |r| {
        text(
            r,
            match here_name {
                Some(n) => format!("You are at {n}"),
                None => "You are not at a flight master".to_string(),
            },
            12.0,
            TEXT_DIM,
        );
        text(r, format!("{} gold", ctx.sheet.gold), 13.0, GOLD);
    });
    let dest = destinations(nodes, here.as_deref(), ctx.act);
    if dest.is_empty() {
        text(p, "You know no other flight masters. Walk to one to learn its route.", 12.0, TEXT);
    }
    card(p, |c| {
        for n in dest.iter().take(14) {
            row(c, |r| {
                text(r, format!("{} ({})", n.name, pretty(n.act.as_str())), 12.0, TEXT);
                if here.is_none() {
                    text(r, "-", 12.0, TEXT_DIM);
                } else if ctx.sheet.gold < n.cost {
                    text(r, format!("{} gold - too dear", n.cost), 11.0, BAD);
                } else {
                    // Flying moves you, so ask the destination's master for
                    // its routes straight away (fares differ from there).
                    button(
                        r,
                        format!("Fly  {}g", n.cost),
                        Action::Send(vec![ClientMsg::FlightTake { node: n.id.clone() }, ClientMsg::FlightList]),
                        GOLD,
                    );
                }
            });
        }
    });
    let unknown = nodes.iter().filter(|n| !n.known).count();
    if unknown > 0 {
        text(p, format!("{unknown} flight masters not yet discovered (see the map, M)."), 11.0, TEXT_DIM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: &str, act: Act, known: bool, cost: u32) -> FlightNode {
        FlightNode { id: id.into(), name: id.to_uppercase(), act, x: 0.0, y: 0.0, known, cost }
    }

    #[test]
    fn destinations_skip_here_and_unknown_and_sort_local_first() {
        let nodes = vec![
            n("inn_hermon", Act::Hermon, true, 40),
            n("fp_1", Act::Eden, true, 9),
            n("inn_eden", Act::Eden, true, 0),
            n("fp_2", Act::Eden, false, 5),
            n("fp_3", Act::Eden, true, 4),
        ];
        let d: Vec<&str> = destinations(&nodes, Some("inn_eden"), Act::Eden).iter().map(|n| n.id.as_str()).collect();
        assert_eq!(d, ["fp_3", "fp_1", "inn_hermon"]);
    }
}
