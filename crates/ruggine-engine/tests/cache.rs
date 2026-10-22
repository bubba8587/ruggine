// [[B1]] upstreamIsSpec: rete-engine/test/utils/cache.test.ts
use std::cell::RefCell;
use std::rc::Rc;

use ruggine_engine::{Cache, Error};

fn cache() -> Cache<String, i32> {
    Cache::new()
}

#[test]
fn adds_and_gets_an_item() {
    let c = cache();
    c.add("key1".into(), 1).unwrap();
    assert_eq!(c.get(&"key1".into()), Some(1));
}

#[test]
fn refuses_a_duplicate_key() {
    let c = cache();
    c.add("key1".into(), 1).unwrap();
    let err = c.add("key1".into(), 2).unwrap_err();
    assert_eq!(err, Error::CacheExists);
    assert_eq!(err.to_string(), "cache already exists");
}

#[test]
fn patches_an_item() {
    let c = cache();
    c.add("key1".into(), 1).unwrap();
    c.patch("key1".into(), 2);
    assert_eq!(c.get(&"key1".into()), Some(2));
}

#[test]
fn deletes_an_item() {
    let c = cache();
    c.add("key1".into(), 1).unwrap();
    c.delete(&"key1".into());
    assert_eq!(c.get(&"key1".into()), None);
}

#[test]
fn calls_on_delete_with_the_item() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let c = Cache::<String, i32>::with_on_delete(move |item| log.borrow_mut().push(item));
    c.add("key1".into(), 1).unwrap();
    c.delete(&"key1".into());
    c.delete(&"missing".into());
    assert_eq!(*seen.borrow(), [Some(1), None]);
}

#[test]
fn clears_every_item() {
    let c = cache();
    c.add("key1".into(), 1).unwrap();
    c.add("key2".into(), 2).unwrap();
    c.clear();
    assert_eq!(c.get(&"key1".into()), None);
    assert_eq!(c.get(&"key2".into()), None);
}
