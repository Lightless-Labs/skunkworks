#[test]
fn hidden_exact_deadline_is_expired() {
    let mut c = LruCache::new(7);
    c.put(11, 110, 3, Some(6), 20).unwrap();
    assert_eq!(c.get(11, 25), Some(110));
    assert_eq!(c.get(11, 26), None);
    assert_eq!(c.total_weight(26), 0);
    assert!(c.keys_lru(26).is_empty());
}

#[test]
fn hidden_expired_holes_are_purged() {
    let mut c = LruCache::new(12);
    c.put(21, 1, 2, None, 0).unwrap();
    c.put(22, 2, 3, Some(5), 0).unwrap();
    c.put(23, 3, 4, None, 0).unwrap();
    c.put(24, 4, 1, Some(4), 0).unwrap();
    assert_eq!(c.keys_lru(6), vec![21, 23]);
    assert_eq!(c.total_weight(6), 6);
    assert_eq!(c.len(6), 2);
}

#[test]
fn hidden_error_still_purges_before_validation() {
    let mut c = LruCache::new(8);
    c.put(31, 1, 3, Some(3), 0).unwrap();
    c.put(32, 2, 4, None, 0).unwrap();
    assert_eq!(c.put(32, 99, 9, None, 5), Err(PutError::TooHeavy { weight: 9, capacity: 8 }));
    // Moving time backward makes the purge observable independently of later purges.
    assert_eq!(c.keys_lru(1), vec![32]);
    assert_eq!(c.peek(32, 1), Some(2));
    assert_eq!(c.total_weight(1), 4);
}

#[test]
fn hidden_replacement_installs_new_deadline() {
    let mut c = LruCache::new(5);
    c.put(41, 1, 2, Some(4), 0).unwrap();
    assert_eq!(c.put(41, 2, 3, Some(20), 2), Ok(Some(1)));
    assert_eq!(c.peek(41, 5), Some(2));
    assert_eq!(c.peek(41, 22), None);
    assert_eq!(c.total_weight(22), 0);
}

#[test]
fn hidden_ttl_can_change_between_finite_and_immortal() {
    let mut c = LruCache::new(7);
    c.put(51, 1, 2, Some(3), 0).unwrap();
    assert_eq!(c.put(51, 2, 2, None, 1), Ok(Some(1)));
    assert_eq!(c.peek(51, 100), Some(2));
    assert_eq!(c.put(51, 3, 2, Some(2), 100), Ok(Some(2)));
    assert_eq!(c.peek(51, 102), None);
}

#[test]
fn hidden_zero_ttl_deletes_without_evicting() {
    let mut c = LruCache::new(6);
    c.put(61, 1, 3, None, 0).unwrap(); c.put(62, 2, 3, None, 0).unwrap();
    assert_eq!(c.put(63, 3, 6, Some(0), 2), Ok(None));
    assert_eq!(c.keys_lru(2), vec![61, 62]);
    assert_eq!(c.put(62, 20, 6, Some(0), 2), Ok(Some(2)));
    assert_eq!(c.keys_lru(2), vec![61]);
    assert_eq!(c.total_weight(2), 3);
}

#[test]
fn hidden_zero_ttl_still_checks_weight_first() {
    let mut c = LruCache::new(4);
    c.put(71, 1, 2, Some(10), 0).unwrap(); c.put(72, 2, 2, None, 0).unwrap();
    assert_eq!(c.put(71, 99, 5, Some(0), 3), Err(PutError::TooHeavy { weight: 5, capacity: 4 }));
    assert_eq!(c.keys_lru(3), vec![71, 72]);
    assert_eq!(c.peek(71, 3), Some(1));
    assert_eq!(c.peek(71, 10), None);
    assert_eq!(c.keys_lru(10), vec![72]);
}

#[test]
fn hidden_saturated_deadlines_and_max_now() {
    let mut c = LruCache::new(9);
    let near = u64::MAX - 3;
    c.put(81, 1, 2, Some(20), near).unwrap();
    c.put(82, 2, 3, None, near).unwrap();
    assert_eq!(c.peek(81, u64::MAX - 1), Some(1));
    assert_eq!(c.peek(81, u64::MAX), None);
    assert_eq!(c.peek(82, u64::MAX), Some(2));
    assert_eq!(c.put(83, 3, 9, Some(1), u64::MAX), Ok(None));
    assert_eq!(c.keys_lru(u64::MAX), vec![82]);
    assert_eq!(c.put(82, 20, 9, Some(u64::MAX), u64::MAX), Ok(Some(2)));
    assert_eq!(c.len(u64::MAX), 0);
}

#[test]
fn hidden_purge_precedes_live_eviction_and_old_lookup() {
    let mut c = LruCache::new(10);
    c.put(91, 1, 4, None, 0).unwrap();
    c.put(92, 2, 4, Some(3), 0).unwrap();
    c.put(93, 3, 2, None, 0).unwrap();
    assert_eq!(c.put(92, 20, 4, None, 4), Ok(None));
    assert_eq!(c.keys_lru(4), vec![91, 93, 92]);
    assert_eq!(c.total_weight(4), 10);
}

#[test]
fn hidden_get_changes_order_but_never_lifetime() {
    let mut c = LruCache::new(6);
    c.put(101, 1, 2, Some(8), 0).unwrap();
    c.put(102, 2, 2, None, 0).unwrap();
    c.put(103, 3, 2, None, 0).unwrap();
    assert_eq!(c.peek(101, 3), Some(1));
    assert_eq!(c.keys_lru(3), vec![101, 102, 103]);
    assert_eq!(c.get(101, 4), Some(1));
    assert_eq!(c.keys_lru(4), vec![102, 103, 101]);
    assert_eq!(c.get(101, 8), None);
    assert_eq!(c.keys_lru(8), vec![102, 103]);
}

#[test]
fn hidden_zero_capacity_with_zero_weight_expiration() {
    let mut c = LruCache::new(0);
    c.put(111, 1, 0, None, 0).unwrap();
    c.put(112, 2, 0, Some(5), 0).unwrap();
    assert_eq!(c.put(113, 3, 1, Some(0), 2), Err(PutError::TooHeavy { weight: 1, capacity: 0 }));
    assert_eq!(c.len(2), 2);
    assert_eq!(c.len(5), 1);
    assert_eq!(c.total_weight(5), 0);
    assert_eq!(c.keys_lru(5), vec![111]);
}

#[test]
fn hidden_multi_eviction_with_zero_weights_and_growth() {
    let mut c = LruCache::new(10);
    c.put(121, 1, 0, None, 0).unwrap();
    c.put(122, 2, 2, None, 0).unwrap();
    c.put(123, 3, 3, Some(50), 0).unwrap();
    c.put(124, 4, 3, None, 0).unwrap();
    c.put(125, 5, 2, None, 0).unwrap();
    assert_eq!(c.put(123, 30, 9, Some(40), 1), Ok(Some(3)));
    assert_eq!(c.keys_lru(1), vec![123]);
    assert_eq!(c.total_weight(1), 9);
    assert_eq!(c.peek(123, 41), None);
}

#[test]
fn hidden_every_observer_and_miss_purges() {
    for op in 0..7 {
        let mut c = LruCache::new(5);
        c.put(131, 1, 1, None, 0).unwrap();
        c.put(132, 2, 2, Some(2), 0).unwrap();
        match op {
            0 => { assert_eq!(c.len(3), 1); }
            1 => { assert_eq!(c.total_weight(3), 1); }
            2 => { assert_eq!(c.keys_lru(3), vec![131]); }
            3 => { assert_eq!(c.get(999, 3), None); }
            4 => { assert_eq!(c.peek(999, 3), None); }
            5 => { assert_eq!(c.remove(999, 3), None); }
            _ => { assert_eq!(c.put(133, 3, 0, Some(0), 3), Ok(None)); }
        }
        assert_eq!(c.keys_lru(0), vec![131]);
        assert_eq!(c.total_weight(0), 1);
    }
}

#[test]
fn hidden_time_can_go_back_without_reviving() {
    let mut c = LruCache::new(7);
    c.put(141, 1, 2, Some(5), 20).unwrap();
    assert_eq!(c.peek(141, 10), Some(1));
    assert_eq!(c.remove(141, 26), None);
    assert_eq!(c.get(141, 0), None);
    c.put(142, 2, 3, Some(5), 100).unwrap();
    assert_eq!(c.get(142, 2), Some(2));
    assert_eq!(c.get(142, 105), None);
    assert_eq!(c.total_weight(0), 0);
}

#[test]
fn hidden_max_weight_and_extreme_keys() {
    let mut c = LruCache::new(usize::MAX);
    c.put(i32::MIN, i32::MAX, usize::MAX - 1, Some(9), 0).unwrap();
    c.put(i32::MAX, i32::MIN, 1, None, 0).unwrap();
    assert_eq!(c.total_weight(1), usize::MAX);
    assert_eq!(c.put(151, 3, usize::MAX, None, 9), Ok(None));
    assert_eq!(c.keys_lru(9), vec![151]);
    assert_eq!(c.total_weight(9), usize::MAX);
    assert_eq!(c.remove(151, 9), Some(3));
    assert_eq!(c.total_weight(9), 0);
}

#[test]
fn hidden_large_mixed_expiration_keeps_survivor_order() {
    let mut c = LruCache::new(1000);
    for k in 200..500 {
        let ttl = if k % 3 == 0 { Some(4) } else { None };
        c.put(k, -k, 1, ttl, 10).unwrap();
    }
    let mut snapshot = c.keys_lru(11); snapshot.clear();
    assert_eq!(c.keys_lru(15), (200..500).filter(|k| k % 3 != 0).collect::<Vec<_>>());
    assert_eq!(c.len(15), 200);
    assert_eq!(c.total_weight(15), 200);
    assert_eq!(c.remove(202, 15), Some(-202));
    assert_eq!(c.get(203, 15), Some(-203));
    let expected = (200..500).filter(|k| k % 3 != 0 && *k != 202 && *k != 203).chain([203]).collect::<Vec<_>>();
    assert_eq!(c.keys_lru(15), expected);
}
