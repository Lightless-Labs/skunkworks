#[test]
fn hidden_empty_all_operations() {
    let mut c = LruCache::new(4);
    assert_eq!(c.peek(29), None);
    assert_eq!(c.remove(29), None);
    assert_eq!(c.get(29), None);
    assert_eq!(c.len(), 0);
    assert!(c.keys_lru().is_empty());
}

#[test]
fn hidden_zero_all_operations() {
    let mut c = LruCache::new(0);
    for k in -4..4 {
        assert_eq!(c.put(k, 12), None);
        assert_eq!(c.peek(k), None);
        assert_eq!(c.remove(k), None);
        assert_eq!(c.get(k), None);
    }
    assert!(c.keys_lru().is_empty());
}

#[test]
fn hidden_peek_does_not_protect_victim() {
    let mut c = LruCache::new(3);
    for k in 11..14 { c.put(k, k * 2); }
    assert_eq!(c.peek(11), Some(22));
    c.put(14, 28);
    assert_eq!(c.keys_lru(), vec![12, 13, 14]);
    assert_eq!(c.peek(11), None);
}

#[test]
fn hidden_middle_peek_does_not_shuffle() {
    let mut c = LruCache::new(4);
    for k in 21..25 { c.put(k, k); }
    for _ in 0..5 { assert_eq!(c.peek(22), Some(22)); }
    assert_eq!(c.keys_lru(), vec![21, 22, 23, 24]);
}

#[test]
fn hidden_missing_remove_is_noop() {
    let mut c = LruCache::new(3);
    c.put(31, 310); c.put(32, 320);
    assert_eq!(c.remove(30), None);
    assert_eq!(c.keys_lru(), vec![31, 32]);
    assert_eq!(c.peek(31), Some(310));
}

#[test]
fn hidden_remove_middle_preserves_order() {
    let mut c = LruCache::new(4);
    for k in 41..45 { c.put(k, -k); }
    assert_eq!(c.remove(42), Some(-42));
    assert_eq!(c.keys_lru(), vec![41, 43, 44]);
    assert_eq!(c.len(), 3);
}

#[test]
fn hidden_remove_mru() {
    let mut c = LruCache::new(3);
    for k in 51..54 { c.put(k, k); }
    assert_eq!(c.remove(53), Some(53));
    assert_eq!(c.keys_lru(), vec![51, 52]);
    assert_eq!(c.remove(53), None);
}

#[test]
fn hidden_remove_frees_slot() {
    let mut c = LruCache::new(2);
    c.put(61, 1); c.put(62, 2);
    c.remove(62); c.put(63, 3);
    assert_eq!(c.keys_lru(), vec![61, 63]);
    assert_eq!(c.len(), 2);
}

#[test]
fn hidden_reinsert_removed_is_new() {
    let mut c = LruCache::new(3);
    c.put(71, 1); c.put(72, 2); c.put(73, 3);
    c.remove(72);
    assert_eq!(c.put(72, 20), None);
    assert_eq!(c.keys_lru(), vec![71, 73, 72]);
}

#[test]
fn hidden_update_promotes_before_eviction() {
    let mut c = LruCache::new(3);
    c.put(81, 1); c.put(82, 2); c.put(83, 3);
    assert_eq!(c.put(81, 10), Some(1));
    c.put(84, 4);
    assert_eq!(c.keys_lru(), vec![83, 81, 84]);
    assert_eq!(c.peek(82), None);
}

#[test]
fn hidden_same_value_update_promotes() {
    let mut c = LruCache::new(2);
    c.put(-9, -19); c.put(-8, -18);
    assert_eq!(c.put(-9, -19), Some(-19));
    assert_eq!(c.keys_lru(), vec![-8, -9]);
}

#[test]
fn hidden_get_peek_remove_interaction() {
    let mut c = LruCache::new(4);
    for k in 91..95 { c.put(k, k * 3); }
    assert_eq!(c.get(92), Some(276));
    assert_eq!(c.peek(91), Some(273));
    assert_eq!(c.remove(93), Some(279));
    assert_eq!(c.keys_lru(), vec![91, 94, 92]);
}

#[test]
fn hidden_extreme_keys_and_values() {
    let mut c = LruCache::new(2);
    c.put(i32::MIN, i32::MAX); c.put(i32::MAX, i32::MIN);
    assert_eq!(c.peek(i32::MIN), Some(i32::MAX));
    assert_eq!(c.remove(i32::MAX), Some(i32::MIN));
    assert_eq!(c.keys_lru(), vec![i32::MIN]);
}

#[test]
fn hidden_single_slot_reuse() {
    let mut c = LruCache::new(1);
    c.put(101, 1);
    assert_eq!(c.remove(101), Some(1));
    assert_eq!(c.remove(101), None);
    c.put(102, 2); c.put(103, 3);
    assert_eq!(c.peek(102), None);
    assert_eq!(c.peek(103), Some(3));
}

#[test]
fn hidden_bulk_removals_and_refill() {
    let mut c = LruCache::new(100);
    for k in 200..300 { c.put(k, -k); }
    for k in (200..300).step_by(2) { assert_eq!(c.remove(k), Some(-k)); }
    for k in 400..450 { c.put(k, k); }
    let expected = (201..300).step_by(2).chain(400..450).collect::<Vec<_>>();
    assert_eq!(c.keys_lru(), expected);
    assert_eq!(c.len(), 100);
}

#[test]
fn hidden_observations_and_misses_preserve_order() {
    let mut c = LruCache::new(usize::MAX);
    c.put(111, 11); c.put(112, 12);
    let mut snapshot = c.keys_lru(); snapshot.reverse();
    assert_eq!(c.len(), 2);
    assert_eq!(c.peek(113), None);
    assert_eq!(c.get(113), None);
    assert_eq!(c.keys_lru(), vec![111, 112]);
}
