// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Social window (O): friends, ignore list, who is online, raid roster and
//! chat channels, one tab each (B07). Names are typed in chat
//! (`/friend NAME`, `/ignore NAME`, `/rinvite NAME`); everything that needs
//! no typing has a button.

use antediluvia_protocol::ClientMsg;
use bevy::prelude::*;

use super::{button, card, pretty, row, text, Action, Blocks, Ctx, SocialTab, BORDER, BTN, GOLD, GOOD, TEXT, TEXT_DIM};

const TABS: [(SocialTab, &str); 5] = [
    (SocialTab::Friends, "Friends"),
    (SocialTab::Ignore, "Ignore"),
    (SocialTab::Who, "Who"),
    (SocialTab::Raid, "Raid"),
    (SocialTab::Channels, "Channels"),
];

/// Rows a list tab shows before it says "and N more".
pub const ROWS: usize = 12;

/// Header count for a tab, e.g. "Friends (2/5 online)".
pub fn tab_summary(tab: SocialTab, st: &super::WinState) -> String {
    match tab {
        SocialTab::Friends => {
            format!("{} of {} friends online", st.friends.iter().filter(|f| f.online).count(), st.friends.len())
        }
        SocialTab::Ignore => format!("Ignoring {}", st.ignored.len()),
        SocialTab::Who => format!("{} online ({} shown)", st.who_total, st.who.len().min(ROWS)),
        SocialTab::Raid if st.raid.is_empty() => "Not in a raid".to_string(),
        SocialTab::Raid => format!("Raid of {} - leader {}", st.raid.len(), st.raid_leader),
        SocialTab::Channels => format!("In {} of {} channels", st.channels_joined.len(), st.channels_available.len()),
    }
}

fn send(m: ClientMsg) -> Action {
    Action::Send(vec![m])
}

pub(crate) fn build(p: &mut ChildBuilder, ctx: &Ctx) {
    let st = ctx.st;
    let me = ctx.sheet.name.as_str();
    // Tab strip.
    p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(4.0), ..default() }).with_children(|tabs| {
        for (tab, label) in TABS {
            let active = st.social_tab == tab;
            tabs.spawn((
                Button,
                Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)), border: UiRect::all(Val::Px(1.0)), ..default() },
                BackgroundColor(if active { Color::srgb(0.30, 0.26, 0.14) } else { BTN }),
                BorderColor(BORDER),
                super::WinAction(Action::SocialTab(tab)),
                Blocks,
            ))
            .with_children(|b| {
                b.spawn((Text::new(label), TextFont { font_size: 12.0, ..default() }, TextColor(if active { GOLD } else { TEXT_DIM })));
            });
        }
    });
    text(p, tab_summary(st.social_tab, st), 12.0, TEXT_DIM);
    match st.social_tab {
        SocialTab::Friends => {
            card(p, |c| {
                if st.friends.is_empty() {
                    text(c, "No friends yet.", 12.0, TEXT_DIM);
                }
                for f in st.friends.iter().take(ROWS) {
                    row(c, |r| {
                        let place = match (f.online, f.act) {
                            (true, Some(a)) => format!("level {} - {}", f.level, pretty(a.as_str())),
                            (true, None) => format!("level {}", f.level),
                            _ => "offline".to_string(),
                        };
                        text(r, format!("{}  ({place})", f.name), 12.0, if f.online { GOOD } else { TEXT_DIM });
                        button(r, "Remove", send(ClientMsg::FriendRemove { player: f.name.clone() }), TEXT_DIM);
                    });
                }
            });
            row(p, |r| {
                text(r, "Add: /friend NAME   Whisper: /w NAME text", 11.0, TEXT_DIM);
                button(r, "Refresh", send(ClientMsg::FriendList), TEXT);
            });
        }
        SocialTab::Ignore => {
            card(p, |c| {
                if st.ignored.is_empty() {
                    text(c, "You are ignoring no one.", 12.0, TEXT_DIM);
                }
                for name in st.ignored.iter().take(ROWS) {
                    row(c, |r| {
                        text(r, name.clone(), 12.0, TEXT);
                        button(r, "Stop ignoring", send(ClientMsg::IgnoreRemove { player: name.clone() }), TEXT_DIM);
                    });
                }
            });
            text(p, "Add: /ignore NAME (their chat and whispers are hidden).", 11.0, TEXT_DIM);
        }
        SocialTab::Who => {
            card(p, |c| {
                if st.who.is_empty() {
                    text(c, "Nobody listed. Press Refresh.", 12.0, TEXT_DIM);
                }
                for w in st.who.iter().take(ROWS) {
                    row(c, |r| {
                        let class = w.class.as_deref().map(pretty).unwrap_or_else(|| "Classless".into());
                        let guild = w.guild.as_deref().map(|g| format!(" <{g}>")).unwrap_or_default();
                        text(r, format!("{}{guild} - {} {class}, {}", w.name, w.level, pretty(w.act.as_str())), 12.0, TEXT);
                        if w.name != me {
                            r.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(4.0), ..default() })
                                .with_children(|b| {
                                    button(b, "Friend", send(ClientMsg::FriendAdd { player: w.name.clone() }), TEXT);
                                    button(b, "Party", send(ClientMsg::PartyInvite { player: w.name.clone() }), TEXT);
                                });
                        }
                    });
                }
                if st.who.len() > ROWS {
                    text(c, format!("...and {} more. Narrow it: /who TEXT", st.who.len() - ROWS), 11.0, TEXT_DIM);
                }
            });
            row(p, |r| {
                text(r, "Filter: /who name, class, land or guild", 11.0, TEXT_DIM);
                button(r, "Refresh", send(ClientMsg::Who { filter: None }), TEXT);
            });
        }
        SocialTab::Raid => {
            let leader = st.raid_leader == me;
            card(p, |c| {
                if st.raid.is_empty() {
                    text(c, "Form a raid to gather up to 40. Raid chat: /ra text", 12.0, TEXT_DIM);
                }
                for name in st.raid.iter().take(20) {
                    row(c, |r| {
                        let tag = if *name == st.raid_leader { " (leader)" } else { "" };
                        text(r, format!("{name}{tag}"), 12.0, if *name == st.raid_leader { GOLD } else { TEXT });
                        if leader && name != me {
                            button(r, "Kick", send(ClientMsg::RaidKick { player: name.clone() }), TEXT_DIM);
                        }
                    });
                }
                if st.raid.len() > 20 {
                    text(c, format!("...and {} more", st.raid.len() - 20), 11.0, TEXT_DIM);
                }
            });
            row(p, |r| {
                text(r, "Invite: /rinvite NAME   Join: /raccept", 11.0, TEXT_DIM);
                if st.raid.is_empty() {
                    button(r, "Form raid", send(ClientMsg::RaidCreate), GOLD);
                } else {
                    button(r, "Leave raid", send(ClientMsg::RaidLeave), TEXT);
                }
            });
        }
        SocialTab::Channels => {
            card(p, |c| {
                if st.channels_available.is_empty() {
                    text(c, "No channel list yet. Press Refresh.", 12.0, TEXT_DIM);
                }
                for ch in &st.channels_available {
                    let joined = st.channels_joined.contains(ch);
                    row(c, |r| {
                        text(r, format!("/{ch}  - {}", if joined { "joined" } else { "not joined" }), 12.0, if joined { GOOD } else { TEXT_DIM });
                        if joined {
                            button(r, "Leave", send(ClientMsg::ChannelLeave { channel: ch.clone() }), TEXT_DIM);
                        } else {
                            button(r, "Join", send(ClientMsg::ChannelJoin { channel: ch.clone() }), TEXT);
                        }
                    });
                }
            });
            row(p, |r| {
                text(r, "Speak: /general text, or pick a chat tab.", 11.0, TEXT_DIM);
                button(r, "Refresh", send(ClientMsg::ChannelList), TEXT);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win::WinState;
    use antediluvia_protocol::FriendEntry;

    #[test]
    fn summaries_count_what_the_tab_lists() {
        let mut st = WinState::default();
        st.friends = vec![
            FriendEntry { name: "Abel".into(), online: true, act: None, level: 4 },
            FriendEntry { name: "Seth".into(), online: false, act: None, level: 0 },
        ];
        assert_eq!(tab_summary(SocialTab::Friends, &st), "1 of 2 friends online");
        assert_eq!(tab_summary(SocialTab::Raid, &st), "Not in a raid");
        st.raid = vec!["Abel".into(), "Seth".into(), "Enosh".into()];
        st.raid_leader = "Seth".into();
        assert_eq!(tab_summary(SocialTab::Raid, &st), "Raid of 3 - leader Seth");
        st.channels_available = vec!["general".into(), "trade".into(), "lfg".into(), "zone".into()];
        st.channels_joined = vec!["general".into()];
        assert_eq!(tab_summary(SocialTab::Channels, &st), "In 1 of 4 channels");
    }
}
