// [[B1]] upstreamIsSpec, [[B2]] browserAndDesktop
//! Upstream's `getUID`.

use std::cell::RefCell;

type Source = Box<dyn FnMut(&mut [u8; 8])>;

thread_local! {
    static SOURCE: RefCell<Option<Source>> = const { RefCell::new(None) };
    static POOL: RefCell<Pool> = const { RefCell::new(Pool { bytes: [0; 512], used: 512 }) };
}

/// Secure random bytes drawn from the platform in batches: one call to the platform per 64 ids
/// instead of one per id.
struct Pool {
    bytes: [u8; 512],
    used: usize,
}

impl Pool {
    fn take(&mut self) -> [u8; 8] {
        if self.used + 8 > self.bytes.len() {
            getrandom::fill(&mut self.bytes).expect("no secure random source on this platform");
            self.used = 0;
        }
        let mut out = [0; 8];
        out.copy_from_slice(&self.bytes[self.used..self.used + 8]);
        self.used += 8;
        out
    }
}

/// Eight random bytes as sixteen lowercase hex characters.
pub fn get_uid() -> String {
    let mut bytes = [0u8; 8];
    SOURCE.with(|source| match source.borrow_mut().as_mut() {
        Some(fill) => fill(&mut bytes),
        None => bytes = POOL.with(|pool| pool.borrow_mut().take()),
    });
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(16);
    for b in bytes {
        out.push(HEX[usize::from(b >> 4)] as char);
        out.push(HEX[usize::from(b & 15)] as char);
    }
    out
}

/// Replaces this thread's random source; `None` restores the platform's.
pub fn set_random_source(source: Option<Source>) {
    SOURCE.with(|s| *s.borrow_mut() = source);
}
