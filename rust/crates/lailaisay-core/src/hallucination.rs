use crate::tokens::is_spoken_number_text;
use regex::Regex;
use std::sync::OnceLock;

/// Why this Whisper string will paste empty: blank, `[BLANK_AUDIO]`, or
/// watermark-only junk. `None` if the text is usable (including spoken numbers).
pub fn unusable_whisper_reason(text: &str) -> Option<&'static str> {
    let t = text.trim();
    if t.is_empty() {
        Some("empty")
    } else if t.eq_ignore_ascii_case("[BLANK_AUDIO]") {
        Some("BLANK_AUDIO")
    } else if is_likely_hallucination(t) {
        Some("hallucination")
    } else {
        None
    }
}

/// Diagnostic line for empty / junk STT. Includes language, prompt size, and
/// raw length so a silent paste can be diagnosed from stderr.
pub fn format_unusable_whisper_log(
    raw: &str,
    language: Option<&str>,
    prompt_chars: usize,
) -> Option<String> {
    let reason = unusable_whisper_reason(raw)?;
    Some(format!(
        "[lailaisay-stt] unusable Whisper output ({reason}) language={} prompt_chars={} raw_chars={}",
        language.unwrap_or("auto"),
        prompt_chars,
        raw.chars().count(),
    ))
}

/// Whisper junk filter, ported from `TranscriptionFeature.isLikelyHallucinationText`.
///
/// Short results (≤15 chars) that are exactly a known filler/thanks/goodbye
/// phrase are dropped. Longer YouTube/Bilibili promo sentences are dropped
/// at any length. Repeating patterns (`好好好`, `謝謝謝謝`) are dropped.
/// Digit-only / Chinese-numeral utterances are kept (spoken numbers).
pub fn is_likely_hallucination(text: &str) -> bool {
    let cleaned = text.trim().to_lowercase();
    if is_spoken_number_text(&cleaned) {
        return false;
    }
    if cleaned.chars().count() < 2 {
        return true;
    }

    // Normalize fullwidth/halfwidth parens and colon so watermark variants
    // share one shape before substring checks.
    let normalized = normalize_width(&cleaned);
    const LONG_PATTERNS: &[&str] = &[
        "請不吝點贊",
        "訂閱轉發",
        "打賞支持",
        "明鏡與點點",
        "點點欄目",
        "支持明鏡",
        "歡迎訂閱",
        "記得點贊",
        "喜歡就訂閱",
        // YouTube/Bilibili-style subtitle credits; Whisper often emits these
        // even when the speaker never said them. Contains-match, any length.
        "字幕製作",
        "字幕制作",
    ];
    for pattern in LONG_PATTERNS {
        if normalized.contains(pattern) {
            return true;
        }
    }

    // YouTube-style "watching" outros can exceed the short-utterance cap
    // (`thank you for watching`). Match the whole phrase, not a contains
    // check — `我在觀看這部片` must stay.
    if is_watching_watermark(&normalized) {
        return true;
    }

    if cleaned.chars().count() > 15 {
        return false;
    }

    const COMMON: &[&str] = &[
        "thank you",
        "thank you.",
        "thanks",
        "thanks.",
        "goodbye",
        "bye",
        "bye.",
        "see you",
        "okay",
        "ok",
        "ok.",
        "alright",
        "hello",
        "hi",
        "hey",
        "hey.",
        "subtitle",
        "subtitles",
        "caption",
        "captions",
        "music",
        "bgm",
        "background music",
        "applause",
        "clapping",
        "silence",
        "the end",
        "end",
        "that's it",
        "done",
        "謝謝",
        "謝謝大家",
        "感謝",
        "感謝大家",
        "再見",
        "拜拜",
        "掰掰",
        "下次見",
        "好的",
        "好",
        "沒問題",
        "知道了",
        "你好",
        "哈囉",
        "嗨",
        "大家好",
        "字幕",
        "音樂",
        "背景音樂",
        "掌聲",
        "結束",
        "完了",
        "就這樣",
        "沒了",
        "觀看",
        "觀看！",
        "觀看。",
        "观看",
        "观看！",
        "观看。",
        "请观看",
        "請觀看",
        "請觀看！",
        "请观看！",
        "謝謝收看",
        "谢谢收看",
        "感謝收看",
        "感谢收看",
    ];
    if COMMON.contains(&cleaned.as_str()) {
        return true;
    }
    // Whisper often emits a closed-caption watermark on near-silent clips
    // (`(CC)`, `（CC）`, `[CC]`, `CC`, `C.C.`). Exact COMMON miss after
    // lowercasing because of wrapping punctuation / fullwidth glyphs.
    if is_closed_caption_marker(&cleaned) {
        return true;
    }
    if has_repeating_pattern(&cleaned) {
        return true;
    }
    if is_repeated_single_character(&cleaned) {
        return true;
    }
    false
}

/// Strip trailing/embedded common thanks hallucinations from an otherwise
/// useful result (before the pure-hallucination check).
pub fn strip_common_hallucination_phrases(text: &str) -> String {
    use regex::RegexBuilder;
    // Watching outros first so "Thank you" / "謝謝" / "感謝" do not
    // slice through "thank you for watching" / "謝謝收看" / "感謝收看".
    let mut result = strip_watching_watermarks(text);
    // Longer phrases first so "Thank you." wins over "Thank you".
    for phrase in [
        "Thank you.",
        "Thank you",
        "謝謝大家",
        "謝謝",
        "感謝大家",
        "感謝",
    ] {
        if phrase.is_ascii() {
            if let Ok(re) = RegexBuilder::new(&regex::escape(phrase))
                .case_insensitive(true)
                .build()
            {
                result = re.replace_all(&result, "").into_owned();
            }
        } else {
            result = result.replace(phrase, "");
        }
    }
    let result = strip_subtitle_credit_watermarks(&result);
    strip_closed_caption_watermarks(&result)
}

/// Fold fullwidth parens/colon to halfwidth so credit watermarks share one shape.
fn normalize_width(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '（' => '(',
            '）' => ')',
            '：' => ':',
            _ => c,
        })
        .collect()
}

/// Remove YouTube/Bilibili-style subtitle-credit watermarks, including optional
/// wrapping parens, colon, and a short credit name (貝爾 / 贝尔 / Bell / other).
/// Leaves a real surrounding sentence intact.
fn strip_subtitle_credit_watermarks(text: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(concat!(
            // Parenthesized credit, any name or none:
            // （字幕製作：貝爾） / (字幕制作:Bell)
            r"[（(]\s*字幕(?:製作|制作)(?:\s*[：:]\s*[^）)]*)?\s*[）)]",
            r"|",
            // Unparenthesized credit with colon + short name
            r"字幕(?:製作|制作)\s*[：:]\s*(?:貝爾|贝尔|[Bb]ell|[^\s。，？！；、（()）]{1,20})",
        ))
        .expect("subtitle credit regex")
    });
    re.replace_all(text, "").into_owned()
}

/// True when the whole (already-lowercased, short) utterance is a closed-caption
/// marker: `(CC)`, `（CC）`, `[CC]`, `CC`, `cc`, `C.C.`, optional punctuation.
fn is_closed_caption_marker(text: &str) -> bool {
    let compact: String = text
        .chars()
        .filter_map(|c| {
            let folded = match c {
                '（' => '(',
                '）' => ')',
                '［' | '【' => '[',
                '］' | '】' => ']',
                '｛' => '{',
                '｝' => '}',
                'Ｃ' | 'ｃ' => 'c',
                '．' | '·' | '・' => '.',
                other => other,
            };
            match folded {
                '(' | ')' | '[' | ']' | '{' | '}' | '.' | '。' | ',' | '，' | '!' | '！' | '?'
                | '？' | ';' | '；' | ':' | '：' => None,
                c if c.is_whitespace() => None,
                c => Some(c),
            }
        })
        .collect();
    compact == "cc"
}

/// Remove embedded parenthesized/bracketed closed-caption watermarks so they
/// never get pasted next to a real sentence. Bare `CC` in useful text is kept.
fn strip_closed_caption_watermarks(text: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(concat!(
            r"[（(\[［【｛{]\s*",
            r"[CcＣｃ]\s*[.．·・]?\s*[CcＣｃ]",
            r"\s*[）)\]］】｝}]",
        ))
        .expect("closed caption marker regex")
    });
    re.replace_all(text, "").into_owned()
}

/// True when the whole (already-lowercased / width-normalized) utterance is a
/// YouTube-style watching outro: `觀看！`, `請觀看`, `謝謝收看`,
/// `thank you for watching`, optional punctuation.
fn is_watching_watermark(text: &str) -> bool {
    let compact: String = text
        .chars()
        .filter_map(|c| {
            let folded = match c {
                '（' => '(',
                '）' => ')',
                '［' | '【' => '[',
                '］' | '】' => ']',
                '｛' => '{',
                '｝' => '}',
                '．' | '·' | '・' => '.',
                other => other,
            };
            match folded {
                '(' | ')' | '[' | ']' | '{' | '}' | '.' | '。' | ',' | '，' | '!' | '！' | '?'
                | '？' | ';' | '；' | ':' | '：' | '、' => None,
                c if c.is_whitespace() => None,
                c => Some(c),
            }
        })
        .collect();
    matches!(
        compact.as_str(),
        "觀看"
            | "观看"
            | "请观看"
            | "請觀看"
            | "請观看"
            | "謝謝收看"
            | "谢谢收看"
            | "感謝收看"
            | "感谢收看"
            | "thankyouforwatching"
            | "thanksforwatching"
    )
}

/// Strip a trailing / standalone watching outro. Does not remove `觀看`
/// from a real phrase such as `我在觀看這部片`.
fn strip_watching_watermarks(text: &str) -> String {
    use regex::RegexBuilder;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        RegexBuilder::new(concat!(
            r"[（(\[［【｛{]\s*(?:觀看|观看)\s*[）)\]］】｝}][！!。．.？?]*\s*$",
            r"|",
            r"(^|[\s　。．.！!？?；;，,])(?:觀看|观看)[！!。．.？?]*\s*$",
            r"|",
            r"(?:[\s　]+)?(?:请观看|請觀看|請观看|謝謝收看|谢谢收看|感謝收看|感谢收看|thank you for watching|thanks for watching)[！!。．.？?，,]*\s*$",
        ))
        .case_insensitive(true)
        .build()
        .expect("watching watermark regex")
    });
    re.replace_all(text, |caps: &regex::Captures| {
        caps.get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default()
    })
    .into_owned()
}

fn has_repeating_pattern(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 4 {
        return false;
    }
    let max_pattern_length = chars.len() / 2;
    for pattern_len in 1..=max_pattern_length {
        let pattern: String = chars[..pattern_len].iter().collect();
        let expected_repeats = chars.len() / pattern_len;
        if expected_repeats >= 2 {
            let reconstructed = pattern.repeat(expected_repeats);
            let reconstructed_len = reconstructed.chars().count();
            if text.starts_with(&reconstructed) && reconstructed_len >= chars.len() * 3 / 4 {
                return true;
            }
        }
    }
    false
}

fn is_repeated_single_character(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if text.chars().count() < 3 {
        return false;
    }
    text.chars().all(|c| c == first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_short_thanks() {
        assert!(is_likely_hallucination("Thank you"));
        assert!(is_likely_hallucination("謝謝大家"));
        assert!(is_likely_hallucination("謝謝"));
    }

    #[test]
    fn drops_bilibili_promo() {
        assert!(is_likely_hallucination(
            "請不吝點贊訂閱轉發打賞支持明鏡與點點欄目"
        ));
    }

    #[test]
    fn keeps_real_sentences() {
        assert!(!is_likely_hallucination("我們今天下午開會討論課程設計"));
        assert!(!is_likely_hallucination("I will send the report later"));
    }

    #[test]
    fn drops_repeats() {
        assert!(is_likely_hallucination("好好好"));
        assert!(is_likely_hallucination("謝謝謝謝"));
    }

    #[test]
    fn keeps_spoken_digits_and_chinese_numerals() {
        assert!(!is_likely_hallucination("1"));
        assert!(!is_likely_hallucination("123456"));
        assert!(!is_likely_hallucination("1 2 3 4 5 6"));
        assert!(!is_likely_hallucination("1,2,3"));
        assert!(!is_likely_hallucination("1,2,3,4,5,6,7"));
        assert!(!is_likely_hallucination("一二三四五六"));
        assert!(!is_likely_hallucination("一二三四五六七"));
        assert!(!is_likely_hallucination("一二三四五六。"));
        assert!(!is_likely_hallucination("一、二、三"));
        assert!(!is_likely_hallucination("第一、第二、第三"));
        assert!(!is_likely_hallucination("one, two, three"));
        assert!(!is_likely_hallucination("[1] [2] [3]"));
    }

    #[test]
    fn drops_fullwidth_subtitle_credit_watermark() {
        assert!(is_likely_hallucination("（字幕製作：貝爾）。"));
        assert!(is_likely_hallucination("（字幕制作：贝尔）。"));
    }

    #[test]
    fn drops_halfwidth_subtitle_credit_watermark() {
        assert!(is_likely_hallucination("(字幕製作:貝爾)"));
        assert!(is_likely_hallucination("(字幕制作:Bell)"));
    }

    #[test]
    fn strips_embedded_subtitle_credit_keeps_sentence() {
        let stripped = strip_common_hallucination_phrases("今天開會。（字幕製作：貝爾）");
        assert_eq!(stripped.trim(), "今天開會。");
        assert!(!is_likely_hallucination(stripped.trim()));
    }

    #[test]
    fn strips_watermark_only_to_leftover_punctuation() {
        let stripped = strip_common_hallucination_phrases("（字幕製作：貝爾）。");
        assert!(is_likely_hallucination(stripped.trim()));
    }

    #[test]
    fn drops_closed_caption_markers() {
        for sample in ["(CC)", "（CC）", "[CC]", "CC", "cc", "C.C.", "（cc）。"] {
            assert!(
                is_likely_hallucination(sample),
                "expected closed-caption marker to drop: {sample}"
            );
        }
    }

    #[test]
    fn strips_embedded_closed_caption_keeps_sentence() {
        let halfwidth = strip_common_hallucination_phrases("今天開會。(CC)");
        assert_eq!(halfwidth.trim(), "今天開會。");
        assert!(!is_likely_hallucination(halfwidth.trim()));

        let fullwidth = strip_common_hallucination_phrases("今天開會。（CC）");
        assert_eq!(fullwidth.trim(), "今天開會。");
        assert!(!is_likely_hallucination(fullwidth.trim()));

        let bracketed = strip_common_hallucination_phrases("今天開會。[CC]");
        assert_eq!(bracketed.trim(), "今天開會。");
        assert!(!is_likely_hallucination(bracketed.trim()));
    }

    #[test]
    fn keeps_sentence_with_unwrapped_cc() {
        assert!(!is_likely_hallucination("請把會議記錄 CC 給我"));
        let stripped = strip_common_hallucination_phrases("請把會議記錄 CC 給我");
        assert!(stripped.contains("CC"), "{stripped}");
    }

    #[test]
    fn unusable_reason_covers_empty_blank_and_watermarks() {
        assert_eq!(unusable_whisper_reason(""), Some("empty"));
        assert_eq!(unusable_whisper_reason("   "), Some("empty"));
        assert_eq!(
            unusable_whisper_reason("[BLANK_AUDIO]"),
            Some("BLANK_AUDIO")
        );
        assert_eq!(
            unusable_whisper_reason("  [blank_audio]  "),
            Some("BLANK_AUDIO")
        );
        assert_eq!(unusable_whisper_reason("(CC)"), Some("hallucination"));
        assert_eq!(
            unusable_whisper_reason("（字幕製作：貝爾）"),
            Some("hallucination")
        );
        assert_eq!(unusable_whisper_reason("觀看！"), Some("hallucination"));
        assert_eq!(unusable_whisper_reason("1 2 3"), None);
        assert_eq!(unusable_whisper_reason("今天開會"), None);
        assert_eq!(unusable_whisper_reason("我在觀看這部片"), None);
    }

    #[test]
    fn unusable_log_line_includes_language_prompt_and_raw_len() {
        let line = format_unusable_whisper_log("(CC)", Some("zh"), 42).expect("log");
        assert!(line.contains("hallucination"), "{line}");
        assert!(line.contains("language=zh"), "{line}");
        assert!(line.contains("prompt_chars=42"), "{line}");
        assert!(line.contains("raw_chars=4"), "{line}");
        assert!(format_unusable_whisper_log("1 2 3", Some("zh"), 8).is_none());
    }

    #[test]
    fn drops_watching_watermark_utterances() {
        for sample in [
            "觀看",
            "觀看！",
            "觀看。",
            "请观看",
            "請觀看",
            "謝謝收看",
            "谢谢收看",
            "感謝收看",
            "thank you for watching",
            "Thanks for watching!",
            "Thank you for watching.",
        ] {
            assert!(
                is_likely_hallucination(sample),
                "expected watching watermark to drop: {sample}"
            );
        }
    }

    #[test]
    fn strips_trailing_watching_keeps_sentence() {
        let bang = strip_common_hallucination_phrases("今天開會。觀看！");
        assert_eq!(bang.trim(), "今天開會。");
        assert!(!is_likely_hallucination(bang.trim()));

        let thanks = strip_common_hallucination_phrases("今天開會。謝謝收看");
        assert_eq!(thanks.trim(), "今天開會。");
        assert!(!is_likely_hallucination(thanks.trim()));

        let en = strip_common_hallucination_phrases("今天開會。Thank you for watching.");
        assert_eq!(en.trim(), "今天開會。");
        assert!(!is_likely_hallucination(en.trim()));
    }

    #[test]
    fn keeps_real_sentence_containing_watching() {
        assert!(!is_likely_hallucination("我在觀看這部片"));
        let stripped = strip_common_hallucination_phrases("我在觀看這部片");
        assert!(stripped.contains("觀看"), "{stripped}");
        assert!(!is_likely_hallucination(stripped.trim()));
    }

    #[test]
    fn watching_only_after_strip_is_hallucination() {
        let stripped = strip_common_hallucination_phrases("觀看！");
        assert!(is_likely_hallucination(stripped.trim()));
    }
}
