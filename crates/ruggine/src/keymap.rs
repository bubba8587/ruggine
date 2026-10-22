// [[B1]] upstreamIsSpec
//! The stand-in for upstream's name-keyed plain objects: iterates in JavaScript's property
//! order (canonical array-index keys ascending, then insertion order).

use std::borrow::Borrow;
use std::fmt;

#[derive(Clone, PartialEq)]
pub struct KeyMap<V> {
    entries: Vec<(String, V)>,
    index_keys: usize,
}

/// Whether a JavaScript object would list `key` first: `"0"`, `"42"`, below 2^32 - 1, no
/// leading zero.
pub fn is_array_index(key: &str) -> bool {
    if key.is_empty() || key.len() > 10 || !key.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if key.len() > 1 && key.starts_with('0') {
        return false;
    }
    key.parse::<u64>().is_ok_and(|n| n < u32::MAX as u64)
}

fn index_value(key: &str) -> u64 {
    key.parse().unwrap_or(u64::MAX)
}

impl<V> KeyMap<V> {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            index_keys: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|(k, _)| k == key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.position(key).is_some()
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.position(key).map(|i| &self.entries[i].1)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.position(key).map(|i| &mut self.entries[i].1)
    }

    /// Sets `key`. An existing key keeps its place and the old value is returned; a new key
    /// takes the place JavaScript would give it.
    pub fn insert(&mut self, key: impl Into<String>, value: V) -> Option<V> {
        let key = key.into();
        if let Some(i) = self.position(&key) {
            return Some(std::mem::replace(&mut self.entries[i].1, value));
        }
        if is_array_index(&key) {
            let n = index_value(&key);
            let at = self.entries[..self.index_keys]
                .iter()
                .position(|(k, _)| index_value(k) > n)
                .unwrap_or(self.index_keys);
            self.entries.insert(at, (key, value));
            self.index_keys += 1;
        } else {
            self.entries.push((key, value));
        }
        None
    }

    pub fn remove(&mut self, key: &str) -> Option<V> {
        let i = self.position(key)?;
        if i < self.index_keys {
            self.index_keys -= 1;
        }
        Some(self.entries.remove(i).1)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Upstream's `Object.keys`.
    pub fn key_list(&self) -> Vec<String> {
        self.keys().map(str::to_owned).collect()
    }
}

impl<V> Default for KeyMap<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: fmt::Debug> fmt::Debug for KeyMap<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: Into<String>, V> FromIterator<(K, V)> for KeyMap<V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = Self::new();
        for (k, v) in iter {
            map.insert(k, v);
        }
        map
    }
}

impl<V> IntoIterator for KeyMap<V> {
    type Item = (String, V);
    type IntoIter = std::vec::IntoIter<(String, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<Q: Borrow<str> + ?Sized, V> std::ops::Index<&Q> for KeyMap<V> {
    type Output = V;

    fn index(&self, key: &Q) -> &V {
        let key = key.borrow();
        self.get(key)
            .unwrap_or_else(|| panic!("no key '{key}' in KeyMap"))
    }
}

/// Builds a [`KeyMap`] from `key => value` pairs.
#[macro_export]
macro_rules! keymap {
    () => { $crate::KeyMap::new() };
    ($($key:expr => $value:expr),+ $(,)?) => {{
        let mut map = $crate::KeyMap::new();
        $( map.insert($key, $value); )+
        map
    }};
}
