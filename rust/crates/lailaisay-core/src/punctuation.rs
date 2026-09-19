use crate::chinese::is_chinese_character;
use crate::tokens::{clean_whisper_tokens, is_spoken_number_text, is_timestamp_junk_segment};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptionSegment {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

impl TranscriptionSegment {
    pub fn new(text: impl Into<String>, start: f64, end: f64) -> Self {
        Self {
            text: text.into(),
            start,
            end,
        }
    }
}

const ALL_PUNCT: &[char] = &['。', '，', '？', '！', '；', '：', '、'];

fn forward_binding_connectives() -> &'static [&'static str] {
    &[
        "接下來",
        "但是",
        "然而",
        "可是",
        "不過",
        "所以",
        "因此",
        "因而",
        "因為",
        "由於",
        "而且",
        "並且",
        "況且",
        "另外",
        "此外",
        "同時",
        "雖然",
        "儘管",
        "即使",
        "如果",
        "假如",
        "要是",
        "一旦",
        "只要",
        "除非",
        "否則",
        "不然",
        "然後",
        "接著",
        "或者",
        "或是",
    ]
}

const RELOCATION_ONLY: &[&str] = &["一旦", "只要", "除非", "或是"];

const NON_FINAL_TAILS: &[&str] = &[
    "在於",
    "對於",
    "關於",
    "屬於",
    "等於",
    "位於",
    "就是",
    "而是",
    "像是",
    "甚至是",
    "例如",
    "譬如",
    "比如",
    "包括",
    "以及",
    "加上",
];

const DISCOURSE_OPENERS: &[&str] = &[
    "當然",
    "其實",
    "事實上",
    "總之",
    "簡單來說",
    "坦白說",
    "老實說",
    "說真的",
    "基本上",
    "換句話說",
    "也就是說",
    "總而言之",
    "非常",
    "可以",
    "請",
    "麻煩",
];

const BACKWARD_BINDING_OPENERS: &[&str] = &["還是", "或者", "或是", "而且", "並且", "以及", "甚至"];

const COORDINATING: &[&str] = &["和", "與", "及", "跟", "或", "還有"];

const TEMPORAL_MARKERS: &[&str] = &["目前", "現在", "後來", "剛才", "最近", "當時"];

const DISCOURSE_MARKERS: &[&str] = &["其實", "事實上", "總之", "簡單來說"];

/// Drop timestamp-only segments; keep neighbor t0/t1 so pause gaps stay valid.
pub fn sanitize_transcription_segments(
    segments: &[TranscriptionSegment],
) -> Vec<TranscriptionSegment> {
    segments
        .iter()
        .filter(|s| !is_timestamp_junk_segment(&s.text))
        .map(|s| TranscriptionSegment::new(clean_whisper_tokens(&s.text), s.start, s.end))
        .filter(|s| !s.text.is_empty())
        .collect()
}

/// Insert pause-based punctuation across VAD segments from segment timing.
pub fn punctuated_text(segments: &[TranscriptionSegment]) -> String {
    let owned = sanitize_transcription_segments(segments);
    let segments = owned.as_slice();
    if segments.len() < 2 {
        let plain = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        return normalize_punctuation(&insert_punctuation_at_clause_boundaries(&plain));
    }

    let gaps: Vec<f64> = segments
        .windows(2)
        .map(|w| (w[1].start - w[0].end).max(0.0))
        .collect();
    let mut scale = 1.0;
    if gaps.len() >= 3 {
        let mut sorted = gaps.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        scale = (median / 0.3).clamp(1.0, 3.0);
    }
    let enumeration_threshold = 0.10 * scale;
    let comma_threshold = 0.22 * scale;
    let period_threshold = 0.7 * scale;

    let mut output = String::new();
    for i in 0..segments.len() {
        let text = clean_whisper_tokens(&segments[i].text);
        if text.is_empty() {
            continue;
        }
        output.push_str(&text);

        if i + 1 >= segments.len() {
            continue;
        }
        let gap = segments[i + 1].start - segments[i].end;
        let next_text = clean_whisper_tokens(&segments[i + 1].text);
        let last = text.chars().last().unwrap_or(' ');
        let already = "。，？！；：、.?! ,;:？！，".contains(last);
        if already {
            continue;
        }

        let mut handled = false;
        if gap >= comma_threshold {
            if let Some((body, connective)) = split_trailing_connective(&text) {
                if !body.is_empty() {
                    for _ in 0..connective.chars().count() {
                        output.pop();
                    }
                    let punct = if gap >= period_threshold {
                        determine_sentence_end_punctuation(&body)
                    } else {
                        "，".into()
                    };
                    output.push_str(&punct);
                    output.push_str(&connective);
                }
                handled = true;
            } else if ends_with_non_final_tail(&text) || starts_with_backward_binding(&next_text) {
                output.push('，');
                handled = true;
            } else if gap >= period_threshold {
                output.push_str(&determine_sentence_end_punctuation(&text));
                handled = true;
            } else if !starts_with_coordinating(&next_text) {
                output.push_str(&determine_clause_punctuation(&text, &next_text));
                handled = true;
            } else {
                handled = true;
            }
        } else if gap >= enumeration_threshold
            && split_trailing_connective(&text).is_none()
            && looks_like_enumeration(&text, &next_text)
            && !starts_with_coordinating(&next_text)
        {
            output.push('、');
            handled = true;
        }

        if !handled && starts_with_discourse_opener(&next_text) {
            output.push('，');
            handled = true;
        }
        if !handled && is_spoken_number_text(&text) && is_spoken_number_text(&next_text) {
            output.push(' ');
        }
    }

    let output = insert_punctuation_at_clause_boundaries(&output);
    normalize_punctuation(&output)
}

fn split_trailing_connective(text: &str) -> Option<(String, String)> {
    for marker in forward_binding_connectives() {
        if text.ends_with(marker) {
            let body = text[..text.len() - marker.len()].to_string();
            return Some((body, (*marker).to_string()));
        }
    }
    None
}

fn ends_with_non_final_tail(text: &str) -> bool {
    NON_FINAL_TAILS.iter().any(|t| text.ends_with(t))
}

fn starts_with_discourse_opener(text: &str) -> bool {
    DISCOURSE_OPENERS.iter().any(|t| text.starts_with(t))
}

fn starts_with_backward_binding(text: &str) -> bool {
    BACKWARD_BINDING_OPENERS.iter().any(|t| text.starts_with(t))
}

fn starts_with_coordinating(text: &str) -> bool {
    COORDINATING.iter().any(|t| text.starts_with(t))
}

fn looks_like_enumeration(text: &str, next: &str) -> bool {
    let cur = text.chars().filter(|c| is_chinese_character(*c)).count();
    let nxt = next.chars().filter(|c| is_chinese_character(*c)).count();
    (1..=5).contains(&cur) && (1..=5).contains(&nxt)
}

fn determine_sentence_end_punctuation(text: &str) -> String {
    let trimmed = text.trim();
    if is_question_pattern(trimmed) {
        "？".into()
    } else if is_exclamatory_pattern(trimmed) {
        "！".into()
    } else {
        "。".into()
    }
}

fn determine_clause_punctuation(text: &str, next: &str) -> String {
    if looks_like_enumeration(text, next) {
        "、".into()
    } else {
        "，".into()
    }
}

fn is_question_pattern(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    for marker in ["難道", "豈不", "豈能", "何必", "何嘗"] {
        if text.contains(marker) {
            return true;
        }
    }
    for ending in ["嗎", "呢", "麼", "嘛"] {
        if text.ends_with(ending) {
            return true;
        }
    }
    for pred in [
        "知道", "曉得", "清楚", "記得", "明白", "瞭解", "了解", "告訴", "好奇", "關心", "取決",
        "問",
    ] {
        if text.contains(pred) {
            return false;
        }
    }
    for pattern in [
        "是不是",
        "有沒有",
        "能不能",
        "可不可以",
        "會不會",
        "要不要",
        "對不對",
        "好不好",
        "行不行",
        "願不願意",
        "算不算",
        "夠不夠",
        "想不想",
    ] {
        if text.contains(pattern) {
            return true;
        }
    }
    if let Some(idx) = text.find("還是") {
        let after = idx + "還是".len();
        if let Some(ch) = text[after..].chars().next() {
            const ADVERB: &[char] = &[
                '有', '很', '不', '沒', '會', '要', '得', '比', '能', '可', '應', '該', '算', '蠻',
                '挺', '滿',
            ];
            if !ADVERB.contains(&ch) {
                return true;
            }
        }
    }
    for word in [
        "多少", "幾個", "幾天", "幾次", "什麼", "怎樣", "如何", "哪裡",
    ] {
        if text.ends_with(word) {
            return true;
        }
    }
    false
}

fn is_exclamatory_pattern(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    for ending in ["死了", "極了", "透了", "壞了", "慘了", "爆了", "翻了"] {
        if text.ends_with(ending) {
            return true;
        }
    }
    for ending in ["耶", "哇", "噢", "唷", "欸", "咧"] {
        if text.ends_with(ending) {
            return true;
        }
    }
    for kw in [
        "救命",
        "天啊",
        "我的天",
        "太棒了",
        "太好了",
        "太厲害",
        "太扯",
        "夠了",
        "閉嘴",
        "不要啊",
        "好痛",
        "好可怕",
    ] {
        if text.contains(kw) {
            return true;
        }
    }
    for starter in [
        "別", "不要", "不准", "不許", "快", "趕快", "馬上", "立刻", "給我",
    ] {
        if text.starts_with(starter) {
            return true;
        }
    }
    if let Some(last) = text.chars().last() {
        if "啊呀哪".contains(last) {
            for adv in ["好", "真", "太", "多麼"] {
                if text.contains(adv) {
                    return true;
                }
            }
        }
    }
    false
}

pub fn insert_punctuation_at_clause_boundaries(text: &str) -> String {
    if text.is_empty() || !text.chars().any(is_chinese_character) {
        return text.to_string();
    }
    let mut markers: Vec<&str> = forward_binding_connectives()
        .iter()
        .copied()
        .filter(|m| !RELOCATION_ONLY.contains(m))
        .chain(TEMPORAL_MARKERS.iter().copied())
        .chain(DISCOURSE_MARKERS.iter().copied())
        .collect();
    markers.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()));

    let mut result = text.to_string();
    for marker in markers {
        let mut search = 0usize;
        while search < result.len() {
            let Some(rel) = result[search..].find(marker) else {
                break;
            };
            let abs = search + rel;
            if abs == 0 {
                search = abs + marker.len();
                continue;
            }
            let char_before = result[..abs].chars().last().unwrap();
            if ALL_PUNCT.contains(&char_before) {
                search = abs + marker.len();
                continue;
            }
            if forward_binding_connectives()
                .iter()
                .any(|c| result[..abs].ends_with(c))
            {
                search = abs + marker.len();
                continue;
            }
            if TEMPORAL_MARKERS.contains(&marker) {
                if result[abs + marker.len()..].starts_with('的') {
                    search = abs + marker.len();
                    continue;
                }
                if "的在從到比是了".contains(char_before) {
                    search = abs + marker.len();
                    continue;
                }
            }
            result.insert(abs, '，');
            search = abs + '，'.len_utf8() + marker.len();
        }
    }
    let result = insert_commas_after_evaluations(&result);
    let result = insert_commas_before_topic_shifts(&result);
    insert_activity_enumeration_marks(&result)
}

/// `很好非常適合` / `不錯可以去` — appraisal then a new clause, no VAD gap.
fn insert_commas_after_evaluations(text: &str) -> String {
    const TAILS: &[&str] = &[
        "很好", "不錯", "不好", "還好", "太好", "真好", "挺好", "蠻好", "開心", "高興",
    ];
    const HEADS: &[&str] = &[
        "非常", "可以", "應該", "真的", "其實", "而且", "所以", "特別", "尤其",
    ];
    let mut result = text.to_string();
    for tail in TAILS {
        for head in HEADS {
            let pat = format!("{tail}{head}");
            let with = format!("{tail}，{head}");
            result = result.replace(&pat, &with);
        }
    }
    result
}

/// `出去走走心情不錯` → comma before a new topic in the same segment.
fn insert_commas_before_topic_shifts(text: &str) -> String {
    const STARTERS: &[&str] = &["心情", "感覺", "我覺得"];
    let skip_before = "的這那好沒不";
    let mut result = text.to_string();
    for starter in STARTERS {
        let mut search = 0usize;
        while search < result.len() {
            let Some(rel) = result[search..].find(starter) else {
                break;
            };
            let abs = search + rel;
            if abs == 0 {
                search = abs + starter.len();
                continue;
            }
            let char_before = result[..abs].chars().last().unwrap();
            if ALL_PUNCT.contains(&char_before) || skip_before.contains(char_before) {
                search = abs + starter.len();
                continue;
            }
            if !is_chinese_character(char_before) {
                search = abs + starter.len();
                continue;
            }
            result.insert(abs, '，');
            search = abs + '，'.len_utf8() + starter.len();
        }
    }
    result
}

const ACTIVITY_VERBS: &[char] = &[
    '看', '打', '吃', '喝', '玩', '聽', '讀', '騎', '游', '唱', '跳', '買', '逛', '踢', '釣', '滑',
    '跑',
];
const LOCATION_TAILS: &[char] = &['邊', '上', '裡', '里', '下', '旁', '前', '後', '外'];

/// `看電影打籃球河邊騎腳踏車` → `看電影、打籃球、河邊騎腳踏車`.
fn insert_activity_enumeration_marks(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 4 {
        return text.to_string();
    }
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some(end) = match_activity_span(&chars, i) {
            spans.push((i, end));
            i = end;
        } else {
            i += 1;
        }
    }
    if spans.len() < 2 {
        return text.to_string();
    }
    let mut insert_at = Vec::new();
    for w in spans.windows(2) {
        if w[0].1 != w[1].0 {
            continue;
        }
        let a: String = chars[w[0].0..w[0].1].iter().collect();
        let b: String = chars[w[1].0..w[1].1].iter().collect();
        // Same activity restated (`游泳池` / `游泳`) — not a new list item.
        if a.contains(&b) || b.contains(&a) {
            continue;
        }
        insert_at.push(w[1].0);
    }
    if insert_at.is_empty() {
        return text.to_string();
    }
    let mut out = String::new();
    for (idx, ch) in chars.iter().enumerate() {
        if insert_at.contains(&idx) {
            out.push('、');
        }
        out.push(*ch);
    }
    out
}

fn match_activity_span(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    if i + 1 < chars.len()
        && is_chinese_character(chars[i])
        && LOCATION_TAILS.contains(&chars[i + 1])
    {
        i += 2;
    }
    if i >= chars.len() || !ACTIVITY_VERBS.contains(&chars[i]) {
        return None;
    }
    i += 1;
    let obj_start = i;
    while i < chars.len()
        && i - obj_start < 3
        && is_chinese_character(chars[i])
        && !ALL_PUNCT.contains(&chars[i])
        && !ACTIVITY_VERBS.contains(&chars[i])
        && !looks_like_activity_start(chars, i)
    {
        i += 1;
    }
    if i == obj_start {
        return None;
    }
    Some(i)
}

fn looks_like_activity_start(chars: &[char], i: usize) -> bool {
    if i + 2 < chars.len()
        && is_chinese_character(chars[i])
        && LOCATION_TAILS.contains(&chars[i + 1])
        && ACTIVITY_VERBS.contains(&chars[i + 2])
    {
        return true;
    }
    i + 1 < chars.len()
        && ACTIVITY_VERBS.contains(&chars[i])
        && is_chinese_character(chars[i + 1])
        && !ACTIVITY_VERBS.contains(&chars[i + 1])
}

pub fn normalize_punctuation(text: &str) -> String {
    let mut result = text.trim().replace('\u{FFFD}', "");
    if result.is_empty() {
        return result;
    }
    if result.chars().any(is_chinese_character) {
        for (half, full) in [
            ('?', '？'),
            ('!', '！'),
            (',', '，'),
            (';', '；'),
            (':', '：'),
        ] {
            result = result.replace(half, &full.to_string());
        }
    }

    static COLLAPSE: OnceLock<Regex> = OnceLock::new();
    let collapse = COLLAPSE
        .get_or_init(|| Regex::new(r"([。，？！；：、])\s+([。，？！；：、])").expect("collapse"));
    loop {
        let next = collapse.replace_all(&result, "$1$2").into_owned();
        if next == result {
            break;
        }
        result = next;
    }

    result = collapse_repeated_punctuation(&result);

    if let Some(last) = result.chars().last() {
        if !ALL_PUNCT.contains(&last) {
            if is_spoken_number_text(&result) {
                // Digit / numeral lists are not sentences. A forced 。 makes
                // 辨識生稿 look like the utterance was dropped or rewritten.
            } else if is_question_pattern(&result) {
                result.push('？');
            } else if is_exclamatory_pattern(&result) {
                result.push('！');
            } else {
                result.push('。');
            }
        }
    }
    result
}

/// Drop runs of `。，？！` left after hallucination/filler stripping.
pub fn collapse_repeated_punctuation(text: &str) -> String {
    let mut cleaned = String::new();
    let mut last_punct = false;
    for ch in text.chars() {
        if ALL_PUNCT.contains(&ch) {
            if !last_punct {
                cleaned.push(ch);
            }
            last_punct = true;
        } else {
            cleaned.push(ch);
            last_punct = false;
        }
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_after_connective_relocates() {
        let segs = [
            TranscriptionSegment::new("真人口說的語速會有快有慢一旦", 0.0, 2.0),
            TranscriptionSegment::new("如果說的比較慢系統會誤判", 2.5, 5.0),
        ];
        let output = punctuated_text(&segs);
        assert!(!output.contains("一旦，"), "{output}");
        assert!(output.contains("，一旦"), "{output}");
    }

    #[test]
    fn connective_only_segment_no_punct_after() {
        let segs = [
            TranscriptionSegment::new("然後", 0.0, 0.5),
            TranscriptionSegment::new("我們就回家了", 1.5, 3.0),
        ];
        let output = punctuated_text(&segs);
        assert!(!output.contains("然後。"), "{output}");
        assert!(!output.contains("然後，"), "{output}");
    }

    #[test]
    fn slow_speech_uniform_gaps_commas() {
        let segs = [
            TranscriptionSegment::new("我今天早上去學校", 0.0, 2.0),
            TranscriptionSegment::new("參加了一場重要的會議", 3.0, 5.0),
            TranscriptionSegment::new("討論了很多事情", 6.0, 8.0),
            TranscriptionSegment::new("接著就回家了", 9.0, 10.0),
        ];
        let output = punctuated_text(&segs);
        let interior = output.chars().rev().skip(1).filter(|c| *c == '。').count();
        assert_eq!(interior, 0, "{output}");
        assert!(output.contains('，'), "{output}");
    }

    #[test]
    fn normal_speech_long_pause_period() {
        let segs = [
            TranscriptionSegment::new("今天天氣很好", 0.0, 1.5),
            TranscriptionSegment::new("我們出去玩吧", 2.5, 4.0),
        ];
        let output = punctuated_text(&segs);
        assert!(output.contains("今天天氣很好。"), "{output}");
    }

    #[test]
    fn normalize_adds_period_and_fullwidth() {
        assert_eq!(normalize_punctuation("今天天氣很好"), "今天天氣很好。");
        assert_eq!(normalize_punctuation("你好吗?"), "你好吗？");
    }

    #[test]
    fn spoken_digit_segments_survive_sanitize() {
        let segs = [
            TranscriptionSegment::new("1", 0.0, 0.15),
            TranscriptionSegment::new("2", 0.2, 0.35),
            TranscriptionSegment::new("3", 0.4, 0.55),
            TranscriptionSegment::new("4", 0.6, 0.75),
            TranscriptionSegment::new("5", 0.8, 0.95),
            TranscriptionSegment::new("6", 1.0, 1.15),
        ];
        let cleaned = sanitize_transcription_segments(&segs);
        assert_eq!(cleaned.len(), 6, "{cleaned:?}");
        assert_eq!(
            cleaned
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            "1 2 3 4 5 6"
        );
        let joined = punctuated_text(&segs);
        assert!(joined.contains('1') && joined.contains('6'), "{joined}");
        assert!(!joined.contains('１'), "{joined}");
    }

    #[test]
    fn chinese_numerals_and_digit_lists_survive_sanitize() {
        let segs = [
            TranscriptionSegment::new("一二三四五六", 0.0, 1.0),
            TranscriptionSegment::new("1,2,3", 1.2, 1.8),
            TranscriptionSegment::new("123456", 2.0, 2.5),
        ];
        let cleaned = sanitize_transcription_segments(&segs);
        assert_eq!(cleaned.len(), 3, "{cleaned:?}");
        assert_eq!(cleaned[0].text, "一二三四五六");
        assert_eq!(cleaned[1].text, "1,2,3");
        assert_eq!(cleaned[2].text, "123456");
    }

    #[test]
    fn number_only_lists_do_not_get_forced_period() {
        assert_eq!(normalize_punctuation("一二三四五六七"), "一二三四五六七");
        assert_eq!(normalize_punctuation("1,2,3,4,5,6,7"), "1,2,3,4,5,6,7");
        assert_eq!(normalize_punctuation("一、二、三"), "一、二、三");
        assert_eq!(normalize_punctuation("今天天氣很好"), "今天天氣很好。");
    }

    #[test]
    fn continuous_chinese_gets_clause_and_enumeration_punct() {
        let raw = "今天天氣很好非常適合帶家人出去走走心情不錯可以去考慮看電影打籃球河邊騎腳踏車";
        let out = normalize_punctuation(&insert_punctuation_at_clause_boundaries(raw));
        assert!(out.contains("很好，"), "{out}");
        assert!(out.contains("不錯，"), "{out}");
        assert!(out.contains("看電影、"), "{out}");
        assert!(out.contains("打籃球、"), "{out}");
        assert!(out.contains("河邊騎腳踏車"), "{out}");
        assert_eq!(
            insert_punctuation_at_clause_boundaries("氣溫非常高"),
            "氣溫非常高"
        );
        assert_eq!(
            insert_punctuation_at_clause_boundaries("三、去游泳池游泳"),
            "三、去游泳池游泳"
        );
    }

    #[test]
    fn continuous_chinese_single_segment_punctuated_text() {
        let segs = [TranscriptionSegment::new(
            "今天天氣很好非常適合帶家人出去走走。心情不錯可以去考慮看電影打籃球河邊騎腳踏車",
            0.0,
            8.0,
        )];
        let out = punctuated_text(&segs);
        assert!(out.contains("很好，非常"), "{out}");
        assert!(out.contains("不錯，可以"), "{out}");
        assert!(out.contains("看電影、打籃球、河邊騎腳踏車"), "{out}");
    }

    #[test]
    fn timestamp_junk_segments_dropped_pause_punct_kept() {
        let segs = [
            TranscriptionSegment::new("今天天氣很好<|1.00|>", 0.0, 1.5),
            TranscriptionSegment::new("<|0.00|>", 1.55, 1.6),
            TranscriptionSegment::new("<|1.60|>", 1.6, 1.7),
            TranscriptionSegment::new("很適合出去走一走", 2.5, 4.0),
        ];
        let output = punctuated_text(&segs);
        assert!(!output.contains('１'), "{output}");
        assert!(
            output.contains("今天天氣很好。") || output.contains('，'),
            "{output}"
        );
    }

    #[test]
    fn collapse_repeated_periods_after_strip() {
        assert_eq!(collapse_repeated_punctuation("去台南。。"), "去台南。");
        assert_eq!(collapse_repeated_punctuation("你好，，世界"), "你好，世界");
    }
}
