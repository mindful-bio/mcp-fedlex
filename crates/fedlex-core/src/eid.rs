//! eId-Normalisierung für den AKN↔JOLux-Abgleich (X9.4, J18.2).
//!
//! Die Regel ist die Brücke zwischen beiden Schichten und in beiden Lexika
//! eingelockt. Sie lebt deshalb genau einmal hier im Kern — fedlex-jolux und
//! fedlex-akn re-exportieren sie.

/// Normalisiert eine AKN-eId in die JOLux-Subdivision-Schreibweise.
///
/// Regel `_([a-z])($|/)` → `$1$2`: `art_14_a` → `art_14a`, `art_2_b/para_1`
/// → `art_2b/para_1`. Ziffern-Segmente bleiben unverändert.
pub fn normalize_eid(eid: &str) -> String {
    let chars: Vec<char> = eid.chars().collect();
    let mut out = String::with_capacity(eid.len());
    let mut i = 0;
    while i < chars.len() {
        let is_match = chars[i] == '_'
            && i + 1 < chars.len()
            && chars[i + 1].is_ascii_lowercase()
            && (i + 2 == chars.len() || chars[i + 2] == '/');
        if is_match {
            out.push(chars[i + 1]);
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_suffixes_lose_their_underscore() {
        assert_eq!(normalize_eid("art_14_a"), "art_14a");
        assert_eq!(normalize_eid("art_2_b/para_1"), "art_2b/para_1");
        assert_eq!(normalize_eid("chap_5_a"), "chap_5a");
    }

    #[test]
    fn digit_segments_stay_untouched() {
        assert_eq!(normalize_eid("art_14"), "art_14");
        assert_eq!(normalize_eid("annex_1"), "annex_1");
        assert_eq!(normalize_eid("art_1/para_2"), "art_1/para_2");
    }

    #[test]
    fn empty_and_plain_strings_pass_through() {
        assert_eq!(normalize_eid(""), "");
        assert_eq!(normalize_eid("preamble"), "preamble");
    }
}
