#[test]
fn public_timed_insert_and_observe() {
    let mut c = LruCache::new(10);
    assert_eq!(c.len(0), 0);
    assert_eq!(c.total_weight(0), 0);
    assert_eq!(c.put(1, 10, 3, None, 1), Ok(None));
    assert_eq!(c.put(2, 20, 4, Some(20), 1), Ok(None));
    assert_eq!(c.total_weight(2), 7);
    assert_eq!(c.len(2), 2);
    assert_eq!(c.keys_lru(2), vec![1, 2]);
}

#[test]
fn public_timed_reads_and_eviction() {
    let mut c = LruCache::new(8);
    c.put(3, 30, 3, None, 0).unwrap(); c.put(4, 40, 3, None, 0).unwrap();
    assert_eq!(c.peek(3, 1), Some(30));
    assert_eq!(c.keys_lru(1), vec![3, 4]);
    assert_eq!(c.get(3, 1), Some(30));
    c.put(5, 50, 4, None, 1).unwrap();
    assert_eq!(c.keys_lru(1), vec![3, 5]);
    assert_eq!(c.get(4, 1), None);
}

#[test]
fn public_timed_update_remove_and_error() {
    let mut c = LruCache::new(9);
    c.put(6, 60, 2, None, 0).unwrap(); c.put(7, 70, 3, None, 0).unwrap();
    assert_eq!(c.put(6, 61, 4, None, 2), Ok(Some(60)));
    assert_eq!(c.keys_lru(2), vec![7, 6]);
    assert_eq!(c.remove(7, 2), Some(70));
    assert_eq!(c.remove(7, 2), None);
    assert_eq!(c.total_weight(2), 4);
    assert_eq!(c.put(8, 80, 10, None, 2), Err(PutError::TooHeavy { weight: 10, capacity: 9 }));
    assert_eq!(c.keys_lru(2), vec![6]);
}

#[test]
fn public_finite_lifetime() {
    let mut c = LruCache::new(5);
    c.put(9, 90, 2, Some(5), 10).unwrap();
    assert_eq!(c.peek(9, 12), Some(90));
    assert_eq!(c.get(9, 14), Some(90));
    assert_eq!(c.get(9, 16), None);
    assert_eq!(c.len(16), 0);
    assert_eq!(c.total_weight(16), 0);
}
