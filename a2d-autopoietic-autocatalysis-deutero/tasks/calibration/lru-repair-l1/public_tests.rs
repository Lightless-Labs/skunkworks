#[test]
fn public_insert_and_lookup() {
    let mut c = LruCache::new(3);
    assert_eq!(c.len(), 0);
    assert_eq!(c.put(1, 10), None);
    assert_eq!(c.get(1), Some(10));
    assert_eq!(c.get(7), None);
    assert_eq!(c.len(), 1);
}

#[test]
fn public_hit_promotes() {
    let mut c = LruCache::new(3);
    c.put(1, 10); c.put(2, 20); c.put(3, 30);
    assert_eq!(c.get(1), Some(10));
    assert_eq!(c.keys_lru(), vec![2, 3, 1]);
}

#[test]
fn public_evicts_oldest() {
    let mut c = LruCache::new(2);
    c.put(1, 10); c.put(2, 20); c.put(3, 30);
    assert_eq!(c.get(1), None);
    assert_eq!(c.keys_lru(), vec![2, 3]);
}

#[test]
fn public_replacement_value() {
    let mut c = LruCache::new(2);
    assert_eq!(c.put(4, 40), None);
    assert_eq!(c.put(4, 41), Some(40));
    assert_eq!(c.get(4), Some(41));
    assert_eq!(c.len(), 1);
}
