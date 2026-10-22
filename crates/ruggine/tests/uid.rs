// [[B1]] upstreamIsSpec: rete/test/utils.test.ts
use ruggine::get_uid;
use ruggine::uid::set_random_source;

#[test]
fn returns_sixteen_hex_characters() {
    let uid = get_uid();
    assert_eq!(uid.len(), 16);
    assert!(
        uid.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
}

#[test]
fn ids_differ() {
    assert_ne!(get_uid(), get_uid());
}

#[test]
fn reads_the_random_source() {
    set_random_source(Some(Box::new(|bytes| *bytes = [1, 2, 3, 4, 5, 6, 7, 8])));
    assert_eq!(get_uid(), "0102030405060708");
    set_random_source(None);
    assert_ne!(get_uid(), "0102030405060708");
}
