use regex::Regex;
use std::sync::OnceLock;

/// Strip raw Whisper special tokens and timestamp leftovers that leak into
/// segment text (`<|zh|>`, `<|1.28|>`, `[0.00]`). Keep unmarked numbers.
pub fn clean_whisper_tokens(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
    static ALT_TS_RE: OnceLock<Regex> = OnceLock::new();
    static WS_RE: OnceLock<Regex> = OnceLock::new();
    let token_re = TOKEN_RE.get_or_init(|| {
        // Standard `<|…|>`, plus missing-pipe / spaced variants token timestamps emit.
        Regex::new(r"<\|[^|>]{0,64}\|>|<\|[^<>\n]{0,64}>|<\s*\|[^|]{0,64}\|\s*>")
            .expect("token regex")
    });
    let alt_ts = ALT_TS_RE.get_or_init(|| {
        // Timestamp leftovers only: [0.00] [1.28] [1:23] (1.28) (3s).
        // Bare [1] / (2) are spoken list items — do not strip those.
        Regex::new(concat!(
            r"\[\s*(?:\d{1,2}:\d{1,2}(?:\.\d+)?|\d+\.\d+)\s*\]",
            r"|",
            r"\(\s*\d+\.\d+\s*s?\s*\)",
            r"|",
            r"\(\s*\d+\s*s\s*\)",
        ))
        .expect("alt timestamp regex")
    });
    let ws_re = WS_RE.get_or_init(|| Regex::new(r"\s+").expect("ws regex"));

    let stripped = token_re.replace_all(text, "");
    let stripped = alt_ts.replace_all(&stripped, "");
    ws_re.replace_all(stripped.trim(), " ").trim().to_string()
}

/// Only explicitly marked metadata is junk; unmarked numbers may be speech.
pub fn is_timestamp_junk_segment(text: &str) -> bool {
    clean_whisper_tokens(text).is_empty()
}

/// Spoken Arabic digits, digit lists, Chinese numerals / 第-ordinals, or
/// English number-word lists — not timestamp junk.
///
/// Number-only lists later prefer Arabic glyphs (`arabicize_spoken_number_text`).
/// Idioms and mixed prose are not spoken-number text and stay unchanged.
pub fn is_spoken_number_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    if is_number_looking_idiom(trimmed) {
        return false;
    }
    is_digit_or_chinese_numeral_text(trimmed) || is_english_number_words_text(trimmed)
}

/// Number-looking idioms / fixed phrases. Whole-utterance check for
/// [`is_spoken_number_text`]; in-sentence conversion skips these spans.
///
/// Includes 萬一／一共 and other 成語 that would otherwise look like numerals
/// (千萬不要 must not become `10000000不要`).
const NUMBER_IDIOMS: &[&str] = &[
    "千千萬萬",
    "千千万万",
    "一模一樣",
    "一模一样",
    "萬無一失",
    "万无一失",
    "千鈞一髮",
    "千钧一发",
    "千辛萬苦",
    "千辛万苦",
    "獨一無二",
    "独一无二",
    "十全十美",
    "三心二意",
    "三言兩語",
    "三言两语",
    "五花八門",
    "五花八门",
    "七零八落",
    "七嘴八舌",
    "八九不離十",
    "八九不离十",
    "十拿九穩",
    "十拿九稳",
    "十之八九",
    "百發百中",
    "百发百中",
    "一五一十",
    "三五成群",
    "七上八下",
    "九牛一毛",
    "接二連三",
    "接二连三",
    "朝三暮四",
    "不三不四",
    "丟三落四",
    "丢三落四",
    "說一不二",
    "说一不二",
    "略知一二",
    "四分五裂",
    "一乾二淨",
    "一干二净",
    "萬一",
    "万一",
    "一共",
    "千萬",
    "千万",
    "萬分",
    "万分",
    "十分",
    "再三",
];

/// All-numeral idioms (萬一 = “just in case”) must not be treated as digit lists.
fn is_number_looking_idiom(text: &str) -> bool {
    let core: String = text
        .chars()
        .filter(|c| !is_ignorable_number_punct(*c) && !c.is_whitespace())
        .collect();
    NUMBER_IDIOMS.iter().any(|idiom| core == *idiom)
}

/// Character spans covered by [`NUMBER_IDIOMS`] (longer phrases first).
fn number_idiom_spans(chars: &[char]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let mut hit = None;
        for idiom in NUMBER_IDIOMS {
            let id: Vec<char> = idiom.chars().collect();
            if i + id.len() <= chars.len() && chars[i..i + id.len()] == id[..] {
                hit = Some(id.len());
                break;
            }
        }
        if let Some(len) = hit {
            spans.push((i, i + len));
            i += len;
        } else {
            i += 1;
        }
    }
    spans
}

fn span_blocked(spans: &[(usize, usize)], index: usize) -> Option<usize> {
    spans
        .iter()
        .find(|&&(s, e)| index >= s && index < e)
        .map(|&(_, e)| e)
}

/// Rewrite spoken-style Chinese numerals to ASCII Arabic digits.
///
/// * Number-only lists keep Whisper's separators (`一、三、五` → `1、3、5`).
/// * Mixed prose converts compound / range numerals (`大概三千六塊` →
///   `大概3600塊`, `兩到三次` → `2到3次`) but leaves idioms (`萬一`、`一共`、
///   `一模一樣`) and concatenated digit-lists (`今天天氣一五八`) alone.
pub fn arabicize_spoken_number_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    if is_spoken_number_text(text) {
        arabicize_number_only_list(text)
    } else {
        arabicize_in_sentence_numerals(text)
    }
}

fn arabicize_number_only_list(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if is_fullwidth_digit(c) {
            out.push(fullwidth_to_ascii(c));
            i += 1;
            continue;
        }
        if !is_chinese_numeral(c) {
            out.push(c);
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < chars.len() && is_chinese_numeral(chars[i]) {
            i += 1;
        }
        let run: String = chars[start..i].iter().collect();
        out.push_str(&chinese_numeral_run_to_arabic(&run));
    }
    out
}

/// In-sentence 漢字數字 → 阿拉伯數字, skipping [`NUMBER_IDIOMS`] spans.
fn arabicize_in_sentence_numerals(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let blocked = number_idiom_spans(&chars);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some(end) = span_blocked(&blocked, i) {
            for ch in &chars[i..end] {
                out.push(*ch);
            }
            i = end;
            continue;
        }

        if chars[i] == '第' {
            out.push('第');
            i += 1;
            if let Some((end, arabic)) = take_convertible_run(&chars, i, true) {
                out.push_str(&arabic);
                i = end;
            }
            continue;
        }

        if is_chinese_numeral(chars[i]) {
            let (run_end, run) = take_numeral_run(&chars, i);
            let range = if run_end < chars.len() && matches!(chars[run_end], '到' | '至') {
                take_convertible_run(&chars, run_end + 1, true)
            } else {
                None
            };
            if is_convertible_in_sentence(&run) || range.is_some() {
                out.push_str(&chinese_numeral_run_to_arabic(&run));
                if let Some((end, arabic)) = range {
                    out.push(chars[run_end]);
                    out.push_str(&arabic);
                    i = end;
                } else {
                    i = run_end;
                }
                continue;
            }
        }

        if is_fullwidth_digit(chars[i]) {
            out.push(fullwidth_to_ascii(chars[i]));
            i += 1;
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }
    out
}

fn take_numeral_run(chars: &[char], start: usize) -> (usize, String) {
    let mut end = start;
    while end < chars.len() && is_chinese_numeral(chars[end]) {
        end += 1;
    }
    (end, chars[start..end].iter().collect())
}

fn take_convertible_run(
    chars: &[char],
    start: usize,
    allow_single: bool,
) -> Option<(usize, String)> {
    if start >= chars.len() || !is_chinese_numeral(chars[start]) {
        return None;
    }
    let (end, run) = take_numeral_run(chars, start);
    if run.is_empty() {
        return None;
    }
    if allow_single || is_convertible_in_sentence(&run) {
        Some((end, chinese_numeral_run_to_arabic(&run)))
    } else {
        None
    }
}

/// Compound spoken numbers (十一, 三百, 三千六) and bare 十. Isolated 一/二/百
/// stay put so 一定 / 百貨 / 一五八 are not rewritten.
fn is_convertible_in_sentence(run: &str) -> bool {
    if !should_parse_as_chinese_number(run) {
        return false;
    }
    let n = run.chars().count();
    n >= 2 || matches!(run, "十" | "拾")
}

fn is_digit_or_chinese_numeral_text(trimmed: &str) -> bool {
    let core: String = trimmed
        .chars()
        .filter(|c| !is_ignorable_number_punct(*c))
        .collect();
    if core.is_empty() {
        return false;
    }
    if !core
        .chars()
        .any(|c| is_chinese_numeral(c) || is_timestamp_digit(c))
    {
        return false;
    }
    core.chars().all(|c| {
        is_chinese_numeral(c)
            || is_ordinal_marker(c)
            || is_timestamp_digit(c)
            || is_number_list_separator(c)
            || matches!(c, '.' | ':' | '-')
            || c.is_whitespace()
    })
}

/// `one, two, three` / `twenty one` — Whisper sometimes emits English words
/// for a digit-only clip even when decode language is zh.
fn is_english_number_words_text(text: &str) -> bool {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            cur.extend(c.to_lowercase());
            continue;
        }
        if is_number_list_separator(c)
            || is_ignorable_number_punct(c)
            || c.is_whitespace()
            || matches!(c, '.' | ':' | '-')
        {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            continue;
        }
        return false;
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    if tokens.is_empty() {
        return false;
    }
    let has_number = tokens
        .iter()
        .any(|t| is_english_number_word(t) || is_digit_list_token(t));
    has_number
        && tokens.iter().all(|t| {
            is_english_number_word(t) || is_english_number_connector(t) || is_digit_list_token(t)
        })
}

fn is_english_number_word(token: &str) -> bool {
    matches!(
        token,
        "zero"
            | "oh"
            | "nought"
            | "one"
            | "two"
            | "three"
            | "four"
            | "five"
            | "six"
            | "seven"
            | "eight"
            | "nine"
            | "ten"
            | "eleven"
            | "twelve"
            | "thirteen"
            | "fourteen"
            | "fifteen"
            | "sixteen"
            | "seventeen"
            | "eighteen"
            | "nineteen"
            | "twenty"
            | "thirty"
            | "forty"
            | "fifty"
            | "sixty"
            | "seventy"
            | "eighty"
            | "ninety"
            | "hundred"
            | "thousand"
            | "million"
            | "billion"
            | "first"
            | "second"
            | "third"
            | "fourth"
            | "fifth"
            | "sixth"
            | "seventh"
            | "eighth"
            | "ninth"
            | "tenth"
    )
}

fn is_english_number_connector(token: &str) -> bool {
    token == "and"
}

fn is_ignorable_number_punct(c: char) -> bool {
    matches!(
        c,
        '。' | '，'
            | '、'
            | '．'
            | '?'
            | '!'
            | '？'
            | '！'
            | '('
            | ')'
            | '['
            | ']'
            | '（'
            | '）'
            | '［'
            | '］'
    )
}

fn is_number_list_separator(c: char) -> bool {
    matches!(
        c,
        ',' | '，' | '、' | ';' | '；' | '/' | '／' | '·' | '・' | '‧'
    )
}

fn is_ordinal_marker(c: char) -> bool {
    c == '第'
}

fn is_digit_list_token(token: &str) -> bool {
    let t = token.trim();
    !t.is_empty()
        && t.chars().all(|c| {
            is_timestamp_digit(c) || is_number_list_separator(c) || matches!(c, '.' | ':' | '-')
        })
}

fn is_chinese_numeral(c: char) -> bool {
    "零〇一二三四五六七八九十百千万萬億两兩壹贰貳叁參肆伍陆陸柒捌玖拾幺".contains(c)
}

fn is_fullwidth_digit(c: char) -> bool {
    ('０'..='９').contains(&c)
}

fn is_timestamp_digit(c: char) -> bool {
    c.is_ascii_digit() || is_fullwidth_digit(c)
}

fn fullwidth_to_ascii(c: char) -> char {
    if is_fullwidth_digit(c) {
        char::from(b'0' + (c as u32 - '０' as u32) as u8)
    } else {
        c
    }
}

fn chinese_digit_value(c: char) -> Option<u8> {
    match c {
        '零' | '〇' => Some(0),
        '一' | '壹' | '幺' => Some(1),
        '二' | '贰' | '貳' | '两' | '兩' => Some(2),
        '三' | '叁' | '參' => Some(3),
        '四' | '肆' => Some(4),
        '五' | '伍' => Some(5),
        '六' | '陆' | '陸' => Some(6),
        '七' | '柒' => Some(7),
        '八' | '捌' => Some(8),
        '九' | '玖' => Some(9),
        _ => None,
    }
}

fn chinese_place_value(c: char) -> Option<u64> {
    match c {
        '十' | '拾' => Some(10),
        '百' => Some(100),
        '千' => Some(1000),
        '万' | '萬' => Some(10_000),
        '億' => Some(100_000_000),
        _ => None,
    }
}

fn chinese_numeral_run_to_arabic(run: &str) -> String {
    if run.is_empty() {
        return String::new();
    }
    if should_parse_as_chinese_number(run) {
        if let Some(n) = parse_chinese_number(run) {
            return n.to_string();
        }
    }
    let mut out = String::new();
    for c in run.chars() {
        if let Some(d) = chinese_digit_value(c) {
            out.push(char::from(b'0' + d));
        } else if let Some(place) = chinese_place_value(c) {
            out.push_str(&place.to_string());
        }
    }
    out
}

/// Compound numbers (`十一`, `二十三`, `一百零五`) have place values and never
/// two non-zero digit glyphs in a row. Concatenated lists (`一二三`,
/// `一二三四五六七八九十`) fail that shape and stay glyph-by-glyph.
fn should_parse_as_chinese_number(run: &str) -> bool {
    let mut saw_place = false;
    let mut prev_nonzero_digit = false;
    for c in run.chars() {
        if chinese_place_value(c).is_some() {
            saw_place = true;
            prev_nonzero_digit = false;
            continue;
        }
        match chinese_digit_value(c) {
            Some(0) => prev_nonzero_digit = false,
            Some(_) => {
                if prev_nonzero_digit {
                    return false;
                }
                prev_nonzero_digit = true;
            }
            None => return false,
        }
    }
    saw_place
}

fn parse_chinese_number(run: &str) -> Option<u64> {
    let mut total: u64 = 0;
    let mut section: u64 = 0;
    let mut last_digit: Option<u64> = None;
    let mut last_place: Option<u64> = None;
    let mut zero_after_place = false;
    for c in run.chars() {
        if let Some(d) = chinese_digit_value(c) {
            last_digit = Some(u64::from(d));
            if d == 0 && last_place.is_some() {
                zero_after_place = true;
            }
            continue;
        }
        let place = chinese_place_value(c)?;
        let coeff = last_digit.take().unwrap_or(if place == 10 { 1 } else { 0 });
        zero_after_place = false;
        last_place = Some(place);
        if place >= 10_000 {
            section += coeff;
            total = total.saturating_add(section.saturating_mul(place));
            section = 0;
        } else {
            section = section.saturating_add(coeff.saturating_mul(place));
        }
    }
    if let Some(d) = last_digit {
        if let Some(p) = last_place {
            // Spoken shorthand: 三千六 = 3600, 五百三 = 530, 一萬二 = 12000.
            // 一百零五 keeps ones (零 marks the skipped place).
            if !zero_after_place && p >= 100 {
                section = section.saturating_add(d.saturating_mul(p / 10));
            } else {
                section = section.saturating_add(d);
            }
        } else {
            section = section.saturating_add(d);
        }
    }
    Some(total.saturating_add(section))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removes_explicit_metadata_only() {
        for text in ["<|0.00|>", "[0.00]", "<|zh|>"] {
            assert!(is_timestamp_junk_segment(text));
        }
        assert_eq!(clean_whisper_tokens("你好<|1.28|>世界"), "你好世界");
        assert_eq!(clean_whisper_tokens("  hello  "), "hello");
    }
    #[test]
    fn preserves_dictated_numbers_and_context() {
        for text in [
            "1",
            "１",
            "1.28",
            "0.00",
            "1 2 3 4",
            "１、２、３、４",
            "一二三四",
            "123456",
            "1,2,3",
            "1,2,3,4,5,6,7",
            "一二三四五六七",
            "一、二、三",
            "[1] [2] [3]",
            "(1)(2)(3)",
            "123開始",
            "答案是123",
            "選1或2",
            "下午3點見面",
        ] {
            assert_eq!(clean_whisper_tokens(text), text);
            assert!(!is_timestamp_junk_segment(text), "{text}");
        }
        for text in [
            "1",
            "１",
            "1.28",
            "0.00",
            "1 2 3 4",
            "１、２、３、４",
            "一二三四",
            "123456",
            "1,2,3",
            "1,2,3,4,5,6,7",
            "一二三四五六七",
            "一、二、三",
            "第一、第二、第三",
            "[1] [2] [3]",
            "(1)(2)(3)",
            "one, two, three",
            "one two three four five six seven",
        ] {
            assert!(is_spoken_number_text(text), "{text}");
        }
        assert!(!is_spoken_number_text("字幕製作"));
        assert!(!is_spoken_number_text("(CC)"));
        assert!(!is_spoken_number_text("今天開會"));
        assert!(!is_spoken_number_text("一模一樣"));
        assert!(!is_spoken_number_text("三心二意"));
        assert!(!is_spoken_number_text("萬一"));
        assert!(!is_spoken_number_text("今天天氣一五八"));
    }

    #[test]
    fn arabicize_number_only_lists_preserves_separators() {
        assert_eq!(
            arabicize_spoken_number_text("一、三、五、七、九"),
            "1、3、5、7、9"
        );
        assert_eq!(arabicize_spoken_number_text("一，三，五"), "1，3，5");
        assert_eq!(arabicize_spoken_number_text("一, 三, 五"), "1, 3, 5");
        assert_eq!(arabicize_spoken_number_text("一二三四五六七"), "1234567");
        assert_eq!(
            arabicize_spoken_number_text("一 二 三 四 五 六 七"),
            "1 2 3 4 5 6 7"
        );
        assert_eq!(
            arabicize_spoken_number_text("第一、第二、第三"),
            "第1、第2、第3"
        );
        assert_eq!(arabicize_spoken_number_text("八、九、十"), "8、9、10");
        assert_eq!(
            arabicize_spoken_number_text("十一、十二、十三"),
            "11、12、13"
        );
        assert_eq!(arabicize_spoken_number_text("１、２、３"), "1、2、3");
        assert_eq!(arabicize_spoken_number_text("1 2 3"), "1 2 3");
        assert_eq!(
            arabicize_spoken_number_text("1,2,3,4,5,6,7"),
            "1,2,3,4,5,6,7"
        );
        assert_eq!(
            arabicize_spoken_number_text("one, two, three"),
            "one, two, three"
        );
        assert_eq!(
            arabicize_spoken_number_text("今天天氣一五八"),
            "今天天氣一五八"
        );
        assert_eq!(arabicize_spoken_number_text("一模一樣"), "一模一樣");
        assert_eq!(arabicize_spoken_number_text("三心二意"), "三心二意");
        assert_eq!(arabicize_spoken_number_text("萬一"), "萬一");
        assert_eq!(arabicize_spoken_number_text("三千六"), "3600");
        assert_eq!(arabicize_spoken_number_text("一百零五"), "105");
    }

    #[test]
    fn arabicize_in_sentence_spoken_numerals_skips_idioms() {
        assert_eq!(
            arabicize_spoken_number_text("預算大概三千六塊"),
            "預算大概3600塊"
        );
        assert_eq!(
            arabicize_spoken_number_text("大概五百塊就好"),
            "大概500塊就好"
        );
        assert_eq!(arabicize_spoken_number_text("兩到三次"), "2到3次");
        assert_eq!(
            arabicize_spoken_number_text("會議訂在十一點"),
            "會議訂在11點"
        );
        assert_eq!(
            arabicize_spoken_number_text("第一點是預算，第二點是時程"),
            "第1點是預算，第2點是時程"
        );
        assert_eq!(arabicize_spoken_number_text("一共二十個人"), "一共20個人");
        assert_eq!(
            arabicize_spoken_number_text("萬一失敗就完蛋"),
            "萬一失敗就完蛋"
        );
        assert_eq!(arabicize_spoken_number_text("千萬不要遲到"), "千萬不要遲到");
        assert_eq!(arabicize_spoken_number_text("一模一樣"), "一模一樣");
        assert_eq!(arabicize_spoken_number_text("三心二意"), "三心二意");
        assert_eq!(
            arabicize_spoken_number_text("今天天氣一五八"),
            "今天天氣一五八"
        );
        assert_eq!(arabicize_spoken_number_text("十分好"), "十分好");
        assert_eq!(arabicize_spoken_number_text("一定可以"), "一定可以");
        assert_eq!(arabicize_spoken_number_text("星期一開會"), "星期一開會");
        assert_eq!(arabicize_spoken_number_text("從三到五樓"), "從3到5樓");
    }

    #[test]
    fn timestamp_decimals_still_junk_bare_list_items_kept() {
        assert!(is_timestamp_junk_segment("[0.00]"));
        assert!(is_timestamp_junk_segment("(1.28s)"));
        assert!(!is_timestamp_junk_segment("[1]"));
        assert!(!is_timestamp_junk_segment("(2)"));
        assert_eq!(clean_whisper_tokens("[1] [2] [3]"), "[1] [2] [3]");
        assert_eq!(clean_whisper_tokens("你好[0.00]世界"), "你好世界");
    }
}
