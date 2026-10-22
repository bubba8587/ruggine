// [[B1]]
//! Upstream's `Cache`: a map that tells its owner about every deletion.

use ruggine::fxhash::FxHashMap as HashMap;
use std::cell::RefCell;
use std::hash::Hash;

use crate::Error;

type OnDelete<T> = Box<dyn Fn(Option<T>)>;

pub struct Cache<K, T> {
    items: RefCell<HashMap<K, T>>,
    on_delete: Option<OnDelete<T>>,
}

impl<K: Eq + Hash + Clone, T: Clone> Cache<K, T> {
    pub fn new() -> Self {
        Self {
            items: RefCell::new(HashMap::default()),
            on_delete: None,
        }
    }

    /// `on_delete` gets the deleted item, or `None` when the key held nothing.
    pub fn with_on_delete(on_delete: impl Fn(Option<T>) + 'static) -> Self {
        Self {
            items: RefCell::new(HashMap::default()),
            on_delete: Some(Box::new(on_delete)),
        }
    }

    pub fn get(&self, key: &K) -> Option<T> {
        self.items.borrow().get(key).cloned()
    }

    pub fn add(&self, key: K, data: T) -> Result<(), Error> {
        let mut items = self.items.borrow_mut();
        if items.contains_key(&key) {
            return Err(Error::CacheExists);
        }
        items.insert(key, data);
        Ok(())
    }

    pub fn patch(&self, key: K, data: T) {
        self.items.borrow_mut().insert(key, data);
    }

    pub fn delete(&self, key: &K) {
        let item = self.items.borrow_mut().remove(key);
        if let Some(on_delete) = &self.on_delete {
            on_delete(item);
        }
    }

    pub fn clear(&self) {
        let keys: Vec<K> = self.items.borrow().keys().cloned().collect();
        for key in keys {
            self.delete(&key);
        }
    }
}

impl<K: Eq + Hash + Clone, T: Clone> Default for Cache<K, T> {
    fn default() -> Self {
        Self::new()
    }
}
