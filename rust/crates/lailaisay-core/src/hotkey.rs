use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;
use std::fmt;

pub const DOUBLE_TAP_THRESHOLD: f64 = 0.3;
pub const PRESS_AND_HOLD_CANCEL_THRESHOLD: f64 = 1.0;
/// Mic start delay so a quick tap can become a double-tap instead of a hold.
pub const RECORD_START_DELAY_SECS: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Modifier {
    Command,
    Option,
    Shift,
    Control,
    Fn,
}

impl Modifier {
    pub fn as_str(self) -> &'static str {
        match self {
            Modifier::Command => "command",
            Modifier::Option => "option",
            Modifier::Shift => "shift",
            Modifier::Control => "control",
            Modifier::Fn => "fn",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "command" | "cmd" | "meta" => Some(Self::Command),
            "option" | "alt" | "alternate" => Some(Self::Option),
            "shift" => Some(Self::Shift),
            "control" | "ctrl" => Some(Self::Control),
            "fn" | "function" => Some(Self::Fn),
            _ => None,
        }
    }
}

/// Persisted modifiers may use `{ "fn": {} }`. We also accept `"fn"`.
impl Serialize for Modifier {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(self.as_str(), &serde_json::json!({}))?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for Modifier {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Modifier;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a modifier string or keyed object like {{\"fn\":{{}}}}")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Modifier, E> {
                Modifier::parse(v).ok_or_else(|| E::unknown_variant(v, VARIANTS))
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Modifier, E> {
                self.visit_str(&v)
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Modifier, A::Error> {
                let key: String = map
                    .next_key()?
                    .ok_or_else(|| serde::de::Error::custom("empty modifier object"))?;
                let _: serde_json::Value = map.next_value()?;
                while map.next_entry::<String, serde_json::Value>()?.is_some() {}
                Modifier::parse(&key)
                    .ok_or_else(|| serde::de::Error::unknown_variant(&key, VARIANTS))
            }
        }
        deserializer.deserialize_any(V)
    }
}

const VARIANTS: &[&str] = &["command", "option", "shift", "control", "fn"];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub modifiers: BTreeSet<Modifier>,
}

impl Modifiers {
    pub fn new(mods: impl IntoIterator<Item = Modifier>) -> Self {
        Self {
            modifiers: mods.into_iter().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.modifiers.is_empty()
    }

    pub fn contains(&self, m: Modifier) -> bool {
        self.modifiers.contains(&m)
    }

    pub fn is_subset_of(&self, other: &Modifiers) -> bool {
        self.modifiers.is_subset(&other.modifiers)
    }

    pub fn is_hyperkey(&self) -> bool {
        self.contains(Modifier::Command)
            && self.contains(Modifier::Option)
            && self.contains(Modifier::Shift)
            && self.contains(Modifier::Control)
    }
}

impl FromIterator<Modifier> for Modifiers {
    fn from_iter<T: IntoIterator<Item = Modifier>>(iter: T) -> Self {
        Self::new(iter)
    }
}

impl Serialize for Modifiers {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Helper<'a> {
            modifiers: &'a BTreeSet<Modifier>,
        }
        Helper {
            modifiers: &self.modifiers,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Modifiers {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Wrapped { modifiers: Vec<Modifier> },
            Flat(Vec<Modifier>),
        }
        Ok(match Helper::deserialize(deserializer)? {
            Helper::Wrapped { modifiers } | Helper::Flat(modifiers) => Self::new(modifiers),
        })
    }
}

/// A subset of Sauce `Key` names used by lailaisay (letters, space, escape, digits).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Space,
    Escape,
    Return,
    Tab,
    Zero,
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Other(String),
}

impl Key {
    pub fn parse(raw: &str) -> Self {
        match raw.to_ascii_lowercase().as_str() {
            "a" => Key::A,
            "b" => Key::B,
            "c" => Key::C,
            "d" => Key::D,
            "e" => Key::E,
            "f" => Key::F,
            "g" => Key::G,
            "h" => Key::H,
            "i" => Key::I,
            "j" => Key::J,
            "k" => Key::K,
            "l" => Key::L,
            "m" => Key::M,
            "n" => Key::N,
            "o" => Key::O,
            "p" => Key::P,
            "q" => Key::Q,
            "r" => Key::R,
            "s" => Key::S,
            "t" => Key::T,
            "u" => Key::U,
            "v" => Key::V,
            "w" => Key::W,
            "x" => Key::X,
            "y" => Key::Y,
            "z" => Key::Z,
            "space" => Key::Space,
            "escape" => Key::Escape,
            "return" | "enter" => Key::Return,
            "tab" => Key::Tab,
            "zero" | "0" => Key::Zero,
            "one" | "1" => Key::One,
            "two" | "2" => Key::Two,
            "three" | "3" => Key::Three,
            "four" | "4" => Key::Four,
            "five" | "5" => Key::Five,
            "six" | "6" => Key::Six,
            "seven" | "7" => Key::Seven,
            "eight" | "8" => Key::Eight,
            "nine" | "9" => Key::Nine,
            other => Key::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Key::A => "a",
            Key::B => "b",
            Key::C => "c",
            Key::D => "d",
            Key::E => "e",
            Key::F => "f",
            Key::G => "g",
            Key::H => "h",
            Key::I => "i",
            Key::J => "j",
            Key::K => "k",
            Key::L => "l",
            Key::M => "m",
            Key::N => "n",
            Key::O => "o",
            Key::P => "p",
            Key::Q => "q",
            Key::R => "r",
            Key::S => "s",
            Key::T => "t",
            Key::U => "u",
            Key::V => "v",
            Key::W => "w",
            Key::X => "x",
            Key::Y => "y",
            Key::Z => "z",
            Key::Space => "space",
            Key::Escape => "escape",
            Key::Return => "return",
            Key::Tab => "tab",
            Key::Zero => "zero",
            Key::One => "one",
            Key::Two => "two",
            Key::Three => "three",
            Key::Four => "four",
            Key::Five => "five",
            Key::Six => "six",
            Key::Seven => "seven",
            Key::Eight => "eight",
            Key::Nine => "nine",
            Key::Other(s) => s.as_str(),
        }
    }
}

impl Serialize for Key {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Key::parse(&s))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotKey {
    /// Absent / null = modifier-only chord.
    #[serde(default)]
    pub key: Option<Key>,
    pub modifiers: Modifiers,
}

impl Default for HotKey {
    fn default() -> Self {
        Self {
            key: Some(Key::Space),
            modifiers: Modifiers::new([Modifier::Command, Modifier::Shift]),
        }
    }
}

impl HotKey {
    /// Speak-to-Edit default: ⌥⇧Space (distinct from the ⌘⇧Space hold-to-talk chord).
    pub fn default_edit() -> Self {
        Self {
            key: Some(Key::Space),
            modifiers: Modifiers::new([Modifier::Option, Modifier::Shift]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Option<Key>,
    pub modifiers: Modifiers,
}

impl KeyEvent {
    pub fn new(key: Option<Key>, modifiers: impl IntoIterator<Item = Modifier>) -> Self {
        Self {
            key,
            modifiers: Modifiers::new(modifiers),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HotKeyState {
    Idle,
    PressAndHold { start_time: f64 },
    DoubleTapLock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotKeyOutput {
    StartRecording,
    StopRecording,
    Cancel,
}

/// Press-and-hold + double-tap lock.
///
/// Time is injected via [`Self::set_now`] so tests can replay hotkey scenarios.
#[derive(Debug, Clone)]
pub struct HotKeyProcessor {
    pub hotkey: HotKey,
    pub use_double_tap_only: bool,
    state: HotKeyState,
    last_tap_at: Option<f64>,
    is_dirty: bool,
    now: f64,
}

impl HotKeyProcessor {
    pub fn new(hotkey: HotKey, use_double_tap_only: bool) -> Self {
        Self {
            hotkey,
            use_double_tap_only,
            state: HotKeyState::Idle,
            last_tap_at: None,
            is_dirty: false,
            now: 0.0,
        }
    }

    pub fn set_now(&mut self, now: f64) {
        self.now = now;
    }

    /// Stamp `now` from the wall clock (menu-bar / event-tap path).
    pub fn stamp_wall_clock(&mut self) {
        use std::time::{SystemTime, UNIX_EPOCH};
        self.now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
    }

    pub fn state(&self) -> HotKeyState {
        self.state
    }

    pub fn is_matched(&self) -> bool {
        !matches!(self.state, HotKeyState::Idle)
    }

    pub fn process(&mut self, event: &KeyEvent) -> Option<HotKeyOutput> {
        if event.key == Some(Key::Escape) && self.state != HotKeyState::Idle {
            self.reset_to_idle();
            return Some(HotKeyOutput::Cancel);
        }

        if self.is_dirty {
            if chord_is_fully_released(event) {
                self.is_dirty = false;
            } else {
                return None;
            }
        }

        if self.chord_matches_hotkey(event) {
            self.handle_matching_chord()
        } else {
            if self.chord_is_dirty(event) {
                self.is_dirty = true;
            }
            self.handle_nonmatching_chord(event)
        }
    }

    fn handle_matching_chord(&mut self) -> Option<HotKeyOutput> {
        match self.state {
            HotKeyState::Idle => {
                if self.use_double_tap_only && self.hotkey.key.is_some() {
                    self.last_tap_at = Some(self.now);
                    None
                } else {
                    self.state = HotKeyState::PressAndHold {
                        start_time: self.now,
                    };
                    Some(HotKeyOutput::StartRecording)
                }
            }
            HotKeyState::PressAndHold { .. } => None,
            HotKeyState::DoubleTapLock => {
                self.reset_to_idle();
                Some(HotKeyOutput::StopRecording)
            }
        }
    }

    fn handle_nonmatching_chord(&mut self, e: &KeyEvent) -> Option<HotKeyOutput> {
        match self.state {
            HotKeyState::Idle => {
                if self.use_double_tap_only
                    && self.hotkey.key.is_some()
                    && chord_is_fully_released(e)
                    && self.last_tap_at.is_some()
                {
                    if let Some(prev) = self.last_tap_at {
                        if self.now - prev < DOUBLE_TAP_THRESHOLD {
                            self.state = HotKeyState::DoubleTapLock;
                            return Some(HotKeyOutput::StartRecording);
                        }
                    }
                    self.last_tap_at = None;
                }
                None
            }
            HotKeyState::PressAndHold { start_time } => {
                if self.is_release_for_active_hotkey(e) {
                    if let Some(prev) = self.last_tap_at {
                        if self.now - prev < DOUBLE_TAP_THRESHOLD {
                            self.state = HotKeyState::DoubleTapLock;
                            return None;
                        }
                    }
                    self.state = HotKeyState::Idle;
                    self.last_tap_at = Some(self.now);
                    Some(HotKeyOutput::StopRecording)
                } else {
                    let elapsed = self.now - start_time;
                    if elapsed < PRESS_AND_HOLD_CANCEL_THRESHOLD {
                        self.is_dirty = true;
                        self.reset_to_idle();
                        Some(HotKeyOutput::StopRecording)
                    } else {
                        None
                    }
                }
            }
            HotKeyState::DoubleTapLock => {
                if self.use_double_tap_only
                    && self.hotkey.key.is_some()
                    && chord_is_fully_released(e)
                {
                    self.reset_to_idle();
                    return Some(HotKeyOutput::StopRecording);
                }
                None
            }
        }
    }

    fn chord_matches_hotkey(&self, e: &KeyEvent) -> bool {
        e.key == self.hotkey.key && e.modifiers == self.hotkey.modifiers
    }

    fn chord_is_dirty(&self, e: &KeyEvent) -> bool {
        let has_extra = !e.modifiers.is_subset_of(&self.hotkey.modifiers);
        let is_wrong_key = self.hotkey.key.is_some() && e.key.is_some() && e.key != self.hotkey.key;
        let key_for_modifier_only =
            self.hotkey.key.is_none() && e.key.is_some() && !e.modifiers.is_empty();
        has_extra || is_wrong_key || key_for_modifier_only
    }

    fn is_release_for_active_hotkey(&self, e: &KeyEvent) -> bool {
        if self.hotkey.key.is_some() {
            e.key.is_none() && e.modifiers == self.hotkey.modifiers
        } else {
            e.key.is_none() && e.modifiers.is_subset_of(&self.hotkey.modifiers)
        }
    }

    fn reset_to_idle(&mut self) {
        self.state = HotKeyState::Idle;
        self.last_tap_at = None;
    }
}

fn chord_is_fully_released(e: &KeyEvent) -> bool {
    e.key.is_none() && e.modifiers.is_empty()
}

impl fmt::Display for HotKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for m in &self.modifiers.modifiers {
            match m {
                Modifier::Command => write!(f, "⌘")?,
                Modifier::Option => write!(f, "⌥")?,
                Modifier::Shift => write!(f, "⇧")?,
                Modifier::Control => write!(f, "⌃")?,
                Modifier::Fn => write!(f, "fn")?,
            }
        }
        if let Some(key) = &self.key {
            write!(f, "{}", key.as_str().to_uppercase())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Step {
        time: f64,
        key: Option<Key>,
        modifiers: Vec<Modifier>,
        expected: Option<HotKeyOutput>,
        matched: Option<bool>,
        state: Option<HotKeyState>,
    }

    fn run(hotkey: HotKey, steps: Vec<Step>) {
        run_with(hotkey, false, steps);
    }

    fn run_with(hotkey: HotKey, double_tap_only: bool, steps: Vec<Step>) {
        let mut processor = HotKeyProcessor::new(hotkey, double_tap_only);
        for step in steps {
            processor.set_now(step.time);
            let event = KeyEvent::new(step.key, step.modifiers);
            let out = processor.process(&event);
            assert_eq!(
                out, step.expected,
                "at {}s: expected {:?}, got {:?}",
                step.time, step.expected, out
            );
            if let Some(exp) = step.matched {
                assert_eq!(
                    processor.is_matched(),
                    exp,
                    "at {}s: expected is_matched={}",
                    step.time,
                    exp
                );
            }
            if let Some(exp) = step.state {
                assert_eq!(
                    processor.state(),
                    exp,
                    "at {}s: expected state {:?}",
                    step.time,
                    exp
                );
            }
        }
    }

    #[test]
    fn record_start_delay_stays_200ms_for_double_tap_cancel() {
        assert!((RECORD_START_DELAY_SECS - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn press_and_hold_starts_on_hotkey_standard() {
        run(
            HotKey {
                key: Some(Key::A),
                modifiers: Modifiers::new([Modifier::Command]),
            },
            vec![Step {
                time: 0.0,
                key: Some(Key::A),
                modifiers: vec![Modifier::Command],
                expected: Some(HotKeyOutput::StartRecording),
                matched: Some(true),
                state: None,
            }],
        );
    }

    #[test]
    fn press_and_hold_starts_on_hotkey_modifier_only() {
        run(
            HotKey {
                key: None,
                modifiers: Modifiers::new([Modifier::Option]),
            },
            vec![Step {
                time: 0.0,
                key: None,
                modifiers: vec![Modifier::Option],
                expected: Some(HotKeyOutput::StartRecording),
                matched: Some(true),
                state: None,
            }],
        );
    }

    #[test]
    fn press_and_hold_stops_on_release_standard() {
        run(
            HotKey {
                key: Some(Key::A),
                modifiers: Modifiers::new([Modifier::Command]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: Some(Key::A),
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.2,
                    key: None,
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StopRecording),
                    matched: Some(false),
                    state: None,
                },
            ],
        );
    }

    #[test]
    fn press_and_hold_stops_on_release_modifier_only() {
        run(
            HotKey {
                key: None,
                modifiers: Modifiers::new([Modifier::Option]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: None,
                    modifiers: vec![Modifier::Option],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.2,
                    key: None,
                    modifiers: vec![],
                    expected: Some(HotKeyOutput::StopRecording),
                    matched: Some(false),
                    state: None,
                },
            ],
        );
    }

    #[test]
    fn press_and_hold_multiple_modifiers() {
        run(
            HotKey {
                key: None,
                modifiers: Modifiers::new([Modifier::Option, Modifier::Command]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: None,
                    modifiers: vec![Modifier::Option],
                    expected: None,
                    matched: Some(false),
                    state: None,
                },
                Step {
                    time: 0.1,
                    key: None,
                    modifiers: vec![Modifier::Option, Modifier::Command],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.2,
                    key: None,
                    modifiers: vec![],
                    expected: Some(HotKeyOutput::StopRecording),
                    matched: Some(false),
                    state: None,
                },
            ],
        );
    }

    #[test]
    fn press_and_hold_cancels_on_other_key() {
        run(
            HotKey {
                key: Some(Key::A),
                modifiers: Modifiers::new([Modifier::Command]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: Some(Key::A),
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.5,
                    key: Some(Key::B),
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StopRecording),
                    matched: Some(false),
                    state: None,
                },
            ],
        );
    }

    #[test]
    fn press_and_hold_does_not_cancel_after_threshold() {
        run(
            HotKey {
                key: Some(Key::A),
                modifiers: Modifiers::new([Modifier::Command]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: Some(Key::A),
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 1.5,
                    key: Some(Key::B),
                    modifiers: vec![Modifier::Command],
                    expected: None,
                    matched: Some(true),
                    state: None,
                },
            ],
        );
    }

    #[test]
    fn press_and_hold_no_backslide() {
        run(
            HotKey {
                key: Some(Key::A),
                modifiers: Modifiers::new([Modifier::Command]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: Some(Key::A),
                    modifiers: vec![Modifier::Command, Modifier::Shift],
                    expected: None,
                    matched: Some(false),
                    state: None,
                },
                Step {
                    time: 0.1,
                    key: Some(Key::A),
                    modifiers: vec![Modifier::Command],
                    expected: None,
                    matched: Some(false),
                    state: None,
                },
                Step {
                    time: 0.2,
                    key: None,
                    modifiers: vec![],
                    expected: None,
                    matched: Some(false),
                    state: None,
                },
                Step {
                    time: 0.3,
                    key: Some(Key::A),
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
            ],
        );
    }

    #[test]
    fn double_tap_lock_standard() {
        run(
            HotKey {
                key: Some(Key::A),
                modifiers: Modifiers::new([Modifier::Command]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: Some(Key::A),
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.1,
                    key: None,
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StopRecording),
                    matched: Some(false),
                    state: None,
                },
                Step {
                    time: 0.1,
                    key: None,
                    modifiers: vec![],
                    expected: None,
                    matched: Some(false),
                    state: None,
                },
                Step {
                    time: 0.15,
                    key: None,
                    modifiers: vec![Modifier::Command],
                    expected: None,
                    matched: Some(false),
                    state: None,
                },
                Step {
                    time: 0.2,
                    key: Some(Key::A),
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.3,
                    key: None,
                    modifiers: vec![Modifier::Command],
                    expected: None,
                    matched: Some(true),
                    state: Some(HotKeyState::DoubleTapLock),
                },
            ],
        );
    }

    #[test]
    fn double_tap_lock_modifier_only() {
        run(
            HotKey {
                key: None,
                modifiers: Modifiers::new([Modifier::Option]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: None,
                    modifiers: vec![Modifier::Option],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.1,
                    key: None,
                    modifiers: vec![],
                    expected: Some(HotKeyOutput::StopRecording),
                    matched: Some(false),
                    state: None,
                },
                Step {
                    time: 0.2,
                    key: None,
                    modifiers: vec![Modifier::Option],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.3,
                    key: None,
                    modifiers: vec![],
                    expected: None,
                    matched: Some(true),
                    state: Some(HotKeyState::DoubleTapLock),
                },
            ],
        );
    }

    #[test]
    fn double_tap_ignores_slow_second_tap() {
        run(
            HotKey {
                key: Some(Key::A),
                modifiers: Modifiers::new([Modifier::Command]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: Some(Key::A),
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.1,
                    key: None,
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StopRecording),
                    matched: Some(false),
                    state: None,
                },
                Step {
                    time: 0.4,
                    key: Some(Key::A),
                    modifiers: vec![Modifier::Command],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
            ],
        );
    }

    #[test]
    fn double_tap_third_tap_stops() {
        run(
            HotKey {
                key: None,
                modifiers: Modifiers::new([Modifier::Option]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: None,
                    modifiers: vec![Modifier::Option],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.1,
                    key: None,
                    modifiers: vec![],
                    expected: Some(HotKeyOutput::StopRecording),
                    matched: Some(false),
                    state: None,
                },
                Step {
                    time: 0.2,
                    key: None,
                    modifiers: vec![Modifier::Option],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.3,
                    key: None,
                    modifiers: vec![],
                    expected: None,
                    matched: Some(true),
                    state: Some(HotKeyState::DoubleTapLock),
                },
                Step {
                    time: 1.0,
                    key: None,
                    modifiers: vec![Modifier::Option],
                    expected: Some(HotKeyOutput::StopRecording),
                    matched: Some(false),
                    state: None,
                },
            ],
        );
    }

    #[test]
    fn stays_dirty_after_accidental_key() {
        run(
            HotKey {
                key: None,
                modifiers: Modifiers::new([Modifier::Option]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: None,
                    modifiers: vec![Modifier::Option],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 0.1,
                    key: Some(Key::C),
                    modifiers: vec![Modifier::Option],
                    expected: Some(HotKeyOutput::StopRecording),
                    matched: Some(false),
                    state: None,
                },
                Step {
                    time: 0.2,
                    key: None,
                    modifiers: vec![Modifier::Option],
                    expected: None,
                    matched: Some(false),
                    state: None,
                },
            ],
        );
    }

    #[test]
    fn stays_matched_after_two_seconds_extra_modifier() {
        run(
            HotKey {
                key: None,
                modifiers: Modifiers::new([Modifier::Option]),
            },
            vec![
                Step {
                    time: 0.0,
                    key: None,
                    modifiers: vec![Modifier::Option],
                    expected: Some(HotKeyOutput::StartRecording),
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 2.0,
                    key: None,
                    modifiers: vec![Modifier::Option, Modifier::Command],
                    expected: None,
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 2.1,
                    key: None,
                    modifiers: vec![Modifier::Option],
                    expected: None,
                    matched: Some(true),
                    state: None,
                },
                Step {
                    time: 2.2,
                    key: None,
                    modifiers: vec![],
                    expected: Some(HotKeyOutput::StopRecording),
                    matched: Some(false),
                    state: None,
                },
            ],
        );
    }

    #[test]
    fn default_hotkey_serde() {
        let hk = HotKey::default();
        let json = serde_json::to_string(&hk).unwrap();
        let back: HotKey = serde_json::from_str(&json).unwrap();
        assert_eq!(hk, back);
        let persisted = r#"{"key":"space","modifiers":{"modifiers":["command","shift"]}}"#;
        let parsed: HotKey = serde_json::from_str(persisted).unwrap();
        assert_eq!(parsed.key, Some(Key::Space));
        assert!(parsed.modifiers.contains(Modifier::Command));
    }

    #[test]
    fn keyed_fn_modifier_deserializes() {
        let raw = r#"{
            "modifiers": {
                "modifiers": [ { "fn": {} } ]
            }
        }"#;
        let hk: HotKey = serde_json::from_str(raw).unwrap();
        assert_eq!(hk.key, None);
        assert!(hk.modifiers.contains(Modifier::Fn));
        assert_eq!(hk.modifiers.modifiers.len(), 1);

        let cmd: Modifier = serde_json::from_str(r#"{"command":{}}"#).unwrap();
        assert_eq!(cmd, Modifier::Command);
        let as_str: Modifier = serde_json::from_str(r#""option""#).unwrap();
        assert_eq!(as_str, Modifier::Option);

        let encoded = serde_json::to_value(Modifier::Fn).unwrap();
        assert_eq!(encoded, serde_json::json!({"fn": {}}));
    }
}
