use crate::chinese::is_chinese_character;
use pinyin::ToPinyin;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::error::LailaisayError;

/// Whole-glossary homophone correction (tone-stripped pinyin keys).
///
/// Phonetic glossary. Pinyin comes from the `pinyin` crate
/// instead of macOS `CFStringTransform`.
#[derive(Debug, Clone, Default)]
pub struct PhoneticGlossary {
    terms_by_key: HashMap<String, String>,
    lengths: Vec<usize>,
}

impl PhoneticGlossary {
    pub fn new(terms: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let mut index: HashMap<String, String> = HashMap::new();
        let mut ambiguous: HashSet<String> = HashSet::new();
        let mut length_set: HashSet<usize> = HashSet::new();

        for term in terms {
            let term = term.as_ref();
            let chars: Vec<char> = term.chars().collect();
            if chars.len() < 2 || !chars.iter().copied().all(is_chinese_character) {
                continue;
            }
            let key: String = chars.iter().copied().map(pinyin_key).collect();
            if key.is_empty() {
                continue;
            }
            if let Some(existing) = index.get(&key) {
                if existing != term {
                    ambiguous.insert(key);
                    continue;
                }
            }
            index.insert(key, term.to_string());
            length_set.insert(chars.len());
        }
        for key in ambiguous {
            index.remove(&key);
        }
        let mut lengths: Vec<usize> = length_set.into_iter().collect();
        lengths.sort_by(|a, b| b.cmp(a));
        Self {
            terms_by_key: index,
            lengths,
        }
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, LailaisayError> {
        let value: Value = serde_json::from_slice(bytes)?;
        let terms = match value {
            Value::Array(items) => items
                .into_iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        Ok(Self::new(terms))
    }

    pub fn load_path(path: &Path) -> Result<Self, LailaisayError> {
        let data = std::fs::read(path)?;
        Self::from_json_bytes(&data)
    }

    pub fn is_empty(&self) -> bool {
        self.terms_by_key.is_empty()
    }

    pub fn correct(&self, text: &str) -> String {
        if self.terms_by_key.is_empty() || text.is_empty() {
            return text.to_string();
        }
        let mut chars: Vec<char> = text.chars().collect();
        let keys: Vec<String> = chars
            .iter()
            .map(|c| {
                if is_chinese_character(*c) {
                    pinyin_key(*c)
                } else {
                    String::new()
                }
            })
            .collect();

        let mut i = 0;
        while i < chars.len() {
            if keys[i].is_empty() {
                i += 1;
                continue;
            }
            let mut advance = 1;
            for &len in &self.lengths {
                if i + len > chars.len() {
                    continue;
                }
                let window = &keys[i..i + len];
                if window.iter().any(|k| k.is_empty()) {
                    continue;
                }
                let joined = window.concat();
                let Some(term) = self.terms_by_key.get(&joined) else {
                    continue;
                };
                let term_chars: Vec<char> = term.chars().collect();
                if chars[i..i + len] != term_chars {
                    chars.splice(i..i + len, term_chars);
                }
                advance = len;
                break;
            }
            i += advance;
        }
        chars.into_iter().collect()
    }
}

fn pinyin_key(ch: char) -> String {
    ch.to_pinyin()
        .map(|p| p.plain().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homophone_variant_is_corrected() {
        let glossary = PhoneticGlossary::new(["當責", "燈塔學校"]);
        assert_eq!(glossary.correct("我們強調當則的文化"), "我們強調當責的文化");
        assert_eq!(glossary.correct("登塔學校很棒"), "燈塔學校很棒");
    }

    #[test]
    fn correct_or_unrelated_is_untouched() {
        let glossary = PhoneticGlossary::new(["當責", "燈塔學校"]);
        assert_eq!(glossary.correct("當責文化很重要"), "當責文化很重要");
        assert_eq!(glossary.correct("今天天氣很好"), "今天天氣很好");
    }

    #[test]
    fn mixed_non_cjk_is_safe() {
        let glossary = PhoneticGlossary::new(["當責"]);
        assert_eq!(glossary.correct("7個習慣的當則精神"), "7個習慣的當責精神");
        assert_eq!(glossary.correct("OK，沒問題。"), "OK，沒問題。");
    }

    #[test]
    fn longer_term_wins() {
        let glossary = PhoneticGlossary::new(["燈塔", "燈塔學校"]);
        assert_eq!(glossary.correct("登塔學校在山上"), "燈塔學校在山上");
    }
}
