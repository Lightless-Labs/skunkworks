#[test]
fn public_weighted_insert_and_observe() {
    let mut c = LruCache::new(10);
    assert_eq!(c.len(), 0);
    assert_eq!(c.total_weight(), 0);
    assert_eq!(c.put(1, 10, 3), Ok(None));
    assert_eq!(c.put(2, 20, 4), Ok(None));
    assert_eq!(c.total_weight(), 7);
    assert_eq!(c.keys_lru(), vec![1, 2]);
    assert_eq!(c.len(), 2);
}

#[test]
fn public_get_peek_and_one_eviction() {
    let mut c = LruCache::new(8);
    c.put(1, 10, 3).unwrap(); c.put(2, 20, 3).unwrap();
    assert_eq!(c.peek(1), Some(10));
    assert_eq!(c.keys_lru(), vec![1, 2]);
    assert_eq!(c.get(1), Some(10));
    c.put(3, 30, 4).unwrap();
    assert_eq!(c.keys_lru(), vec![1, 3]);
    assert_eq!(c.total_weight(), 7);
    assert_eq!(c.get(2), None);
}

#[test]
fn public_update_and_remove() {
    let mut c = LruCache::new(9);
    c.put(4, 40, 2).unwrap(); c.put(5, 50, 3).unwrap();
    assert_eq!(c.put(4, 41, 4), Ok(Some(40)));
    assert_eq!(c.keys_lru(), vec![5, 4]);
    assert_eq!(c.total_weight(), 7);
    assert_eq!(c.remove(5), Some(50));
    assert_eq!(c.total_weight(), 4);
    assert_eq!(c.remove(5), None);
    assert_eq!(c.len(), 1);
}

#[test]
fn public_reject_new_oversized_item() {
    let mut c = LruCache::new(5);
    c.put(6, 60, 3).unwrap();
    assert_eq!(c.put(7, 70, 6), Err(PutError::TooHeavy { weight: 6, capacity: 5 }));
    assert_eq!(c.keys_lru(), vec![6]);
    assert_eq!(c.total_weight(), 3);
}
