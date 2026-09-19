use crate::chinese::is_chinese_character;
use fancy_regex::{Regex, RegexBuilder};
use std::sync::OnceLock;

/// Options controlling which local (no-LLM) cleanup steps to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextProcessingOptions {
    pub remove_fillers: bool,
    pub resolve_self_corrections: bool,
    /// Hint for primary language; `None` = auto-detect from content.
    pub detected_language: Option<String>,
}

impl Default for TextProcessingOptions {
    fn default() -> Self {
        Self {
            remove_fillers: true,
            resolve_self_corrections: true,
            detected_language: None,
        }
    }
}

/// Pure-function text processor.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextProcessor;

impl TextProcessor {
    pub fn process(&self, text: &str, options: &TextProcessingOptions) -> String {
        if text.is_empty() {
            return String::new();
        }
        let mut result = text.to_string();
        if options.resolve_self_corrections {
            result = self.resolve_self_corrections(&result);
        }
        if options.remove_fillers {
            let lang = options
                .detected_language
                .clone()
                .unwrap_or_else(|| detect_language(&result));
            result = self.remove_filler_words(&result, &lang);
        }
        collapse_whitespace(&result)
    }

    pub fn resolve_self_corrections(&self, text: &str) -> String {
        let mut result = apply_zh_corrections(text);

        const EN_SIGNALS: &[&str] = &[
            r"[,，\s]+(?:sorry\s+)?I\s+mean[t]?[,，\s]+",
            r"[,，\s]+(?:no|nah)[,，\s]+(?:I\s+mean[t]?[,，\s]+)?",
            r"[,，\s]+(?:sorry|wait)[,，\s]+(?:I\s+mean[t]?[,，\s]+)?",
            r"[,，\s]+(?:actually|correction)[,，\s]+",
        ];
        for signal in EN_SIGNALS {
            let pattern = format!(r"([^.!?;\n]*?){signal}([^.!?;\n]+)");
            if let Ok(re) = RegexBuilder::new(&pattern).case_insensitive(true).build() {
                result = replace_all(&re, &result, "$2");
            }
        }
        result
    }

    pub fn remove_filler_words(&self, text: &str, language: &str) -> String {
        let mut result = text.to_string();
        if language.starts_with("zh") || language == "mixed" {
            result = remove_chinese_fillers(&result);
        }
        if !language.starts_with("zh") || language == "mixed" {
            result = remove_english_fillers(&result);
        }
        result
    }
}

/// Ratio of CJK characters.
pub fn detect_language(text: &str) -> String {
    if text.is_empty() {
        return "en".into();
    }
    let total = text.chars().count() as f64;
    let chinese = text.chars().filter(|c| is_chinese_character(*c)).count() as f64;
    let ratio = chinese / total;
    if ratio > 0.3 {
        "zh".into()
    } else if ratio > 0.05 {
        "mixed".into()
    } else {
        "en".into()
    }
}

fn replace_all(re: &Regex, text: &str, rep: &str) -> String {
    re.replace_all(text, rep).into_owned()
}

fn collapse_whitespace(text: &str) -> String {
    static WS: OnceLock<Regex> = OnceLock::new();
    let re = WS.get_or_init(|| Regex::new(r"\s+").expect("ws"));
    replace_all(re, text, " ").trim().to_string()
}

/// Split on sentence-ending punctuation, keeping the delimiter so we can
/// apply `^…` patterns per clause without variable-length lookbehind.
fn map_zh_sentences(text: &str, mut f: impl FnMut(&str) -> String) -> String {
    let mut out = String::new();
    let mut start = 0usize;
    for (idx, ch) in text.char_indices() {
        if matches!(ch, '。' | '；' | '！' | '？' | '\n') {
            let clause = &text[start..idx];
            out.push_str(&f(clause));
            out.push(ch);
            start = idx + ch.len_utf8();
        }
    }
    if start < text.len() {
        out.push_str(&f(&text[start..]));
    }
    out
}

fn apply_zh_corrections(text: &str) -> String {
    // Pause-delimited (ambiguous with negation): 不對 / 不是 must be followed
    // by a pause. Explicit phrases stay lenient.
    const PAUSE_SIGNALS: &[&str] = &[r"不對[，、,\s]+", r"不是[，、,\s]+"];
    const LENIENT_SIGNALS: &[&str] = &[
        r"我是說[，、,\s]*",
        r"我的意思是[，、,\s]*",
        r"更正[，、,\s]*",
    ];

    let apply = |input: &str| {
        let mut result = input.to_string();
        for signal in PAUSE_SIGNALS.iter().chain(LENIENT_SIGNALS) {
            let pattern = format!(r"^([^。；！？\n]*?[，、,\s]+|){signal}([^。；！？\n]+)");
            if let Ok(re) = Regex::new(&pattern) {
                result = replace_all(&re, &result, "$2");
            }
        }
        result
    };
    map_zh_sentences(text, apply)
}

fn remove_chinese_fillers(text: &str) -> String {
    const FILLERS: &[&str] = &[
        r"嗯+[，、\s]*",
        r"啊[，、。\s]",
        r"呃+[，、\s]*",
        r"那個[，、\s]+",
        r"就是說[，、\s]+",
        r"怎麼說[，、\s]+",
        r"然後[，、\s]+(?=然後|嗯|啊|就是)",
    ];
    let mut result = text.to_string();
    for filler in FILLERS {
        let pattern = format!(r"(?:^|(?<=[，、。；！？\s])){filler}");
        if let Ok(re) = Regex::new(&pattern) {
            result = replace_all(&re, &result, "");
        }
    }
    result
}

fn remove_english_fillers(text: &str) -> String {
    const FILLERS: &[&str] = &[
        r"\bum+\b[,;\s]*",
        r"\buh+\b[,;\s]*",
        r"\byou know[,;\s]+",
        r"\bbasically[,;\s]+",
        r"\bI mean[,;\s]+",
        r"\bsort of[,;\s]+",
        r"\bkind of[,;\s]+",
        r"(?:^|(?<=[.!?,;\s]))like[,;\s]+(?=[a-z])",
    ];
    let mut result = text.to_string();
    for filler in FILLERS {
        if let Ok(re) = RegexBuilder::new(filler).case_insensitive(true).build() {
            result = replace_all(&re, &result, "");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zh_opts() -> TextProcessingOptions {
        TextProcessingOptions {
            remove_fillers: true,
            resolve_self_corrections: true,
            detected_language: Some("zh".into()),
        }
    }

    #[test]
    fn epistemic_ying_gai_shi_is_not_a_correction_signal() {
        let p = TextProcessor;
        let o = zh_opts();
        assert_eq!(p.process("應該是吧?", &o), "應該是吧?");
        assert_eq!(p.process("今天應該是星期五。", &o), "今天應該是星期五。");
    }

    #[test]
    fn negation_bu_shi_mid_clause_is_preserved() {
        let p = TextProcessor;
        let o = zh_opts();
        assert_eq!(p.process("我不是故意的。", &o), "我不是故意的。");
        assert_eq!(p.process("這不是問題。", &o), "這不是問題。");
    }

    #[test]
    fn pause_delimited_correction_still_resolves() {
        let p = TextProcessor;
        let o = zh_opts();
        assert_eq!(p.process("五百塊 不是 三百塊", &o), "三百塊");
        assert_eq!(p.process("去台北，不對，去台南。", &o), "去台南。");
    }

    #[test]
    fn clause_initial_correction_signal_drops_signal_only() {
        let p = TextProcessor;
        let o = zh_opts();
        assert_eq!(p.process("不對，去台南。", &o), "去台南。");
    }

    #[test]
    fn clause_initial_negation_without_trailing_pause_is_preserved() {
        let p = TextProcessor;
        let o = zh_opts();
        assert_eq!(p.process("不是的。", &o), "不是的。");
        assert_eq!(p.process("不是這樣的。", &o), "不是這樣的。");
        assert_eq!(
            p.process("這個方案，不是問題。", &o),
            "這個方案，不是問題。"
        );
    }

    #[test]
    fn explicit_correction_phrase_still_resolves() {
        let p = TextProcessor;
        let o = zh_opts();
        assert_eq!(p.process("我要去台北，我是說，台南。", &o), "台南。");
    }

    #[test]
    fn discourse_opener_ji_ben_shang_is_not_removed_as_filler() {
        let p = TextProcessor;
        let o = zh_opts();
        assert_eq!(
            p.process("基本上，我同意你的看法。", &o),
            "基本上，我同意你的看法。"
        );
        assert_eq!(p.process("嗯，那個，我們開始吧。", &o), "我們開始吧。");
    }
}
