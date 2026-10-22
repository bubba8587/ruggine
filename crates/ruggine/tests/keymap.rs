// [[B1]] upstreamIsSpec
use ruggine::keymap::is_array_index;
use ruggine::{KeyMap, keymap};

#[test]
fn array_index_keys_are_javascripts() {
    for key in ["0", "1", "42", "4294967294"] {
        assert!(is_array_index(key), "{key}");
    }
    for key in ["", "01", "-1", "1.5", "4294967295", "a", " 1", "1e3"] {
        assert!(!is_array_index(key), "{key}");
    }
}

#[test]
fn iterates_in_property_order() {
    let map: KeyMap<i32> = keymap! { "z" => 1, "10" => 2, "a" => 3, "2" => 4, "01" => 5 };
    assert_eq!(map.key_list(), ["2", "10", "z", "a", "01"]);
}

#[test]
fn overwriting_keeps_the_place_and_removal_closes_the_gap() {
    let mut map: KeyMap<i32> = keymap! { "a" => 1, "b" => 2, "0" => 3 };
    assert_eq!(map.insert("a", 9), Some(1));
    assert_eq!(map.key_list(), ["0", "a", "b"]);
    assert_eq!(map["a"], 9);
    assert_eq!(map.remove("0"), Some(3));
    map.insert("5", 4);
    map.insert("3", 5);
    assert_eq!(map.key_list(), ["3", "5", "a", "b"]);
}
