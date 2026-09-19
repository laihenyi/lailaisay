//! Gate auto-learned dictionary entries: only phonetic mishearings, not rewrites.
//!
//! 借鑑 typefree 開源 MishearingCheck 哲學（kdsz001/typefree MishearingCheck.swift）：
//! 讀音相近才學（玉米→域名）；改內容／選意思（週四→週三、他→她）不學。
//! lailaisay-owned rewrite using the `pinyin` crate — not a GPL Swift port.

use crate::chinese::is_chinese_character;

/// Max Latin/pinyin edit-distance ÷ longer length (typefree `maxLatinDifference`).
const MAX_LATIN_DIFFERENCE: f64 = 0.5;

/// Meaning-bearing homophones: swapping them is choosing sense, not fixing ASR.
const MEANINGFUL_HOMOPHONES: &[&[char]] = &[
    &['他', '她', '它', '牠'],
    &['的', '得', '地'],
    &['在', '再'],
    &['那', '哪'],
    &['做', '作'],
    &['已', '以'],
    &['像', '象'],
    &['須', '需'],
    &['即', '既'],
];

const SIMILAR_INITIALS: &[&[&str]] = &[
    &["zh", "z"],
    &["ch", "c"],
    &["sh", "s"],
    &["n", "l"],
    &["f", "h"],
    &["r", "l"],
];

const INITIALS: &[&str] = &[
    "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x", "r",
    "z", "c", "s", "y", "w",
];

/// True when `old` → `new` looks like a speech-recognition mishearing.
pub fn is_likely_mishearing(old: &str, new: &str) -> bool {
    let old = old.trim();
    let new = new.trim();
    if old.is_empty() || new.is_empty() || old == new {
        return false;
    }
    if digits_key(old) != digits_key(new) {
        return false;
    }

    let a = units_of(old);
    let b = units_of(new);
    if a.is_empty() || b.is_empty() || a == b {
        return false;
    }

    if a.iter().all(|u| u.is_han) && b.iter().all(|u| u.is_han) {
        // Pure CJK: same length, each changed syllable sounds alike.
        // A single-character global replace is too dangerous to learn.
        if a.len() != b.len() || a.len() < 2 {
            return false;
        }
        let mut changed = false;
        for (x, y) in a.iter().zip(b.iter()) {
            if x.ch == y.ch {
                continue;
            }
            changed = true;
            if is_meaningful_homophone(x.ch, y.ch) {
                return false;
            }
            if !syllables_sound_alike(&x.sound, &y.sound) {
                return false;
            }
        }
        return changed;
    }

    let s: String = a.iter().map(|u| u.sound.as_str()).collect();
    let t: String = b.iter().map(|u| u.sound.as_str()).collect();
    if s == t || s.is_empty() || t.is_empty() {
        return false;
    }
    let distance = edit_distance(&s, &t) as f64;
    let longer = s.chars().count().max(t.chars().count()) as f64;
    distance / longer <= MAX_LATIN_DIFFERENCE
}

fn digits_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_digit() || ('０'..='９').contains(c))
        .map(|c| {
            if ('０'..='９').contains(&c) {
                char::from(b'0' + (c as u32 - '０' as u32) as u8)
            } else {
                c
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Unit {
    ch: char,
    sound: String,
    is_han: bool,
}

fn units_of(text: &str) -> Vec<Unit> {
    let mut result = Vec::new();
    let mut latin = String::new();
    let flush = |latin: &mut String, result: &mut Vec<Unit>| {
        if let Some(first) = latin.chars().next() {
            result.push(Unit {
                ch: first,
                sound: latin.to_ascii_lowercase(),
                is_han: false,
            });
        }
        latin.clear();
    };
    for ch in text.chars() {
        if is_chinese_character(ch) {
            flush(&mut latin, &mut result);
            result.push(Unit {
                ch,
                sound: pinyin_key(ch),
                is_han: true,
            });
        } else if ch.is_ascii_alphanumeric() {
            latin.push(ch);
        } else {
            flush(&mut latin, &mut result);
        }
    }
    flush(&mut latin, &mut result);
    result
}

fn pinyin_key(ch: char) -> String {
    use pinyin::ToPinyin;
    ch.to_pinyin()
        .map(|p| p.plain().to_string())
        .unwrap_or_default()
}

fn is_meaningful_homophone(a: char, b: char) -> bool {
    MEANINGFUL_HOMOPHONES
        .iter()
        .any(|set| set.contains(&a) && set.contains(&b))
}

fn syllables_sound_alike(x: &str, y: &str) -> bool {
    if x == y || x.is_empty() || y.is_empty() {
        return x == y;
    }
    let (ix, fx) = split_syllable(x);
    let (iy, fy) = split_syllable(y);
    let initials_ok = ix == iy
        || SIMILAR_INITIALS
            .iter()
            .any(|g| g.contains(&ix) && g.contains(&iy));
    initials_ok && final_core(fx) == final_core(fy)
}

fn split_syllable(syllable: &str) -> (&str, &str) {
    for initial in INITIALS {
        if syllable.starts_with(initial) && syllable.len() > initial.len() {
            return (initial, &syllable[initial.len()..]);
        }
    }
    ("", syllable)
}

fn final_core(final_: &str) -> &str {
    if let Some(rest) = final_.strip_suffix("ng") {
        rest
    } else if let Some(rest) = final_.strip_suffix('n') {
        rest
    } else {
        final_
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = if a[i - 1] == b[j - 1] {
                prev[j - 1]
            } else {
                1 + prev[j - 1].min(prev[j]).min(cur[j - 1])
            };
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_phonetic_near_misses() {
        assert!(is_likely_mishearing("當則", "當責"));
        assert!(is_likely_mishearing("登塔學校", "燈塔學校"));
        assert!(is_likely_mishearing("玉米", "域名"));
        assert!(is_likely_mishearing("OpenWrt", "OpenWiki"));
    }

    #[test]
    fn rejects_content_rewrites_and_sense_swaps() {
        assert!(!is_likely_mishearing("週四", "週三"));
        assert!(!is_likely_mishearing("周四", "周三"));
        assert!(!is_likely_mishearing("共5", "共4"));
        assert!(!is_likely_mishearing("共五", "共四"));
        assert!(!is_likely_mishearing("他", "她"));
        assert!(!is_likely_mishearing("他們", "她們"));
        assert!(!is_likely_mishearing("測試一下", "要求後續變更"));
        assert!(!is_likely_mishearing("台北", "台南"));
        assert!(!is_likely_mishearing("是不是", "是否"));
        assert!(!is_likely_mishearing("same", "same"));
        assert!(!is_likely_mishearing("", "當責"));
    }
}
