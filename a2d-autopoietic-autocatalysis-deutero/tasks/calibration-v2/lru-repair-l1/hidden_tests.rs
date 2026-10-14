// R1–R3, R6
#[test]
fn initial_zero_and_extremes() {
    let mut c = Cache::new(0);
    assert!(c.snapshot().is_empty());
    assert_eq!(c.clock(), 0);
    assert_eq!(
        c.insert(u64::MAX, i64::MIN, 1, None, 0),
        Err(Error::TooHeavy {
            weight: 1,
            capacity: 0
        })
    );
}

// R7–R8
#[test]
fn deadline_is_inclusive() {
    let mut c = Cache::new(8);
    c.insert(99, -7, 2, Some(3), 4).unwrap();
    assert_eq!(c.get(99, 7), Ok(None));
    assert_eq!(c.used(), 0);
}

// R7–R8
#[test]
fn deadline_before_boundary() {
    let mut c = Cache::new(8);
    c.insert(90, -7, 2, Some(3), 4).unwrap();
    assert_eq!(c.get(90, 6), Ok(Some(-7)));
    assert_eq!(c.snapshot()[0].expires_at, Some(7));
}

// R9–R10
#[test]
fn purge_key_order() {
    let mut c = Cache::new(10);
    for k in [8, 2, 6] {
        c.insert(k, 0, 1, Some(1), 0).unwrap();
    }
    assert_eq!(c.advance(2), Ok(vec![2, 6, 8]));
}

// R2–R3, R15
#[test]
fn snapshot_does_not_touch() {
    let mut c = Cache::new(2);
    c.insert(8, 8, 1, None, 0).unwrap();
    c.insert(9, 9, 1, None, 0).unwrap();
    let _ = c.snapshot();
    assert_eq!(c.insert(10, 0, 1, None, 0), Ok(vec![8]));
}

// R13–R14, R23
#[test]
fn zero_ttl_replacement() {
    let mut c = Cache::new(8);
    c.insert(6, 6, 5, None, 0).unwrap();
    assert_eq!(c.insert(6, 7, 2, Some(0), 1), Ok(vec![]));
    assert_eq!(c.used(), 0);
    assert!(c.snapshot().is_empty());
}

// R14
#[test]
fn zero_ttl_no_other_evictions() {
    let mut c = Cache::new(3);
    c.insert(10, 0, 3, None, 0).unwrap();
    assert_eq!(c.insert(20, 0, 3, Some(0), 0), Ok(vec![]));
    assert_eq!(c.used(), 3);
}

// R5–R6, R14
#[test]
fn zero_ttl_validates_weight() {
    let mut c = Cache::new(1);
    assert_eq!(
        c.insert(6, 0, 2, Some(0), 5),
        Err(Error::TooHeavy {
            weight: 2,
            capacity: 1
        })
    );
    assert_eq!(c.clock(), 0);
}

// R4–R6
#[test]
fn validation_time_precedes_weight() {
    let mut c = Cache::new(1);
    c.advance(8).unwrap();
    assert_eq!(
        c.insert(4, 0, 0, Some(u64::MAX), 7),
        Err(Error::BackwardTime { last: 8, now: 7 })
    );
}

// R6
#[test]
fn weight_precedence_over_overflow() {
    let mut c = Cache::new(3);
    assert_eq!(c.insert(4, 0, 0, Some(u64::MAX), 1), Err(Error::ZeroWeight));
    assert_eq!(
        c.insert(4, 0, 4, Some(u64::MAX), 1),
        Err(Error::TooHeavy {
            weight: 4,
            capacity: 3
        })
    );
}

// R5–R7
#[test]
fn overflow_atomic_no_expiry() {
    let mut c = Cache::new(3);
    c.insert(8, 1, 1, Some(1), 0).unwrap();
    let before = c.snapshot();
    assert_eq!(
        c.insert(9, 0, 1, Some(u64::MAX), 2),
        Err(Error::DeadlineOverflow)
    );
    assert_eq!(c.snapshot(), before);
    assert_eq!(c.clock(), 0);
}

// R5–R6
#[test]
fn invalid_weight_atomic_no_expiry() {
    let mut c = Cache::new(4);
    c.insert(8, 1, 2, Some(1), 0).unwrap();
    assert_eq!(c.insert(9, 0, 0, None, 2), Err(Error::ZeroWeight));
    assert_eq!(c.used(), 2);
    assert_eq!(c.snapshot().len(), 1);
    assert_eq!(c.clock(), 0);
}

// R13, R15
#[test]
fn replacement_credit_before_eviction() {
    let mut c = Cache::new(8);
    c.insert(2, 2, 4, None, 0).unwrap();
    c.insert(3, 3, 4, None, 0).unwrap();
    assert_eq!(c.insert(3, 30, 5, None, 0), Ok(vec![2]));
    assert_eq!(c.used(), 5);
    assert_eq!(c.snapshot()[0].value, 30);
}

// R13, R16
#[test]
fn replacement_preserves_other_keys() {
    let mut c = Cache::new(8);
    c.insert(2, 2, 4, None, 0).unwrap();
    c.insert(3, 3, 4, None, 0).unwrap();
    assert_eq!(c.insert(3, 30, 3, None, 0), Ok(vec![]));
    assert_eq!(c.used(), 7);
    assert_eq!(c.snapshot().len(), 2);
}

// R16, R20
#[test]
fn replacement_touches_at_equal_time() {
    let mut c = Cache::new(3);
    for k in [7, 8, 9] {
        c.insert(k, 0, 1, None, 0).unwrap();
    }
    c.insert(7, 1, 1, None, 0).unwrap();
    assert_eq!(c.insert(10, 0, 1, None, 0), Ok(vec![8]));
}

// R8, R11
#[test]
fn miss_advances_but_not_recency() {
    let mut c = Cache::new(2);
    for k in [30, 40] {
        c.insert(k, 0, 1, None, 0).unwrap();
    }
    assert_eq!(c.get(50, 5), Ok(None));
    assert_eq!(c.clock(), 5);
    assert_eq!(c.insert(60, 0, 1, None, 5), Ok(vec![30]));
}

// R8, R12
#[test]
fn remove_expired_returns_none() {
    let mut c = Cache::new(4);
    c.insert(18, -1, 2, Some(2), 3).unwrap();
    assert_eq!(c.remove(18, 5), Ok(None));
    assert_eq!(c.used(), 0);
}

// R8–R9, R13, R17
#[test]
fn expired_same_key_reported() {
    let mut c = Cache::new(4);
    c.insert(18, 1, 2, Some(1), 0).unwrap();
    assert_eq!(c.insert(18, 2, 3, None, 1), Ok(vec![18]));
    assert_eq!(c.get(18, 1), Ok(Some(2)));
}

// R9, R15, R17
#[test]
fn purge_precedes_capacity_victims() {
    let mut c = Cache::new(7);
    c.insert(80, 0, 2, None, 0).unwrap();
    c.insert(20, 0, 2, Some(1), 0).unwrap();
    c.insert(10, 0, 2, Some(1), 0).unwrap();
    assert_eq!(c.insert(90, 0, 7, None, 2), Ok(vec![10, 20, 80]));
}

// R9, R18
#[test]
fn resize_purge_then_lru() {
    let mut c = Cache::new(9);
    c.insert(90, 0, 2, None, 0).unwrap();
    c.insert(70, 0, 2, Some(1), 0).unwrap();
    c.insert(60, 0, 2, Some(1), 0).unwrap();
    c.insert(80, 0, 2, None, 0).unwrap();
    assert_eq!(c.resize(2, 2), Ok(vec![60, 70, 90]));
    assert_eq!(c.used(), 2);
}

// R18–R19
#[test]
fn resize_zero_and_grow() {
    let mut c = Cache::new(3);
    c.insert(33, 0, 3, None, 0).unwrap();
    assert_eq!(c.resize(0, 0), Ok(vec![33]));
    assert_eq!(c.resize(10, 1), Ok(vec![]));
    assert_eq!(c.capacity(), 10);
}

// R4–R5, R21
#[test]
fn resize_error_atomic() {
    let mut c = Cache::new(4);
    c.insert(5, 0, 3, None, 6).unwrap();
    assert_eq!(c.resize(0, 5), Err(Error::BackwardTime { last: 6, now: 5 }));
    assert_eq!(c.capacity(), 4);
    assert_eq!(c.used(), 3);
    assert_eq!(c.clock(), 6);
}

// R22
#[test]
fn maximum_weight_no_overflow() {
    let mut c = Cache::new(u64::MAX);
    c.insert(14, i64::MAX, u64::MAX, None, 0).unwrap();
    assert_eq!(c.insert(15, i64::MIN, u64::MAX, None, 0), Ok(vec![14]));
    assert_eq!(c.used(), u64::MAX);
    assert_eq!(c.get(15, 0), Ok(Some(i64::MIN)));
}

// R7–R8
#[test]
fn maximum_deadline_valid() {
    let mut c = Cache::new(1);
    c.insert(71, 1, 1, Some(1), u64::MAX - 1).unwrap();
    assert_eq!(c.advance(u64::MAX), Ok(vec![71]));
}

// R3, R21, R23–R24
#[test]
fn survivor_order_after_expiry() {
    let mut c = Cache::new(50);
    for k in 0..30 {
        c.insert(k, k as i64, 1, if k % 2 == 0 { Some(1) } else { None }, 0)
            .unwrap();
    }
    c.advance(1).unwrap();
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        (0..30).filter(|k| k % 2 == 1).collect::<Vec<_>>()
    );
    assert_eq!(c.used(), 15);
}

// R15, R22–R23
#[test]
fn multiple_victims_exact_boundary() {
    let mut c = Cache::new(10);
    for k in [41, 42, 43, 44, 45] {
        c.insert(k, 0, 2, None, 0).unwrap();
    }
    assert_eq!(c.insert(46, 0, 6, None, 0), Ok(vec![41, 42, 43]));
    assert_eq!(c.used(), 10);
}

// R11, R20
#[test]
fn equal_zero_time_get_touches() {
    let mut c = Cache::new(3);
    for k in [101, 102, 103] {
        c.insert(k, 0, 1, None, 0).unwrap();
    }
    assert_eq!(c.get(101, 0), Ok(Some(0)));
    assert_eq!(c.insert(104, 0, 1, None, 0), Ok(vec![102]));
}
