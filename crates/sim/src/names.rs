// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Character-name rules, shared by the builder (live feedback while typing),
//! the server and the solo driver (the authority). Pure and wasm-safe.

/// Shortest and longest name the builder accepts.
pub const NAME_MIN: usize = 3;
pub const NAME_MAX: usize = 16;

/// Names that would let a player pose as staff, the system or a fixed NPC.
const RESERVED: &[&str] = &[
    "admin", "administrator", "moderator", "gamemaster", "server", "system", "developer",
    "support", "antediluvia", "innkeeper", "quartermaster", "elder", "seer", "questgiver",
    "stablemaster", "flightmaster", "banker", "vendor", "player", "unknown", "nobody",
];

/// Check a typed name and return it in its stored form ("eNOCH" -> "Enoch",
/// "bar-jonah" -> "Bar-Jonah"), or the reason it is refused.
pub fn validate_name(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    let n = name.chars().count();
    if n < NAME_MIN {
        return Err("name needs at least 3 letters");
    }
    if n > NAME_MAX {
        return Err("name can be at most 16 letters");
    }
    let chars: Vec<char> = name.chars().collect();
    if !chars.iter().all(|c| c.is_ascii_alphabetic() || *c == '\'' || *c == '-') {
        return Err("letters only (one ' or - allowed inside)");
    }
    let seps = chars.iter().filter(|c| !c.is_ascii_alphabetic()).count();
    if seps > 1 {
        return Err("only one ' or - allowed");
    }
    if !chars[0].is_ascii_alphabetic() || !chars[n - 1].is_ascii_alphabetic() {
        return Err("name must start and end with a letter");
    }
    let lower: Vec<char> = chars.iter().map(|c| c.to_ascii_lowercase()).collect();
    if lower.windows(3).any(|w| w[0] == w[1] && w[1] == w[2]) {
        return Err("no letter three times in a row");
    }
    if !lower.iter().any(|c| "aeiouy".contains(*c)) {
        return Err("name needs a vowel");
    }
    let letters: String = lower.iter().filter(|c| c.is_ascii_alphabetic()).collect();
    if RESERVED.contains(&letters.as_str()) {
        return Err("that name is reserved");
    }
    let mut out = String::with_capacity(n);
    let mut upper = true;
    for c in lower {
        if upper {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push(c);
        }
        upper = !c.is_ascii_alphabetic();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_names_are_normalised() {
        assert_eq!(validate_name("  eNOCH "), Ok("Enoch".to_string()));
        assert_eq!(validate_name("bar-jonah"), Ok("Bar-Jonah".to_string()));
        assert_eq!(validate_name("d'vora"), Ok("D'Vora".to_string()));
        assert_eq!(validate_name("Methuselah"), Ok("Methuselah".to_string()));
    }

    #[test]
    fn bad_names_are_refused_with_a_reason() {
        for bad in [
            "", "ab", "abcdefghijklmnopq", "Enoch7", "En och", "-Enoch", "Enoch'", "a-b-c",
            "Aaaron", "Grmph", "Admin", "ADMIN", "inn-keeper", "Game_Master", "Enoch!",
        ] {
            assert!(validate_name(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn validation_is_idempotent() {
        for n in ["enoch", "bar-jonah", "d'vora", "LAMECH"] {
            let once = validate_name(n).unwrap();
            assert_eq!(validate_name(&once), Ok(once.clone()));
        }
    }
}
