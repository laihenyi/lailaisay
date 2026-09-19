use zhconv::{zhconv, Variant};

/// CJK Unified Ideographs + Extension A + Extension B .
pub fn is_chinese_character(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF
    )
}

pub fn contains_chinese(text: &str) -> bool {
    text.chars().any(is_chinese_character)
}

/// Convert Simplified → Traditional (Taiwan conventions) using
/// the zhconv Taiwan variant.
pub fn to_traditional(text: &str) -> String {
    if text.is_empty() || !contains_chinese(text) {
        return text.to_string();
    }
    zhconv(text, Variant::ZhTW)
}

/// CJK / kana / hangul — used for whole-word replacement (no `\b` boundaries).
pub fn is_cjk_script(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0x3040..=0x309F
            | 0x30A0..=0x30FF
            | 0xAC00..=0xD7AF
    )
}
