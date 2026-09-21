use crate::chinese::is_cjk_script;
use regex::{escape, RegexBuilder};
use serde::{Deserialize, Deserializer, Serialize};
use std::path::Path;
use uuid::Uuid;

use crate::error::LailaisayError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum EntrySource {
    #[default]
    Manual,
    Imported,
    NaturalInput,
    Learned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum EntryType {
    Prompt,
    #[default]
    Replacement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomWordEntry {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    pub original: String,
    pub replacement: String,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub match_whole_word: bool,
    #[serde(default)]
    pub source: EntrySource,
    #[serde(default)]
    pub entry_type: EntryType,
}

fn default_true() -> bool {
    true
}

impl CustomWordEntry {
    pub fn replacement(original: impl Into<String>, replacement: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            original: original.into(),
            replacement: replacement.into(),
            is_enabled: true,
            case_sensitive: false,
            match_whole_word: false,
            source: EntrySource::Manual,
            entry_type: EntryType::Replacement,
        }
    }

    pub fn prompt(word: impl Into<String>) -> Self {
        let w = word.into();
        Self {
            entry_type: EntryType::Prompt,
            replacement: w.clone(),
            ..Self::replacement(w, String::new())
        }
    }

    pub fn is_prompt_entry(&self) -> bool {
        self.entry_type == EntryType::Prompt
    }

    pub fn is_replacement_entry(&self) -> bool {
        self.entry_type == EntryType::Replacement
    }
}

/// Digits / spaces / list separators such as `123`, `1 2 3`, `1,2,3`, `1-2-3`.
const DIGIT_LIST_SEPARATORS: &[char] = &[' ', '\t', '\n', '\r', ',', '，', '、', '.', '-'];

fn is_list_digit(c: char) -> bool {
    c.is_ascii_digit() || ('０'..='９').contains(&c)
}

/// `original` is only digits plus spaces / common list separators.
pub fn is_digit_list_original(original: &str) -> bool {
    let mut has_digit = false;
    for c in original.chars() {
        if is_list_digit(c) {
            has_digit = true;
        } else if c.is_whitespace() || DIGIT_LIST_SEPARATORS.contains(&c) {
            continue;
        } else {
            return false;
        }
    }
    has_digit
}

fn is_ascii_digit_original(original: &str) -> bool {
    !original.is_empty() && original.chars().all(|c| c.is_ascii_digit())
}

/// Replacement is empty, whitespace, and/or punctuation (no letters / digits / CJK).
pub fn is_punctuation_or_whitespace_only(replacement: &str) -> bool {
    replacement.chars().all(|c| !c.is_alphanumeric())
}

/// Digit-list → punctuation/whitespace is an STT footgun (`123` → `。` rewrites `B12345`).
pub fn is_unsafe_digit_list_to_punctuation(original: &str, replacement: &str) -> bool {
    is_digit_list_original(original) && is_punctuation_or_whitespace_only(replacement)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomWordDictionary {
    #[serde(default)]
    pub entries: Vec<CustomWordEntry>,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    /// Accept the persisted timestamp value without changing its representation.
    #[serde(default, deserialize_with = "deserialize_ignored_any")]
    pub last_modified: Option<serde_json::Value>,
}

fn deserialize_ignored_any<'de, D>(deserializer: D) -> Result<Option<serde_json::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(serde_json::Value::deserialize(deserializer)?))
}

impl Default for CustomWordDictionary {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            is_enabled: true,
            last_modified: None,
        }
    }
}

/// Result of [`CustomWordDictionary::merge_by_original`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DictionaryMergeStats {
    pub added: usize,
    pub updated: usize,
}

impl CustomWordDictionary {
    pub fn load_path(path: &Path) -> Result<Self, LailaisayError> {
        let data = std::fs::read(path)?;
        Self::from_json_bytes(&data)
    }

    /// Parse dictionary JSON (file contents or the embedded default).
    pub fn from_json_bytes(data: &[u8]) -> Result<Self, LailaisayError> {
        let mut dict: Self = serde_json::from_slice(data)?;
        dict.drop_unsafe_replacements();
        Ok(dict)
    }

    /// Drop replacement entries that would rewrite digit lists as punctuation.
    pub fn drop_unsafe_replacements(&mut self) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| {
            !e.is_replacement_entry()
                || !is_unsafe_digit_list_to_punctuation(&e.original, &e.replacement)
        });
        before - self.entries.len()
    }

    pub fn enabled_entries(&self) -> Vec<&CustomWordEntry> {
        if !self.is_enabled {
            return Vec::new();
        }
        self.entries.iter().filter(|e| e.is_enabled).collect()
    }

    pub fn enabled_prompt_entries(&self) -> Vec<&CustomWordEntry> {
        self.enabled_entries()
            .into_iter()
            .filter(|e| e.is_prompt_entry())
            .collect()
    }

    pub fn enabled_replacement_entries(&self) -> Vec<&CustomWordEntry> {
        self.enabled_entries()
            .into_iter()
            .filter(|e| e.is_replacement_entry())
            .collect()
    }

    /// Whisper initial-prompt biasing text (context sentence, not a raw list).
    pub fn prompt_text(&self) -> String {
        let words: Vec<&str> = self
            .enabled_prompt_entries()
            .into_iter()
            .map(|e| e.original.as_str())
            .collect();
        if words.is_empty() {
            return String::new();
        }
        let contains_chinese = words.iter().any(|w| {
            w.chars().any(|c| {
                let v = c as u32;
                (0x4E00..=0x9FFF).contains(&v) || (0x3400..=0x4DBF).contains(&v)
            })
        });
        if contains_chinese {
            format!("以下內容可能提及：{}。", words.join("、"))
        } else {
            format!("The following may mention: {}.", words.join(", "))
        }
    }

    /// Lines for the Dictate polish vocabulary block (existing dictionary only).
    /// Prompt entries are preferred spellings; replacement entries are
    /// `original → replacement` (phonetic / ASR near-matches).
    pub fn polish_vocabulary_block(&self) -> Option<String> {
        if !self.is_enabled {
            return None;
        }
        let mut lines = Vec::new();
        for e in self.enabled_prompt_entries() {
            let term = e.original.trim();
            if !term.is_empty() {
                lines.push(format!("- \"{term}\""));
            }
        }
        for e in self.enabled_replacement_entries() {
            let from = e.original.trim();
            let to = e.replacement.trim();
            if from.is_empty() || to.is_empty() {
                continue;
            }
            if from == to {
                lines.push(format!("- \"{to}\""));
            } else {
                lines.push(format!("- \"{from}\" → \"{to}\""));
            }
        }
        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    pub fn add_entry(&mut self, entry: CustomWordEntry) {
        if entry.is_replacement_entry()
            && is_unsafe_digit_list_to_punctuation(&entry.original, &entry.replacement)
        {
            return;
        }
        if !self.entries.iter().any(|e| e.original == entry.original) {
            self.entries.push(entry);
        }
    }

    /// Upsert imported entries by trimmed `original`.
    ///
    /// Existing rows keep their `id` and `source`. New originals keep the
    /// imported `id` unless it collides, and Manual source becomes Imported.
    pub fn merge_by_original(&mut self, imported: Self) -> DictionaryMergeStats {
        let mut stats = DictionaryMergeStats::default();
        for mut entry in imported.entries {
            let original = entry.original.trim();
            if original.is_empty() {
                continue;
            }
            if original != entry.original {
                entry.original = original.to_string();
            }
            if entry.is_replacement_entry()
                && is_unsafe_digit_list_to_punctuation(&entry.original, &entry.replacement)
            {
                continue;
            }
            if let Some(existing) = self
                .entries
                .iter_mut()
                .find(|e| e.original == entry.original)
            {
                existing.replacement = entry.replacement;
                existing.is_enabled = entry.is_enabled;
                existing.case_sensitive = entry.case_sensitive;
                existing.match_whole_word = entry.match_whole_word;
                existing.entry_type = entry.entry_type;
                stats.updated += 1;
            } else {
                if self.entries.iter().any(|e| e.id == entry.id) {
                    entry.id = Uuid::new_v4();
                }
                if entry.source == EntrySource::Manual {
                    entry.source = EntrySource::Imported;
                }
                self.entries.push(entry);
                stats.added += 1;
            }
        }
        stats
    }

    pub fn remove_entry(&mut self, id: Uuid) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    pub fn save_path(&self, path: &Path) -> Result<(), LailaisayError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut sanitized = self.clone();
        sanitized.drop_unsafe_replacements();
        let json = serde_json::to_string_pretty(&sanitized)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Apply replacement-type entries. Longer `original` wins first.
    ///
    /// Digit-list → punctuation entries are never applied. ASCII digit
    /// originals also refuse to match inside a longer alphanumeric token
    /// (`123` must not rewrite `B12345`). CJK originals keep substring match.
    pub fn apply_replacements(&self, text: &str) -> String {
        if !self.is_enabled || self.entries.is_empty() || text.is_empty() {
            return text.to_string();
        }
        let mut entries = self.enabled_replacement_entries();
        entries.retain(|e| !is_unsafe_digit_list_to_punctuation(&e.original, &e.replacement));
        entries.sort_by(|a, b| b.original.chars().count().cmp(&a.original.chars().count()));

        let mut result = text.to_string();
        for entry in entries {
            if is_ascii_digit_original(&entry.original) {
                result = replace_ascii_digit_original(&result, &entry.original, &entry.replacement);
            } else if entry.match_whole_word {
                result = apply_whole_word(
                    &result,
                    &entry.original,
                    &entry.replacement,
                    entry.case_sensitive,
                );
            } else if entry.case_sensitive {
                result = result.replace(&entry.original, &entry.replacement);
            } else {
                result = replace_ignore_ascii_case(&result, &entry.original, &entry.replacement);
            }
        }
        result
    }

    pub fn import_csv(csv: &str) -> Vec<CustomWordEntry> {
        let mut entries = Vec::new();
        for line in csv.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let mut parts = trimmed.splitn(2, ',');
            let Some(original) = parts.next().map(str::trim) else {
                continue;
            };
            let Some(replacement) = parts.next().map(str::trim) else {
                continue;
            };
            if original.is_empty() || replacement.is_empty() {
                continue;
            }
            if is_unsafe_digit_list_to_punctuation(original, replacement) {
                continue;
            }
            let mut entry = CustomWordEntry::replacement(original, replacement);
            entry.source = EntrySource::Imported;
            entries.push(entry);
        }
        entries
    }

    pub fn export_csv(&self) -> String {
        let mut lines = vec![
            "# Custom Word Dictionary".into(),
            "# Format: original,replacement".into(),
        ];
        for entry in &self.entries {
            lines.push(format!("{},{}", entry.original, entry.replacement));
        }
        lines.join("\n")
    }
}

/// Replace an ASCII digit run only when it is not inside a longer alnum token.
fn replace_ascii_digit_original(text: &str, original: &str, replacement: &str) -> String {
    if original.is_empty() {
        return text.to_string();
    }
    let mut out = String::new();
    let mut hay = text;
    while let Some(idx) = hay.find(original) {
        let before = hay[..idx].chars().next_back();
        let after_idx = idx + original.len();
        let after = hay.get(after_idx..).and_then(|s| s.chars().next());
        let inside_alnum = before.is_some_and(|c| c.is_ascii_alphanumeric())
            || after.is_some_and(|c| c.is_ascii_alphanumeric());
        out.push_str(&hay[..idx]);
        if inside_alnum {
            out.push_str(original);
        } else {
            out.push_str(replacement);
        }
        hay = &hay[after_idx..];
    }
    out.push_str(hay);
    out
}

fn apply_whole_word(text: &str, original: &str, replacement: &str, case_sensitive: bool) -> String {
    let is_cjk = original.chars().any(is_cjk_script);
    if is_cjk {
        if case_sensitive {
            text.replace(original, replacement)
        } else {
            replace_ignore_ascii_case(text, original, replacement)
        }
    } else {
        let pattern = format!(r"\b{}\b", escape(original));
        match RegexBuilder::new(&pattern)
            .case_insensitive(!case_sensitive)
            .build()
        {
            Ok(re) => re.replace_all(text, replacement).into_owned(),
            Err(_) => text.replace(original, replacement),
        }
    }
}

fn replace_ignore_ascii_case(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    // For CJK, case folding is a no-op; this still handles mixed ASCII.
    let re = match RegexBuilder::new(&escape(needle))
        .case_insensitive(true)
        .build()
    {
        Ok(re) => re,
        Err(_) => return haystack.replace(needle, replacement),
    };
    re.replace_all(haystack, replacement).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacements_longest_first() {
        let mut dict = CustomWordDictionary::default();
        dict.add_entry(CustomWordEntry::replacement("台灣", "臺灣"));
        dict.add_entry(CustomWordEntry::replacement("台灣大學", "臺灣大學"));
        assert_eq!(dict.apply_replacements("台灣大學在台灣"), "臺灣大學在臺灣");
    }

    #[test]
    fn whole_word_ascii() {
        let mut dict = CustomWordDictionary::default();
        let mut e = CustomWordEntry::replacement("cat", "dog");
        e.match_whole_word = true;
        dict.add_entry(e);
        assert_eq!(dict.apply_replacements("cat catalog"), "dog catalog");
    }

    #[test]
    fn prompt_text_zh() {
        let mut dict = CustomWordDictionary::default();
        dict.add_entry(CustomWordEntry::prompt("當責"));
        dict.add_entry(CustomWordEntry::prompt("燈塔學校"));
        assert_eq!(dict.prompt_text(), "以下內容可能提及：當責、燈塔學校。");
    }

    #[test]
    fn polish_vocabulary_block_lists_prompt_and_replacement() {
        let mut dict = CustomWordDictionary::default();
        dict.add_entry(CustomWordEntry::prompt("當責"));
        dict.add_entry(CustomWordEntry::replacement("台南", "臺南"));
        let block = dict.polish_vocabulary_block().expect("vocab");
        assert!(block.contains("\"當責\""), "{block}");
        assert!(block.contains("\"台南\" → \"臺南\""), "{block}");
        dict.is_enabled = false;
        assert!(dict.polish_vocabulary_block().is_none());
    }

    #[test]
    fn serde_roundtrip_and_legacy_defaults() {
        let json = r#"{"entries":[{"original":"裏","replacement":"裡"}],"isEnabled":true}"#;
        let dict: CustomWordDictionary = serde_json::from_str(json).unwrap();
        assert_eq!(dict.entries[0].replacement, "裡");
        assert!(dict.entries[0].is_enabled);
        assert_eq!(dict.entries[0].entry_type, EntryType::Replacement);
    }

    #[test]
    fn csv_import() {
        let entries = CustomWordDictionary::import_csv("# comment\n台灣,臺灣\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].original, "台灣");
    }

    #[test]
    fn merge_by_original_upserts_and_keeps_ids() {
        let mut dest = CustomWordDictionary::default();
        let kept = CustomWordEntry::replacement("台灣", "台湾");
        let kept_id = kept.id;
        dest.add_entry(kept);
        dest.add_entry(CustomWordEntry::replacement("碰撞", "id-holder"));

        let mut imported = CustomWordDictionary::default();
        let mut updated = CustomWordEntry::replacement("台灣", "臺灣");
        updated.id = Uuid::new_v4();
        updated.case_sensitive = true;
        imported.add_entry(updated);
        imported.add_entry(CustomWordEntry::replacement("台南", "臺南"));
        let mut colliding = CustomWordEntry::replacement("新詞", "新字");
        colliding.id = dest.entries[1].id;
        imported.add_entry(colliding);
        imported.add_entry(CustomWordEntry::replacement("   ", "skip"));

        let stats = dest.merge_by_original(imported);
        assert_eq!(stats.updated, 1);
        assert_eq!(stats.added, 2);
        assert_eq!(dest.entries.len(), 4);

        let tw = dest.entries.iter().find(|e| e.original == "台灣").unwrap();
        assert_eq!(tw.id, kept_id);
        assert_eq!(tw.replacement, "臺灣");
        assert!(tw.case_sensitive);
        assert_eq!(tw.source, EntrySource::Manual);

        let tn = dest.entries.iter().find(|e| e.original == "台南").unwrap();
        assert_eq!(tn.replacement, "臺南");
        assert_eq!(tn.source, EntrySource::Imported);

        let fresh = dest.entries.iter().find(|e| e.original == "新詞").unwrap();
        assert_ne!(fresh.id, dest.entries[1].id);
        assert_eq!(fresh.source, EntrySource::Imported);
    }

    #[test]
    fn remove_and_save_roundtrip() {
        let mut dict = CustomWordDictionary::default();
        let e = CustomWordEntry::replacement("foo", "bar");
        let id = e.id;
        dict.add_entry(e);
        assert!(dict.remove_entry(id));
        assert!(!dict.remove_entry(id));
        let path = std::env::temp_dir().join(format!("lailaisay-dict-{}.json", std::process::id()));
        dict.add_entry(CustomWordEntry::replacement("甲", "乙"));
        dict.save_path(&path).unwrap();
        let loaded = CustomWordDictionary::load_path(&path).unwrap();
        assert_eq!(loaded.entries[0].replacement, "乙");
        let _ = std::fs::remove_file(&path);
    }

    fn dict_with_forced_entries(pairs: &[(&str, &str)]) -> CustomWordDictionary {
        let mut dict = CustomWordDictionary::default();
        for (original, replacement) in pairs {
            dict.entries
                .push(CustomWordEntry::replacement(*original, *replacement));
        }
        dict
    }

    #[test]
    fn hostile_digit_to_punct_does_not_rewrite_alnum_token() {
        let dict =
            dict_with_forced_entries(&[("123", "。"), ("1234567", "。"), ("1 2 3 4 5 6 7", "。")]);
        assert_eq!(
            dict.apply_replacements("數字測試 B12345。"),
            "數字測試 B12345。"
        );
        assert_eq!(dict.apply_replacements("B12345"), "B12345");
        assert_eq!(dict.apply_replacements("A12B"), "A12B");
    }

    #[test]
    fn digit_list_to_punctuation_is_refused_including_bare_digits() {
        let dict = dict_with_forced_entries(&[("123", "。"), ("1,2,3", "."), ("1-2", "，")]);
        assert_eq!(dict.apply_replacements("123"), "123");
        assert_eq!(dict.apply_replacements("1,2,3"), "1,2,3");
        assert_eq!(dict.apply_replacements("1-2"), "1-2");
        assert!(is_unsafe_digit_list_to_punctuation("123", "。"));
        assert!(is_unsafe_digit_list_to_punctuation("1234567", "。"));
        assert!(is_unsafe_digit_list_to_punctuation("1 2 3 4 5 6 7", "。"));
        assert!(!is_unsafe_digit_list_to_punctuation("七個習慣", "7個習慣"));
        assert!(!is_unsafe_digit_list_to_punctuation("當責", "當責"));
    }

    #[test]
    fn legitimate_cjk_replacements_still_apply() {
        let mut dict = CustomWordDictionary::default();
        dict.add_entry(CustomWordEntry::replacement("七個習慣", "7個習慣"));
        dict.add_entry(CustomWordEntry::replacement("當責", "當責"));
        assert_eq!(dict.apply_replacements("讀七個習慣"), "讀7個習慣");
        assert_eq!(dict.apply_replacements("當責"), "當責");
    }

    #[test]
    fn ascii_digit_original_does_not_match_inside_alnum_token() {
        let dict = dict_with_forced_entries(&[("123", "一二三")]);
        assert_eq!(dict.apply_replacements("B12345"), "B12345");
        assert_eq!(dict.apply_replacements("A12B"), "A12B");
        assert_eq!(dict.apply_replacements("123"), "一二三");
        assert_eq!(dict.apply_replacements("編號 123。"), "編號 一二三。");
    }

    #[test]
    fn add_merge_import_and_persist_refuse_digit_to_punct() {
        let mut dict = CustomWordDictionary::default();
        dict.add_entry(CustomWordEntry::replacement("123", "。"));
        dict.add_entry(CustomWordEntry::replacement("七個習慣", "7個習慣"));
        assert_eq!(dict.entries.len(), 1);
        assert_eq!(dict.entries[0].original, "七個習慣");

        let mut imported = CustomWordDictionary::default();
        imported
            .entries
            .push(CustomWordEntry::replacement("1234567", "。"));
        imported
            .entries
            .push(CustomWordEntry::replacement("台南", "臺南"));
        let stats = dict.merge_by_original(imported);
        assert_eq!(stats.added, 1);
        assert!(dict.entries.iter().all(|e| e.original != "1234567"));

        let csv = CustomWordDictionary::import_csv("123,。\n當責,當責\n");
        assert_eq!(csv.len(), 1);
        assert_eq!(csv[0].original, "當責");

        let path =
            std::env::temp_dir().join(format!("lailaisay-dict-unsafe-{}.json", std::process::id()));
        let dirty = dict_with_forced_entries(&[("123", "。"), ("當責", "當責")]);
        dirty.save_path(&path).unwrap();
        let loaded = CustomWordDictionary::load_path(&path).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].original, "當責");
        let persisted = std::fs::read_to_string(&path).unwrap();
        assert!(!persisted.contains("\"123\""), "{persisted}");
        let _ = std::fs::remove_file(&path);
    }
}
