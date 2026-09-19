//! Correction history for auto-learn (threshold 2).

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::chinese::is_chinese_character;
use crate::custom_words::{CustomWordDictionary, CustomWordEntry, EntrySource};
use crate::error::LailaisayError;
use crate::mishearing::is_likely_mishearing;
use crate::paths::correction_history_path;

/// Number of identical corrections required before auto-learning.
pub const LEARNING_THRESHOLD: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionRecord {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    pub original: String,
    pub corrected: String,
    #[serde(default = "one")]
    pub occurrence_count: u32,
    #[serde(default)]
    pub added_to_dictionary: bool,
    #[serde(default)]
    pub last_unix: u64,
}

fn one() -> u32 {
    1
}

impl CorrectionRecord {
    pub fn ready_for_learning(&self) -> bool {
        self.occurrence_count >= LEARNING_THRESHOLD && !self.added_to_dictionary
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionStore {
    #[serde(default)]
    pub records: Vec<CorrectionRecord>,
}

impl CorrectionStore {
    pub fn load_default() -> Self {
        Self::load_path(&correction_history_path()).unwrap_or_default()
    }

    pub fn load_path(path: &Path) -> Result<Self, LailaisayError> {
        let data = std::fs::read(path)?;
        Ok(serde_json::from_slice(&data)?)
    }

    pub fn save_path(&self, path: &Path) -> Result<(), LailaisayError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn save_default(&self) -> Result<(), LailaisayError> {
        self.save_path(&correction_history_path())
    }

    /// Record a clear STT→final substitution. Returns a record ready to learn.
    ///
    /// Only phonetic mishearings are stored (MishearingCheck). Content rewrites
    /// such as 週四→週三 never count toward the threshold.
    pub fn record_pair(&mut self, original: &str, corrected: &str) -> Option<&CorrectionRecord> {
        let Some((orig, corr)) = learnable_substitution(original, corrected) else {
            return None;
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Some(existing) = self
            .records
            .iter_mut()
            .find(|r| r.original == orig && r.corrected == corr)
        {
            existing.occurrence_count = existing.occurrence_count.saturating_add(1);
            existing.last_unix = now;
        } else {
            self.records.push(CorrectionRecord {
                id: Uuid::new_v4(),
                original: orig,
                corrected: corr,
                occurrence_count: 1,
                added_to_dictionary: false,
                last_unix: now,
            });
        }
        self.records.iter().find(|r| r.ready_for_learning())
    }

    /// Add ready pairs to the dictionary (threshold 2).
    pub fn promote_ready(&mut self, dict: &mut CustomWordDictionary) -> usize {
        let mut n = 0;
        for rec in &mut self.records {
            if rec.ready_for_learning() {
                let mut entry = CustomWordEntry::replacement(&rec.original, &rec.corrected);
                entry.source = EntrySource::Learned;
                dict.add_entry(entry);
                rec.added_to_dictionary = true;
                n += 1;
            }
        }
        n
    }
}

/// Infer a substitution and keep it only when it looks like a mishearing.
pub fn learnable_substitution(original: &str, edited: &str) -> Option<(String, String)> {
    let (orig, corr) = infer_substitution(original, edited)?;
    if is_likely_mishearing(&orig, &corr) {
        return Some((orig, corr));
    }
    if let Some((wide_o, wide_c)) = widen_cjk_substitution(original, edited, &orig, &corr) {
        if is_likely_mishearing(&wide_o, &wide_c) {
            return Some((wide_o, wide_c));
        }
    }
    None
}

/// Single-character CJK diffs (`則`/`責`) expand to the preceding CJK syllable
/// (`當則`/`當責`) so a 2-char term can pass MishearingCheck without learning
/// a one-glyph global replace.
fn widen_cjk_substitution(
    original: &str,
    edited: &str,
    orig: &str,
    corr: &str,
) -> Option<(String, String)> {
    if orig.chars().count() > 1 && corr.chars().count() > 1 {
        return None;
    }
    let a: Vec<char> = original.chars().collect();
    let b: Vec<char> = edited.chars().collect();
    let mut i = 0usize;
    while i < a.len() && i < b.len() && a[i] == b[i] {
        i += 1;
    }
    if i == 0 || !is_chinese_character(a[i - 1]) {
        return None;
    }
    let orig_len = orig.chars().count();
    let corr_len = corr.chars().count();
    if i - 1 + orig_len > a.len() || i - 1 + corr_len > b.len() {
        return None;
    }
    let wide_o: String = a[i - 1..i - 1 + orig_len + 1].iter().collect();
    let wide_c: String = b[i - 1..i - 1 + corr_len + 1].iter().collect();
    if wide_o.chars().count() < 2 || wide_c.chars().count() < 2 {
        return None;
    }
    Some((wide_o, wide_c))
}

/// Longest shared prefix/suffix → a single substitution span.
pub fn infer_substitution(original: &str, edited: &str) -> Option<(String, String)> {
    if original == edited || original.is_empty() || edited.is_empty() {
        return None;
    }
    let a: Vec<char> = original.chars().collect();
    let b: Vec<char> = edited.chars().collect();
    let mut i = 0usize;
    while i < a.len() && i < b.len() && a[i] == b[i] {
        i += 1;
    }
    let mut ja = a.len();
    let mut jb = b.len();
    while ja > i && jb > i && a[ja - 1] == b[jb - 1] {
        ja -= 1;
        jb -= 1;
    }
    let orig: String = a[i..ja].iter().collect();
    let corr: String = b[i..jb].iter().collect();
    let orig = orig.trim();
    let corr = corr.trim();
    if orig.is_empty() || corr.is_empty() {
        return None;
    }
    if orig.chars().count() > 40 || corr.chars().count() > 40 {
        return None;
    }
    Some((orig.to_string(), corr.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_middle_substitution() {
        assert_eq!(
            infer_substitution("我們去台北開會", "我們去台南開會"),
            Some(("北".into(), "南".into()))
        );
        assert_eq!(infer_substitution("same", "same"), None);
    }

    #[test]
    fn threshold_two_promotes_mishearing_only() {
        let mut store = CorrectionStore::default();
        let mut dict = CustomWordDictionary::default();
        // 當則→當責 is a phonetic near-miss (widened from 則/責).
        assert!(store.record_pair("強調當則", "強調當責").is_none());
        assert!(store.record_pair("強調當則", "強調當責").is_some());
        assert_eq!(store.promote_ready(&mut dict), 1);
        assert_eq!(dict.apply_replacements("強調當則"), "強調當責");
        assert_eq!(store.promote_ready(&mut dict), 0);
    }

    #[test]
    fn content_rewrites_are_not_learned() {
        let mut store = CorrectionStore::default();
        assert!(store.record_pair("週四開會", "週三開會").is_none());
        assert!(store.record_pair("週四開會", "週三開會").is_none());
        assert!(store.record_pair("去台北", "去台南").is_none());
        assert!(store.record_pair("共5人", "共4人").is_none());
        assert!(store.record_pair("是不是會分段", "是否會分段").is_none());
        assert!(store.records.is_empty());
    }

    #[test]
    fn learnable_substitution_widens_single_cjk_glyph() {
        assert_eq!(
            learnable_substitution("強調當則", "強調當責"),
            Some(("當則".into(), "當責".into()))
        );
        assert_eq!(learnable_substitution("週四", "週三"), None);
    }
}
