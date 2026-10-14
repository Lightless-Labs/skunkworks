#[test]
fn public_read_and_evict() {
    let mut c = LruCache::new(2);
    assert_eq!(c.len(), 0);
    c.put(1, 10); c.put(2, 20);
    assert_eq!(c.get(1), Some(10));
    c.put(3, 30);
    assert_eq!(c.keys_lru(), vec![1, 3]);
    assert_eq!(c.get(2), None);
}

#[test]
fn public_peek_keeps_order() {
    let mut c = LruCache::new(2);
    c.put(4, 40); c.put(5, 50);
    assert_eq!(c.peek(4), Some(40));
    assert_eq!(c.keys_lru(), vec![4, 5]);
    assert_eq!(c.peek(99), None);
}

#[test]
fn public_remove_present() {
    let mut c = LruCache::new(3);
    c.put(6, 60); c.put(7, 70);
    assert_eq!(c.remove(6), Some(60));
    assert_eq!(c.len(), 1);
    assert_eq!(c.keys_lru(), vec![7]);
}

#[test]
fn public_update_single_entry() {
    let mut c = LruCache::new(2);
    assert_eq!(c.put(8, 80), None);
    assert_eq!(c.put(8, 81), Some(80));
    assert_eq!(c.peek(8), Some(81));
    assert_eq!(c.len(), 1);
}
