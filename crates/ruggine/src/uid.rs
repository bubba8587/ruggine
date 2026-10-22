// [[B1]] upstreamIsSpec, [[B2]] browserAndDesktop
//! Upstream's `getUID`.

use std::cell::RefCell;

type Source = Box<dyn FnMut(&mut [u8; 8])>;

thread_local! {
    static SOURCE: RefCell<Option<Source>> = const { RefCell::new(None) };
}

/// Eight random bytes as sixteen lowercase hex characters.
pub fn get_uid() -> String {
    let mut bytes = [0u8; 8];
    SOURCE.with(|source| match source.borrow_mut().as_mut() {
        Some(fill) => fill(&mut bytes),
        None => getrandom::fill(&mut bytes).expect("no secure random source on this platform"),
    });
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Replaces this thread's random source; `None` restores the platform's.
pub fn set_random_source(source: Option<Source>) {
    SOURCE.with(|s| *s.borrow_mut() = source);
}
