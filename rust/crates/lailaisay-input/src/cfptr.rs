//! Bit-pattern stand-in for a Core Foundation / Core Graphics pointer.
//!
//! Raw `*mut c_void` is neither [`Send`] nor [`Sync`], so storing a tap port or
//! run-loop ref in `Arc<Mutex<_>>` cannot cross onto the CGEvent tap thread.
//! The integer address is `Send`+`Sync`; CF objects are themselves refcounted.
//! Callers still serialize access (mutex + single run-loop thread).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CfPtr(usize);

impl CfPtr {
    pub const NULL: Self = Self(0);

    pub fn from_ptr<T>(p: *mut T) -> Self {
        Self(p as usize)
    }

    pub fn as_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn cfptr_is_send_sync_and_mutex_can_move_to_a_thread() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<CfPtr>();
        assert_sync::<CfPtr>();
        assert_send::<Arc<Mutex<CfPtr>>>();
        assert_sync::<Arc<Mutex<CfPtr>>>();

        let slot = Arc::new(Mutex::new(CfPtr::NULL));
        let slot2 = slot.clone();
        std::thread::spawn(move || {
            drop(slot2.lock());
        })
        .join()
        .expect("cfptr thread");
    }
}
