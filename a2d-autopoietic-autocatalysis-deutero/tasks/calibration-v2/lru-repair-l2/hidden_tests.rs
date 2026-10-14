// R4, R11, R19
#[test]
fn odd_ceil_aging() {
    let mut c = Cache::new(2, 100).unwrap();
    c.put(70, 0).unwrap();
    c.get(70);
    c.get(70);
    c.age(1);
    assert_eq!(c.snapshot()[0].frequency, 2);
}

// R9–R13
#[test]
fn auto_boundary_before_credit() {
    let mut c = Cache::new(2, 3).unwrap();
    c.put(30, 0).unwrap();
    c.get(30);
    c.get(30);
    assert_eq!(c.pending(), 0);
    assert_eq!(c.snapshot()[0].frequency, 2);
}

// R6, R10–R12, R16, R25
#[test]
fn auto_boundary_before_eviction() {
    let mut c = Cache::new(2, 5).unwrap();
    c.put(90, 0).unwrap();
    c.get(90);
    c.put(20, 0).unwrap();
    assert_eq!(c.get(500), None);
    assert_eq!(c.put(30, 0), Ok(vec![90]));
}

// R9–R10, R14
#[test]
fn miss_get_counts() {
    let mut c = Cache::new(1, 2).unwrap();
    c.put(40, 0).unwrap();
    assert_eq!(c.get(44), None);
    assert_eq!(c.pending(), 0);
}

// R9–R10, R18
#[test]
fn miss_remove_counts() {
    let mut c = Cache::new(1, 2).unwrap();
    c.put(40, 0).unwrap();
    assert_eq!(c.remove(44), None);
    assert_eq!(c.pending(), 0);
}

// R9–R12, R15
#[test]
fn update_counts_boundary() {
    let mut c = Cache::new(2, 3).unwrap();
    c.put(22, 1).unwrap();
    c.get(22);
    assert_eq!(c.put(22, 2), Ok(vec![]));
    assert_eq!(c.pending(), 0);
    assert_eq!(c.snapshot()[0].frequency, 2);
    assert_eq!(c.snapshot()[0].value, 2);
}

// R17, R22–R23
#[test]
fn disabled_does_not_count() {
    let mut c = Cache::new(1, 4).unwrap();
    c.get(10);
    c.resize(0);
    assert_eq!(c.put(20, 0), Err(Error::Disabled));
    assert_eq!(c.pending(), 1);
}

// R9, R23, R27
#[test]
fn disabled_reads_count() {
    let mut c = Cache::new(0, 3).unwrap();
    c.get(1);
    c.remove(1);
    assert_eq!(c.pending(), 2);
    c.get(1);
    assert_eq!(c.pending(), 0);
}

// R5–R7
#[test]
fn global_tie_not_key() {
    let mut c = Cache::new(2, 100).unwrap();
    c.put(81, 0).unwrap();
    c.put(12, 0).unwrap();
    assert_eq!(c.put(33, 0), Ok(vec![81]));
}

// R6, R13, R16
#[test]
fn frequency_overrides_old_recency() {
    let mut c = Cache::new(2, 100).unwrap();
    c.put(80, 0).unwrap();
    c.get(80);
    c.put(10, 0).unwrap();
    assert_eq!(c.put(20, 0), Ok(vec![10]));
}

// R6–R7, R20, R25
#[test]
fn aging_merge_keeps_global_recency() {
    let mut c = Cache::new(3, 100).unwrap();
    c.put(80, 0).unwrap();
    c.get(80);
    c.put(10, 0).unwrap();
    c.age(1);
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![80, 10]
    );
}

// R4, R13, R26
#[test]
fn saturation_seven() {
    let mut c = Cache::new(2, 1000).unwrap();
    c.put(91, 0).unwrap();
    for _ in 0..30 {
        c.get(91);
    }
    assert_eq!(c.snapshot()[0].frequency, 7);
}

// R5–R6, R26
#[test]
fn saturated_hit_touches() {
    let mut c = Cache::new(2, 1000).unwrap();
    for k in [50, 60] {
        c.put(k, 0).unwrap();
        for _ in 0..6 {
            c.get(k);
        }
    }
    c.get(50);
    assert_eq!(c.put(70, 0), Ok(vec![60]));
}

// R15, R26
#[test]
fn saturated_update_touches() {
    let mut c = Cache::new(2, 1000).unwrap();
    for k in [50, 60] {
        c.put(k, 0).unwrap();
        for _ in 0..6 {
            c.get(k);
        }
    }
    c.put(50, -1).unwrap();
    assert_eq!(c.put(70, 0), Ok(vec![60]));
    assert_eq!(c.get(50), Some(-1));
}

// R19–R20
#[test]
fn manual_zero_noop() {
    let mut c = Cache::new(2, 100).unwrap();
    c.put(77, 0).unwrap();
    c.get(77);
    let before = c.snapshot();
    c.age(0);
    assert_eq!(c.snapshot(), before);
    assert_eq!(c.pending(), 2);
}

// R4, R11, R19
#[test]
fn manual_many_rounds_floor_one() {
    let mut c = Cache::new(1, 100).unwrap();
    c.put(71, 0).unwrap();
    for _ in 0..6 {
        c.get(71);
    }
    c.age(255);
    assert_eq!(c.snapshot()[0].frequency, 1);
}

// R10, R19–R20
#[test]
fn manual_preserves_boundary() {
    let mut c = Cache::new(1, 4).unwrap();
    c.put(21, 0).unwrap();
    c.get(21);
    c.get(21);
    c.age(1);
    assert_eq!(c.pending(), 3);
    c.get(21);
    assert_eq!(c.snapshot()[0].frequency, 2);
    assert_eq!(c.pending(), 0);
}

// R6, R21
#[test]
fn resize_order_frequency_and_recency() {
    let mut c = Cache::new(4, 100).unwrap();
    for k in [9, 8, 7, 6] {
        c.put(k, 0).unwrap();
    }
    c.get(9);
    c.get(7);
    assert_eq!(c.resize(1), vec![8, 6, 9]);
    assert_eq!(c.snapshot()[0].key, 7);
}

// R8, R21–R22
#[test]
fn resize_keeps_pending() {
    let mut c = Cache::new(2, 7).unwrap();
    c.put(41, 0).unwrap();
    c.get(41);
    assert_eq!(c.resize(0), vec![41]);
    assert_eq!(c.pending(), 2);
    c.resize(3);
    c.put(42, 0).unwrap();
    assert_eq!(c.pending(), 3);
}

// R24
#[test]
fn growth_and_same_capacity() {
    let mut c = Cache::new(2, 9).unwrap();
    c.put(88, 0).unwrap();
    c.get(88);
    let before = c.snapshot();
    assert_eq!(c.resize(8), vec![]);
    assert_eq!(c.resize(8), vec![]);
    assert_eq!(c.snapshot(), before);
    assert_eq!(c.pending(), 2);
}

// R10–R13, R27
#[test]
fn period_one_semantics() {
    let mut c = Cache::new(1, 1).unwrap();
    c.put(15, 1).unwrap();
    for _ in 0..10 {
        c.get(15);
    }
    assert_eq!(c.pending(), 0);
    assert_eq!(c.snapshot()[0].frequency, 2);
    c.remove(99);
    assert_eq!(c.snapshot()[0].frequency, 1);
}

// R1, R27
#[test]
fn period_maximum_no_overflow() {
    let mut c = Cache::new(1, u32::MAX).unwrap();
    for _ in 0..50 {
        c.get(0);
    }
    assert_eq!(c.pending(), 50);
}

// R4, R18, R28
#[test]
fn reinsert_resets_history() {
    let mut c = Cache::new(2, 100).unwrap();
    c.put(64, 1).unwrap();
    for _ in 0..6 {
        c.get(64);
    }
    assert_eq!(c.remove(64), Some(1));
    c.put(64, 2).unwrap();
    assert_eq!(c.snapshot()[0].frequency, 1);
}

// R29
#[test]
fn clone_independent_state() {
    let mut a = Cache::new(2, 4).unwrap();
    a.put(21, 2).unwrap();
    let mut b = a.clone();
    b.get(21);
    b.age(1);
    b.resize(0);
    assert_eq!(a.pending(), 1);
    assert_eq!(a.snapshot()[0].frequency, 1);
    assert_eq!(a.capacity(), 2);
}

// R7–R8, R30
#[test]
fn snapshot_no_operation() {
    let mut c = Cache::new(2, 3).unwrap();
    c.put(99, 1).unwrap();
    c.get(99);
    let before = c.snapshot();
    for _ in 0..10 {
        assert_eq!(c.snapshot(), before);
        assert_eq!(c.capacity(), 2);
        assert_eq!(c.pending(), 2);
    }
}

// R3, R15, R18
#[test]
fn full_domain_values() {
    let mut c = Cache::new(usize::MAX, 100).unwrap();
    c.put(u64::MAX, i64::MIN).unwrap();
    c.put(0, i64::MAX).unwrap();
    assert_eq!(c.get(u64::MAX), Some(i64::MIN));
    assert_eq!(c.remove(0), Some(i64::MAX));
}

// R10–R12, R18
#[test]
fn remove_boundary_ages_survivors() {
    let mut c = Cache::new(2, 5).unwrap();
    c.put(31, 0).unwrap();
    c.get(31);
    c.get(31);
    c.put(32, 0).unwrap();
    c.remove(32);
    assert_eq!(c.snapshot()[0].frequency, 2);
    assert_eq!(c.pending(), 0);
}

// R5–R6, R21, R30
#[test]
fn large_eviction_sequence() {
    let mut c = Cache::new(40, 1000).unwrap();
    for k in (0..40).rev() {
        c.put(k, 0).unwrap();
    }
    assert_eq!(c.resize(0), (0..40).rev().collect::<Vec<_>>());
    assert!(c.snapshot().is_empty());
}

// R1–R2
#[test]
fn zero_period_with_enabled_capacity() {
    assert!(matches!(Cache::new(9, 0), Err(Error::ZeroPeriod)));
    let c = Cache::new(0, 1).unwrap();
    assert_eq!(c.capacity(), 0);
    assert_eq!(c.pending(), 0);
    assert!(c.snapshot().is_empty());
}
