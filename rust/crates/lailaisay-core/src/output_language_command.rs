//! Spoken translate-command stripping (program rules, not LLM guess).
//!
//! 借鑑 typefree 開源口令規則模式（kdsz001/typefree OutputLanguageCommand），
//! 非官方 Typeless system prompt；亦非把 GPL 產品名稱寫進 lailaisay UI。
//! lailaisay-owned rewrite inspired by those patterns — not a verbatim copy,
//! and not an invented Typeless official prompt.

/// Where the cue sat on the utterance (start or end only; mid-sentence is content).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandPosition {
    Leading,
    Trailing,
}

/// A detected speak-and-translate cue, already stripped from the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpokenTranslateCommand {
    /// lailaisay `output_language` code (`en`, `zh`, `ja`, …).
    pub target: String,
    pub position: CommandPosition,
    pub matched_phrase: String,
    pub stripped_text: String,
}

/// One row in the extensible cue table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageCue {
    pub target: &'static str,
    pub phrase: &'static str,
}

/// 中／英（及少數日韓）口令。長句優先匹配。可再往表尾加列，不必改偵測邏輯。
pub const LANGUAGE_CUES: &[LanguageCue] = &[
    // English
    LanguageCue {
        target: "en",
        phrase: "translate to english",
    },
    LanguageCue {
        target: "en",
        phrase: "翻譯成英語",
    },
    LanguageCue {
        target: "en",
        phrase: "翻譯成英文",
    },
    LanguageCue {
        target: "en",
        phrase: "翻译成英语",
    },
    LanguageCue {
        target: "en",
        phrase: "翻译成英文",
    },
    LanguageCue {
        target: "en",
        phrase: "翻成英語",
    },
    LanguageCue {
        target: "en",
        phrase: "翻成英文",
    },
    LanguageCue {
        target: "en",
        phrase: "翻成英语",
    },
    LanguageCue {
        target: "en",
        phrase: "转成英文",
    },
    LanguageCue {
        target: "en",
        phrase: "轉成英文",
    },
    LanguageCue {
        target: "en",
        phrase: "轉英文",
    },
    LanguageCue {
        target: "en",
        phrase: "转英文",
    },
    LanguageCue {
        target: "en",
        phrase: "英文輸出",
    },
    LanguageCue {
        target: "en",
        phrase: "輸出英文",
    },
    LanguageCue {
        target: "en",
        phrase: "英文输出",
    },
    LanguageCue {
        target: "en",
        phrase: "输出英文",
    },
    LanguageCue {
        target: "en",
        phrase: "用英語",
    },
    LanguageCue {
        target: "en",
        phrase: "用英文",
    },
    LanguageCue {
        target: "en",
        phrase: "用英语",
    },
    LanguageCue {
        target: "en",
        phrase: "說英文",
    },
    LanguageCue {
        target: "en",
        phrase: "说英文",
    },
    LanguageCue {
        target: "en",
        phrase: "in english",
    },
    LanguageCue {
        target: "en",
        phrase: "english",
    },
    // Chinese
    LanguageCue {
        target: "zh",
        phrase: "翻譯成繁體中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "翻譯成中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "翻译成中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "翻成中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "轉中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "转中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "中文輸出",
    },
    LanguageCue {
        target: "zh",
        phrase: "輸出中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "中文输出",
    },
    LanguageCue {
        target: "zh",
        phrase: "输出中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "用繁體中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "用中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "說中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "说中文",
    },
    LanguageCue {
        target: "zh",
        phrase: "chinese",
    },
    // Japanese
    LanguageCue {
        target: "ja",
        phrase: "翻譯成日文",
    },
    LanguageCue {
        target: "ja",
        phrase: "翻译成日文",
    },
    LanguageCue {
        target: "ja",
        phrase: "翻譯成日語",
    },
    LanguageCue {
        target: "ja",
        phrase: "用日文",
    },
    LanguageCue {
        target: "ja",
        phrase: "用日語",
    },
    LanguageCue {
        target: "ja",
        phrase: "japanese",
    },
    LanguageCue {
        target: "ja",
        phrase: "日本語",
    },
    // Korean
    LanguageCue {
        target: "ko",
        phrase: "翻譯成韓文",
    },
    LanguageCue {
        target: "ko",
        phrase: "用韓文",
    },
    LanguageCue {
        target: "ko",
        phrase: "korean",
    },
];

/// Longer first so 「翻譯成英文」 wins over 「用英文」 / 「english」.
fn cues_longest_first() -> Vec<LanguageCue> {
    let mut cues = LANGUAGE_CUES.to_vec();
    cues.sort_by(|a, b| b.phrase.chars().count().cmp(&a.phrase.chars().count()));
    cues
}

const NEGATIONS: &[&str] = &[
    "don't",
    "do not",
    "never",
    "不需要",
    "不必",
    "不要",
    "不用",
    "不能",
    "無需",
    "不会",
    "不會",
    "没有",
    "沒有",
    "不是",
    "别",
    "別",
    "没",
    "沒",
    "不",
    "not",
];

const MIN_CONTENT_CHARS: usize = 3;

fn is_separator(c: char) -> bool {
    matches!(
        c,
        '，' | '。'
            | '、'
            | '！'
            | '？'
            | '；'
            | '：'
            | ','
            | '.'
            | '!'
            | '?'
            | ';'
            | ':'
            | '"'
            | '“'
            | '”'
            | '‘'
            | '’'
            | '\''
            | '('
            | ')'
            | '（'
            | '）'
            | '['
            | ']'
            | '【'
            | '】'
            | '…'
            | '—'
            | '-'
            | '~'
            | '～'
            | ' '
            | '\t'
            | '\n'
            | '\r'
    )
}

fn is_ascii_letter_phrase(phrase: &str) -> bool {
    phrase.chars().any(|c| c.is_ascii_alphabetic())
}

fn trim_separators(s: &str) -> &str {
    s.trim_matches(is_separator)
}

fn has_punctuation_boundary<I>(chars: I) -> bool
where
    I: IntoIterator<Item = char>,
{
    for c in chars {
        if c == ' ' || c == '\t' {
            continue;
        }
        return is_separator(c);
    }
    false
}

fn has_prefix_ci(text: &str, phrase: &str) -> bool {
    let Some(head) = text.get(..phrase.len()) else {
        return false;
    };
    text.is_char_boundary(phrase.len()) && head.eq_ignore_ascii_case(phrase)
}

fn has_suffix_ci(text: &str, phrase: &str) -> bool {
    if text.len() < phrase.len() {
        return false;
    }
    let start = text.len() - phrase.len();
    text.is_char_boundary(start) && text[start..].eq_ignore_ascii_case(phrase)
}

fn is_negated(before: &str) -> bool {
    let tail = trim_separators(before);
    NEGATIONS.iter().any(|n| {
        tail.len() >= n.len()
            && tail.is_char_boundary(tail.len() - n.len())
            && tail[tail.len() - n.len()..].eq_ignore_ascii_case(n)
    })
}

fn meaningful_char_count(body: &str) -> usize {
    body.chars()
        .filter(|c| c.is_alphanumeric() || crate::chinese::is_chinese_character(*c))
        .count()
}

fn valid_body(body: &str) -> Option<String> {
    let trimmed = trim_separators(body);
    if meaningful_char_count(trimmed) >= MIN_CONTENT_CHARS {
        Some(trimmed.to_string())
    } else {
        None
    }
}

/// Detect a start/end translate cue. Mid-sentence hits are left as content.
pub fn detect_output_language_command(text: &str) -> Option<SpokenTranslateCommand> {
    let trimmed = trim_separators(text);
    if trimmed.is_empty() {
        return None;
    }

    for cue in cues_longest_first() {
        let phrase = cue.phrase;
        let needs_punct = is_ascii_letter_phrase(phrase);

        if has_suffix_ci(trimmed, phrase) {
            let body = &trimmed[..trimmed.len() - phrase.len()];
            let boundary_ok = !needs_punct || has_punctuation_boundary(body.chars().rev());
            if boundary_ok {
                if !is_negated(body) {
                    if let Some(stripped) = valid_body(body) {
                        return Some(SpokenTranslateCommand {
                            target: cue.target.to_string(),
                            position: CommandPosition::Trailing,
                            matched_phrase: phrase.to_string(),
                            stripped_text: stripped,
                        });
                    }
                }
            }
        }

        if has_prefix_ci(trimmed, phrase) {
            let rest = &trimmed[phrase.len()..];
            let boundary_ok = if needs_punct {
                has_punctuation_boundary(rest.chars())
            } else {
                rest.chars().next().is_some_and(is_separator)
            };
            if boundary_ok && !is_negated("") {
                if let Some(stripped) = valid_body(rest) {
                    return Some(SpokenTranslateCommand {
                        target: cue.target.to_string(),
                        position: CommandPosition::Leading,
                        matched_phrase: phrase.to_string(),
                        stripped_text: stripped,
                    });
                }
            }
        }
    }
    None
}

/// Strip a spoken cue and return `(body, Some(target))`, or the original text.
pub fn apply_spoken_translate_command(text: &str) -> (String, Option<String>) {
    match detect_output_language_command(text) {
        Some(cmd) => (cmd.stripped_text, Some(cmd.target)),
        None => (text.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(text: &str) -> Option<(String, CommandPosition, String)> {
        detect_output_language_command(text).map(|c| (c.target, c.position, c.stripped_text))
    }

    #[test]
    fn leading_chinese_needs_pause() {
        let (target, pos, body) = detect("用英文，今天天氣很好").unwrap();
        assert_eq!(target, "en");
        assert_eq!(pos, CommandPosition::Leading);
        assert_eq!(body, "今天天氣很好");

        let (target, _, body) = detect("用英文 我們明天開會").unwrap();
        assert_eq!(target, "en");
        assert_eq!(body, "我們明天開會");

        assert_eq!(
            detect("用英文写信的人越来越少"),
            None,
            "no pause after cue — treat as content"
        );
        assert_eq!(detect("我想用英文寫信給他"), None);
    }

    #[test]
    fn trailing_chinese_and_traditional_cues() {
        let (target, pos, body) = detect("今天天氣很好，用英文").unwrap();
        assert_eq!(target, "en");
        assert_eq!(pos, CommandPosition::Trailing);
        assert_eq!(body, "今天天氣很好");

        let (target, _, body) = detect("明天開會翻譯成英文").unwrap();
        assert_eq!(target, "en");
        assert_eq!(body, "明天開會");

        let (target, _, body) = detect("請把這段用中文輸出。").unwrap();
        assert_eq!(target, "zh");
        assert!(body.contains("這段"), "{body}");
    }

    #[test]
    fn negatives_are_content_not_commands() {
        assert_eq!(detect("這段話不要翻譯成英文"), None);
        assert_eq!(detect("這段話不要翻譯成英文。"), None);
        assert_eq!(detect("別翻譯成英文，今天出門"), None);
        assert_eq!(detect("不用翻譯成英文謝謝"), None);
        assert_eq!(detect("不必翻成英文"), None);
        assert_eq!(detect("Please do not translate to English."), None);
        assert_eq!(detect("never translate to English, keep Chinese"), None);
    }

    #[test]
    fn english_cues_need_punctuation_boundary() {
        assert_eq!(
            detect("I like English"),
            None,
            "bare word English is content"
        );
        assert_eq!(detect("I like English very much"), None);

        let (target, pos, body) = detect("hello world, English").unwrap();
        assert_eq!(target, "en");
        assert_eq!(pos, CommandPosition::Trailing);
        assert_eq!(body, "hello world");

        let (target, _, body) = detect("in English, we should ship tomorrow").unwrap();
        assert_eq!(target, "en");
        assert!(body.contains("ship tomorrow"), "{body}");

        let (target, _, body) = detect("Translate to English: 今天天氣很好").unwrap();
        assert_eq!(target, "en");
        assert_eq!(body, "今天天氣很好");
    }

    #[test]
    fn command_only_or_short_body_stays_content() {
        assert_eq!(detect("用英文"), None);
        assert_eq!(detect("English"), None);
        assert_eq!(detect("用英文。"), None);
        assert_eq!(detect("好，用英文"), None, "body shorter than 3 meaningful");
    }

    #[test]
    fn japanese_and_korean_cues() {
        let (target, _, body) = detect("明天開會，用日文").unwrap();
        assert_eq!(target, "ja");
        assert_eq!(body, "明天開會");

        let (target, _, body) = detect("用韓文，明天開會討論").unwrap();
        assert_eq!(target, "ko");
        assert!(body.contains("明天開會"), "{body}");
    }

    #[test]
    fn apply_returns_original_when_no_cue() {
        let (text, lang) = apply_spoken_translate_command("杭州梅雨季節一般在幾月份");
        assert_eq!(text, "杭州梅雨季節一般在幾月份");
        assert_eq!(lang, None);
    }

    #[test]
    fn longest_cue_wins() {
        let cmd = detect_output_language_command("開會紀錄翻譯成英文").unwrap();
        assert_eq!(cmd.matched_phrase, "翻譯成英文");
        assert_eq!(cmd.target, "en");
        assert_eq!(cmd.stripped_text, "開會紀錄");
    }

    #[test]
    fn cue_table_is_extensible_and_nonempty() {
        assert!(LANGUAGE_CUES.len() >= 20);
        assert!(LANGUAGE_CUES.iter().any(|c| c.target == "en"));
        assert!(LANGUAGE_CUES.iter().any(|c| c.target == "zh"));
        assert!(
            LANGUAGE_CUES.iter().any(|c| c.phrase.contains("翻譯")),
            "Traditional Chinese cues required"
        );
    }
}
