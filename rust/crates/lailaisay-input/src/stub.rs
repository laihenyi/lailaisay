use crate::{HotkeyMonitor, InputError, Result};
use lailaisay_core::KeyEvent;

pub struct UnsupportedMonitor;

impl HotkeyMonitor for UnsupportedMonitor {
    fn next_event(&mut self) -> Result<Option<KeyEvent>> {
        Err(InputError::UnsupportedOs(std::env::consts::OS))
    }
}
