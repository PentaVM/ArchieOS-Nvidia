//! `String.prototype.localeCompare` as the neural sphere's layout uses it to break ties: ICU collation in the
//! locale Lumenfield ran under, Spanish (CLDR `es`: the root order with `ñ` a letter of its own between `n` and
//! `o`), not code points. Three levels, each over the whole string: the base character (punctuation, then
//! symbols, digits and letters, case and accents ignored), then accents, then case (lower before upper). It
//! covers what file names hold; any other character sorts after the letters by code point.
//! Checked against node's own order in `fixtures/neural.golden.json` (`collation`).

use std::cmp::Ordering;

/// Punctuation and symbols in CLDR root order (space first, `$` last before the digits).
const PUNCTUATION: &str = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";

/// (primary, secondary, tertiary) weights of one character.
fn weights(c: char) -> (u32, u8, u8) {
    if let Some(i) = PUNCTUATION.find(c) {
        return (1 + i as u32, 0, 0);
    }
    if c.is_ascii_digit() {
        return (100 + (c as u32 - '0' as u32), 0, 0);
    }
    let (base, accent) = match c {
        'á' | 'Á' => ('a', 1),
        'à' | 'À' => ('a', 2),
        'â' | 'Â' => ('a', 3),
        'ä' | 'Ä' => ('a', 4),
        'ã' | 'Ã' => ('a', 5),
        'å' | 'Å' => ('a', 6),
        'é' | 'É' => ('e', 1),
        'è' | 'È' => ('e', 2),
        'ê' | 'Ê' => ('e', 3),
        'ë' | 'Ë' => ('e', 4),
        'í' | 'Í' => ('i', 1),
        'ì' | 'Ì' => ('i', 2),
        'î' | 'Î' => ('i', 3),
        'ï' | 'Ï' => ('i', 4),
        'ó' | 'Ó' => ('o', 1),
        'ò' | 'Ò' => ('o', 2),
        'ô' | 'Ô' => ('o', 3),
        'ö' | 'Ö' => ('o', 4),
        'õ' | 'Õ' => ('o', 5),
        'ú' | 'Ú' => ('u', 1),
        'ù' | 'Ù' => ('u', 2),
        'û' | 'Û' => ('u', 3),
        'ü' | 'Ü' => ('u', 4),
        // Spanish: ñ is its own letter, right after n.
        'ñ' | 'Ñ' => {
            return (
                200 + 2 * ('n' as u32 - 'a' as u32) + 1,
                0,
                u8::from(c == 'Ñ'),
            )
        }
        'ç' | 'Ç' => ('c', 7),
        _ => (c, 0),
    };
    let lower = base.to_ascii_lowercase();
    if lower.is_ascii_lowercase() {
        let upper = u8::from(c.is_uppercase());
        return (200 + 2 * (lower as u32 - 'a' as u32), accent, upper);
    }
    (1000 + c as u32, 0, u8::from(c.is_uppercase()))
}

fn level<F: Fn((u32, u8, u8)) -> u32>(a: &str, b: &str, pick: F) -> Ordering {
    a.chars()
        .map(|c| pick(weights(c)))
        .cmp(b.chars().map(|c| pick(weights(c))))
}

/// `a.localeCompare(b)` under ICU root collation.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    level(a, b, |w| w.0)
        .then_with(|| level(a, b, |w| u32::from(w.1)))
        .then_with(|| level(a, b, |w| u32::from(w.2)))
}
