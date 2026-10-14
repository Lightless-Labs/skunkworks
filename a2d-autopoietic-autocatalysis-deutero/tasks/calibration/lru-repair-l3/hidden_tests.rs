#[test]
fn hidden_weighted_empty_misses() {
    let mut c = LruCache::new(12);
    assert_eq!(c.get(-70), None);
    assert_eq!(c.peek(-70), None);
    assert_eq!(c.remove(-70), None);
    assert_eq!(c.len(), 0);
    assert_eq!(c.total_weight(), 0);
    assert!(c.keys_lru().is_empty());
}

#[test]
fn hidden_zero_capacity_accepts_only_zero() {
    let mut c = LruCache::new(0);
    assert_eq!(c.put(11, 110, 0), Ok(None));
    assert_eq!(c.put(12, 120, 0), Ok(None));
    assert_eq!(c.put(13, 130, 1), Err(PutError::TooHeavy { weight: 1, capacity: 0 }));
    assert_eq!(c.total_weight(), 0);
    assert_eq!(c.len(), 2);
    assert_eq!(c.keys_lru(), vec![11, 12]);
}

#[test]
fn hidden_multiple_victims_needed() {
    let mut c = LruCache::new(10);
    for k in 21..26 { c.put(k, k, 2).unwrap(); }
    assert_eq!(c.put(26, 260, 7), Ok(None));
    assert_eq!(c.keys_lru(), vec![25, 26]);
    assert_eq!(c.total_weight(), 9);
    assert_eq!(c.len(), 2);
}

#[test]
fn hidden_zero_weight_victims_are_not_skipped() {
    let mut c = LruCache::new(5);
    c.put(31, 1, 0).unwrap(); c.put(32, 2, 0).unwrap();
    c.put(33, 3, 5).unwrap(); c.put(34, 4, 0).unwrap();
    c.put(35, 5, 1).unwrap();
    assert_eq!(c.keys_lru(), vec![34, 35]);
    assert_eq!(c.total_weight(), 1);
}

#[test]
fn hidden_oversized_update_is_atomic() {
    let mut c = LruCache::new(7);
    c.put(41, 410, 2).unwrap(); c.put(42, 420, 3).unwrap();
    assert_eq!(c.put(41, 999, 8), Err(PutError::TooHeavy { weight: 8, capacity: 7 }));
    assert_eq!(c.keys_lru(), vec![41, 42]);
    assert_eq!(c.peek(41), Some(410));
    assert_eq!(c.total_weight(), 5);
    c.put(43, 430, 3).unwrap();
    assert_eq!(c.keys_lru(), vec![42, 43]);
}

#[test]
fn hidden_growth_removes_target_before_eviction() {
    let mut c = LruCache::new(10);
    c.put(51, 1, 3).unwrap(); c.put(52, 2, 4).unwrap(); c.put(53, 3, 3).unwrap();
    assert_eq!(c.put(52, 20, 8), Ok(Some(2)));
    assert_eq!(c.keys_lru(), vec![52]);
    assert_eq!(c.total_weight(), 8);
    assert_eq!(c.peek(52), Some(20));
}

#[test]
fn hidden_shrink_retains_others_and_promotes() {
    let mut c = LruCache::new(10);
    c.put(61, 1, 6).unwrap(); c.put(62, 2, 4).unwrap();
    assert_eq!(c.put(61, 10, 1), Ok(Some(1)));
    assert_eq!(c.keys_lru(), vec![62, 61]);
    assert_eq!(c.total_weight(), 5);
    c.put(63, 3, 5).unwrap();
    assert_eq!(c.keys_lru(), vec![62, 61, 63]);
}

#[test]
fn hidden_removal_releases_weight() {
    let mut c = LruCache::new(6);
    c.put(71, 1, 2).unwrap(); c.put(72, 2, 4).unwrap();
    assert_eq!(c.remove(71), Some(1));
    assert_eq!(c.total_weight(), 4);
    assert_eq!(c.remove(999), None);
    c.put(73, 3, 2).unwrap();
    assert_eq!(c.keys_lru(), vec![72, 73]);
    assert_eq!(c.total_weight(), 6);
}

#[test]
fn hidden_max_weight_arithmetic() {
    let mut c = LruCache::new(usize::MAX);
    c.put(81, 1, usize::MAX - 2).unwrap(); c.put(82, 2, 2).unwrap();
    assert_eq!(c.total_weight(), usize::MAX);
    c.put(83, 3, 3).unwrap();
    assert_eq!(c.keys_lru(), vec![82, 83]);
    assert_eq!(c.total_weight(), 5);
    assert_eq!(c.put(83, 30, usize::MAX), Ok(Some(3)));
    assert_eq!(c.keys_lru(), vec![83]);
    assert_eq!(c.total_weight(), usize::MAX);
}

#[test]
fn hidden_full_exact_fit_and_zero_addition() {
    let mut c = LruCache::new(9);
    c.put(91, 1, 4).unwrap(); c.put(92, 2, 5).unwrap();
    c.put(93, 3, 0).unwrap();
    assert_eq!(c.keys_lru(), vec![91, 92, 93]);
    assert_eq!(c.total_weight(), 9);
    assert_eq!(c.len(), 3);
}

#[test]
fn hidden_reads_change_only_intended_recency() {
    let mut c = LruCache::new(6);
    for k in 101..104 { c.put(k, k, 2).unwrap(); }
    assert_eq!(c.peek(101), Some(101));
    assert_eq!(c.get(102), Some(102));
    assert_eq!(c.peek(777), None);
    assert_eq!(c.get(777), None);
    assert_eq!(c.keys_lru(), vec![101, 103, 102]);
    c.put(104, 104, 2).unwrap();
    assert_eq!(c.keys_lru(), vec![103, 102, 104]);
}

#[test]
fn hidden_same_weight_update_promotes() {
    let mut c = LruCache::new(4);
    c.put(111, 1, 2).unwrap(); c.put(112, 2, 2).unwrap();
    assert_eq!(c.put(111, 1, 2), Ok(Some(1)));
    c.put(113, 3, 2).unwrap();
    assert_eq!(c.keys_lru(), vec![111, 113]);
}

#[test]
fn hidden_weight_changes_between_zero_and_positive() {
    let mut c = LruCache::new(3);
    c.put(121, 1, 0).unwrap(); c.put(122, 2, 3).unwrap();
    assert_eq!(c.put(121, 10, 2), Ok(Some(1)));
    assert_eq!(c.keys_lru(), vec![121]);
    assert_eq!(c.put(121, 11, 0), Ok(Some(10)));
    assert_eq!(c.total_weight(), 0);
    c.put(123, 3, 3).unwrap();
    assert_eq!(c.keys_lru(), vec![121, 123]);
}

#[test]
fn hidden_extreme_values_and_error_fields() {
    let mut c = LruCache::new(usize::MAX - 1);
    c.put(i32::MIN, i32::MAX, 1).unwrap();
    assert_eq!(c.put(i32::MIN, 0, usize::MAX), Err(PutError::TooHeavy { weight: usize::MAX, capacity: usize::MAX - 1 }));
    assert_eq!(c.peek(i32::MIN), Some(i32::MAX));
    assert_eq!(c.remove(i32::MIN), Some(i32::MAX));
    assert_eq!(c.total_weight(), 0);
}

#[test]
fn hidden_many_zero_entries_and_snapshot() {
    let mut c = LruCache::new(1);
    for k in 200..500 { c.put(k, -k, 0).unwrap(); }
    assert_eq!(c.len(), 300);
    let mut snapshot = c.keys_lru(); snapshot.clear();
    assert_eq!(c.keys_lru(), (200..500).collect::<Vec<_>>());
    c.put(500, 1, 1).unwrap();
    assert_eq!(c.len(), 301);
    c.put(501, 2, 1).unwrap();
    assert_eq!(c.keys_lru(), vec![501]);
}

#[test]
fn hidden_repeated_updates_and_deletion() {
    let mut c = LruCache::new(17);
    c.put(131, 0, 1).unwrap(); c.put(132, 2, 2).unwrap();
    for v in 1..80 {
        assert_eq!(c.put(131, v, (v as usize % 7) + 1), Ok(Some(v - 1)));
        assert_eq!(c.total_weight(), (v as usize % 7) + 3);
        assert_eq!(c.len(), 2);
    }
    assert_eq!(c.remove(131), Some(79));
    assert_eq!(c.keys_lru(), vec![132]);
    assert_eq!(c.total_weight(), 2);
}
