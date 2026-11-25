//! Antediluvia game simulation — the authoritative world model, shared by the
//! native server and the in-browser (wasm) single-player client.
pub mod abilities;
pub mod aoi;
pub mod caves;
pub mod economy;
pub mod mobs;
pub mod names;
pub mod pois;
pub mod progression;
pub mod questbits;
pub mod battleground;
pub mod quests;
pub mod world;
// ── beta-server-track (B05-B08) ──
pub mod durability;
pub mod questgen;
pub mod social;
pub mod travel;
#[cfg(test)]
mod pvp_tests;
#[cfg(test)]
mod snapshot_size_tests;

/// "sethite" -> "Sethite" (banner text helper shared by the track modules).
pub(crate) fn questgen_title(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
