// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Chat tabs: one per channel above the chat log. A tab filters the log to
//! its lines and, for the channel tabs, sends plain typed text to that
//! channel (slash commands still work from any tab).

use std::collections::VecDeque;

use bevy::prelude::*;

use super::{Action, Blocks, WinAction, WinState, BORDER, BTN, GOLD, TEXT_DIM};
use crate::Session;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ChatTab {
    #[default]
    All,
    Say,
    General,
    Trade,
    Lfg,
    Zone,
    /// Party, raid and guild.
    Group,
    Whisper,
    /// System lines: notices, loot, combat, dev.
    Log,
}

impl ChatTab {
    pub const ALL: [ChatTab; 9] = [
        ChatTab::All,
        ChatTab::Say,
        ChatTab::General,
        ChatTab::Trade,
        ChatTab::Lfg,
        ChatTab::Zone,
        ChatTab::Group,
        ChatTab::Whisper,
        ChatTab::Log,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ChatTab::All => "All",
            ChatTab::Say => "Say",
            ChatTab::General => "General",
            ChatTab::Trade => "Trade",
            ChatTab::Lfg => "LFG",
            ChatTab::Zone => "Zone",
            ChatTab::Group => "Group",
            ChatTab::Whisper => "Whisper",
            ChatTab::Log => "Log",
        }
    }

    pub fn from_name(s: &str) -> Option<ChatTab> {
        let s = s.trim().to_lowercase();
        ChatTab::ALL.into_iter().find(|t| t.label().to_lowercase() == s)
    }

    /// The server channel a tab speaks on, if it is a channel tab.
    pub fn channel(self) -> Option<&'static str> {
        match self {
            ChatTab::General => Some("general"),
            ChatTab::Trade => Some("trade"),
            ChatTab::Lfg => Some("lfg"),
            ChatTab::Zone => Some("zone"),
            _ => None,
        }
    }
}

/// Which tab a logged line belongs to (every line also shows under All).
pub fn classify(line: &str) -> ChatTab {
    for (prefix, tab) in [
        ("[general] ", ChatTab::General),
        ("[trade] ", ChatTab::Trade),
        ("[lfg] ", ChatTab::Lfg),
        ("[zone] ", ChatTab::Zone),
        ("[raid] ", ChatTab::Group),
        ("[party] ", ChatTab::Group),
        ("[guild] ", ChatTab::Group),
    ] {
        if line.starts_with(prefix) {
            return tab;
        }
    }
    if line.starts_with("Party") || line.starts_with("Raid") || line.starts_with('<') {
        return ChatTab::Group;
    }
    if line.contains(" whispers: ") || (line.starts_with("To ") && line.contains(": ")) {
        return ChatTab::Whisper;
    }
    // "Name: text" with a single-word name is someone speaking; headings the
    // client prints itself ("Friends: ...") are not.
    if let Some((who, _)) = line.split_once(": ") {
        const HEADINGS: [&str; 8] = ["Friends", "Ignoring", "Channels", "Discovered", "Welcome", "Flight", "Notice", "Repair"];
        let name_like = !who.is_empty()
            && who.len() <= 24
            && who.chars().all(|c| c.is_alphanumeric() || c == '\'' || c == '-')
            && !HEADINGS.contains(&who);
        if name_like {
            return ChatTab::Say;
        }
    }
    ChatTab::Log
}

/// The last `n` lines of the log that the tab shows.
pub fn visible<'a>(log: &'a VecDeque<String>, tab: ChatTab, n: usize) -> Vec<&'a str> {
    let mut out: Vec<&str> = log
        .iter()
        .rev()
        .filter(|l| tab == ChatTab::All || classify(l) == tab)
        .take(n)
        .map(String::as_str)
        .collect();
    out.reverse();
    out
}

/// Typed text as it should be dispatched from a tab: plain text on a channel
/// tab goes to that channel; commands and the other tabs are left alone.
pub fn route(text: &str, tab: ChatTab) -> String {
    match tab.channel() {
        Some(ch) if !text.trim_start().starts_with('/') && !text.trim().is_empty() => format!("/{ch} {}", text.trim()),
        _ => text.to_string(),
    }
}

#[derive(Component)]
pub(crate) struct ChatTabBtn(ChatTab);

pub(crate) fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                // Just above the chat panel (log up to 172 px + input bar).
                bottom: Val::Px(214.0),
                left: Val::Px(14.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(2.0),
                ..default()
            },
            Visibility::Hidden,
            ChatTabStrip,
        ))
        .with_children(|strip| {
            for tab in ChatTab::ALL {
                strip
                    .spawn((
                        Button,
                        Node { padding: UiRect::axes(Val::Px(5.0), Val::Px(2.0)), border: UiRect::all(Val::Px(1.0)), ..default() },
                        BackgroundColor(BTN),
                        BorderColor(BORDER),
                        WinAction(Action::ChatTab(tab)),
                        Blocks,
                        ChatTabBtn(tab),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(tab.label()),
                            TextFont { font_size: 10.0, ..default() },
                            TextColor(TEXT_DIM),
                            ChatTabText(tab),
                        ));
                    });
            }
        });
}

#[derive(Component)]
pub(crate) struct ChatTabStrip;
#[derive(Component)]
pub(crate) struct ChatTabText(ChatTab);

#[allow(clippy::type_complexity)]
pub(crate) fn update(
    session: Res<Session>,
    st: Res<WinState>,
    mut strip: Query<&mut Visibility, With<ChatTabStrip>>,
    mut btns: Query<(&ChatTabBtn, &mut BackgroundColor), Without<ChatTabStrip>>,
    mut labels: Query<(&ChatTabText, &mut TextColor)>,
) {
    if let Ok(mut vis) = strip.get_single_mut() {
        let want = if session.sheet.is_some() && session.builder.is_none() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
    for (btn, mut bg) in &mut btns {
        let c = if btn.0 == st.chat_tab { Color::srgb(0.30, 0.26, 0.14) } else { BTN };
        if bg.0 != c {
            bg.0 = c;
        }
    }
    for (label, mut color) in &mut labels {
        let c = if label.0 == st.chat_tab { GOLD } else { TEXT_DIM };
        if color.0 != c {
            color.0 = c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_land_on_their_tabs() {
        assert_eq!(classify("[general] Abel: hello"), ChatTab::General);
        assert_eq!(classify("[trade] Cain: selling ore"), ChatTab::Trade);
        assert_eq!(classify("[lfg] Seth: healer lf group"), ChatTab::Lfg);
        assert_eq!(classify("[zone] Enosh: where is the cave"), ChatTab::Zone);
        assert_eq!(classify("[raid] Seth: pull in 5"), ChatTab::Group);
        assert_eq!(classify("Party: Abel, Seth"), ChatTab::Group);
        assert_eq!(classify("Raid (3), leader Seth: Abel, Seth, Enosh"), ChatTab::Group);
        assert_eq!(classify("Abel whispers: psst"), ChatTab::Whisper);
        assert_eq!(classify("To Abel: hi"), ChatTab::Whisper);
        assert_eq!(classify("Abel: hello there"), ChatTab::Say);
        assert_eq!(classify("Innkeeper: I stock bread 2g."), ChatTab::Say);
        assert_eq!(classify("You collected wolf_pelt."), ChatTab::Log);
        assert_eq!(classify("Friends: none"), ChatTab::Log);
        assert_eq!(classify("Login rejected: bad version"), ChatTab::Log);
        assert_eq!(classify("[dev] time set to 0.30"), ChatTab::Log);
    }

    #[test]
    fn tabs_filter_the_log_and_keep_order() {
        let log: VecDeque<String> = ["Abel: one", "[trade] Cain: ore", "You collected x.", "Abel: two", "[trade] Cain: more"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(visible(&log, ChatTab::All, 8).len(), 5);
        assert_eq!(visible(&log, ChatTab::Say, 8), ["Abel: one", "Abel: two"]);
        assert_eq!(visible(&log, ChatTab::Trade, 1), ["[trade] Cain: more"]);
        assert!(visible(&log, ChatTab::Whisper, 8).is_empty());
        assert_eq!(visible(&log, ChatTab::All, 2), ["Abel: two", "[trade] Cain: more"]);
    }

    #[test]
    fn channel_tabs_route_plain_text_only() {
        assert_eq!(route("hello all", ChatTab::General), "/general hello all");
        assert_eq!(route("/who", ChatTab::General), "/who");
        assert_eq!(route("hello", ChatTab::Say), "hello");
        assert_eq!(route("hello", ChatTab::All), "hello");
        assert_eq!(route("wts ore", ChatTab::Trade), "/trade wts ore");
    }

    #[test]
    fn tab_names_round_trip() {
        for t in ChatTab::ALL {
            assert_eq!(ChatTab::from_name(t.label()), Some(t));
        }
        assert_eq!(ChatTab::from_name("nope"), None);
    }
}
