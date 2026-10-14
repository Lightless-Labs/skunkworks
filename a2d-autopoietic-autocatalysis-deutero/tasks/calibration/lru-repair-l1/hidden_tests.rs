#[test]
fn hidden_empty_and_extreme_misses() {
    let mut c = LruCache::new(7);
    assert_eq!(c.len(), 0);
    assert!(c.keys_lru().is_empty());
    assert_eq!(c.get(i32::MIN), None);
    assert_eq!(c.get(i32::MAX), None);
    assert!(c.keys_lru().is_empty());
}

#[test]
fn hidden_zero_capacity_never_retains() {
    let mut c = LruCache::new(0);
    for k in -5..5 {
        assert_eq!(c.put(k, k * 2), None);
        assert_eq!(c.get(k), None);
        assert_eq!(c.len(), 0);
        assert!(c.keys_lru().is_empty());
    }
}

#[test]
fn hidden_zero_repeated_key() {
    let mut c = LruCache::new(0);
    assert_eq!(c.put(90, 1), None);
    assert_eq!(c.put(90, 2), None);
    assert_eq!(c.get(90), None);
}

#[test]
fn hidden_capacity_one_replaces_and_evicts() {
    let mut c = LruCache::new(1);
    c.put(-8, 1);
    assert_eq!(c.put(-8, 2), Some(1));
    assert_eq!(c.keys_lru(), vec![-8]);
    assert_eq!(c.put(17, 3), None);
    assert_eq!(c.get(-8), None);
    assert_eq!(c.get(17), Some(3));
    assert_eq!(c.len(), 1);
}

#[test]
fn hidden_replacement_promotes() {
    let mut c = LruCache::new(3);
    c.put(11, 1); c.put(22, 2); c.put(33, 3);
    assert_eq!(c.put(11, 111), Some(1));
    assert_eq!(c.keys_lru(), vec![22, 33, 11]);
    c.put(44, 4);
    assert_eq!(c.get(22), None);
    assert_eq!(c.get(11), Some(111));
}

#[test]
fn hidden_identical_update_is_access() {
    let mut c = LruCache::new(2);
    c.put(8, -1); c.put(9, -2);
    assert_eq!(c.put(8, -1), Some(-1));
    c.put(10, -3);
    assert_eq!(c.keys_lru(), vec![8, 10]);
}

#[test]
fn hidden_middle_hit_preserves_survivors() {
    let mut c = LruCache::new(4);
    for k in 10..14 { c.put(k, -k); }
    assert_eq!(c.get(11), Some(-11));
    assert_eq!(c.keys_lru(), vec![10, 12, 13, 11]);
}

#[test]
fn hidden_mru_hits_are_idempotent() {
    let mut c = LruCache::new(3);
    for k in 6..9 { c.put(k, k); }
    for _ in 0..12 { assert_eq!(c.get(8), Some(8)); }
    assert_eq!(c.keys_lru(), vec![6, 7, 8]);
}

#[test]
fn hidden_misses_preserve_victim() {
    let mut c = LruCache::new(2);
    c.put(15, 1); c.put(16, 2);
    assert_eq!(c.get(-100), None);
    assert_eq!(c.keys_lru(), vec![15, 16]);
    c.put(17, 3);
    assert_eq!(c.keys_lru(), vec![16, 17]);
}

#[test]
fn hidden_snapshot_is_owned() {
    let mut c = LruCache::new(2);
    c.put(71, 1); c.put(72, 2);
    let mut snapshot = c.keys_lru();
    snapshot.clear();
    assert_eq!(c.keys_lru(), vec![71, 72]);
    c.put(73, 3);
    assert_eq!(c.keys_lru(), vec![72, 73]);
}

#[test]
fn hidden_i32_boundaries() {
    let mut c = LruCache::new(2);
    c.put(i32::MIN, i32::MAX); c.put(i32::MAX, i32::MIN);
    assert_eq!(c.get(i32::MIN), Some(i32::MAX));
    assert_eq!(c.get(i32::MAX), Some(i32::MIN));
    assert_eq!(c.len(), 2);
}

#[test]
fn hidden_repeated_updates_do_not_consume_capacity() {
    let mut c = LruCache::new(3);
    c.put(50, 0); c.put(51, 1); c.put(52, 2);
    for v in 1..100 {
        assert_eq!(c.put(50, v), Some(v - 1));
        assert_eq!(c.len(), 3);
    }
    assert_eq!(c.keys_lru(), vec![51, 52, 50]);
}

#[test]
fn hidden_large_capacity_is_not_preallocated() {
    let mut c = LruCache::new(usize::MAX);
    c.put(88, 1); c.put(89, 2);
    assert_eq!(c.len(), 2);
    assert_eq!(c.get(88), Some(1));
    assert_eq!(c.keys_lru(), vec![89, 88]);
}

#[test]
fn hidden_long_eviction_sequence() {
    let mut c = LruCache::new(31);
    for k in 0..1000 { assert_eq!(c.put(k, -k), None); }
    assert_eq!(c.len(), 31);
    assert_eq!(c.keys_lru(), (969..1000).collect::<Vec<_>>());
    for k in 0..969 { assert_eq!(c.get(k), None); }
}

#[test]
fn hidden_touch_then_update_then_eviction() {
    let mut c = LruCache::new(3);
    c.put(20, 2); c.put(30, 3); c.put(40, 4);
    c.get(20);
    assert_eq!(c.put(30, 300), Some(3));
    c.put(60, 6);
    assert_eq!(c.keys_lru(), vec![20, 30, 60]);
    assert_eq!(c.get(40), None);
}

#[test]
fn hidden_distinct_cache_instances() {
    let mut a = LruCache::new(2);
    let mut b = LruCache::new(1);
    a.put(70, 7); b.put(70, 8); a.put(80, 9); b.put(90, 10);
    assert_eq!(a.get(70), Some(7));
    assert_eq!(b.get(70), None);
    assert_eq!(b.keys_lru(), vec![90]);
}
