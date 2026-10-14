// R1, R13
#[test]
fn zero_shard_is_valid() {
    let mut c = Cache::new(vec![0, 1]).unwrap();
    assert_eq!(
        c.insert(2, 0, 1, false),
        Err(Error::TooHeavy {
            weight: 1,
            capacity: 0
        })
    );
    assert_eq!(c.insert(3, 0, 1, false), Ok(vec![]));
}

// R2–R3, R6
#[test]
fn modulo_full_key() {
    let mut c = Cache::new(vec![3, 3, 3]).unwrap();
    c.insert(u64::MAX, i64::MIN, 1, false).unwrap();
    assert_eq!(c.snapshot()[0].shard, 0);
    assert_eq!(c.get(u64::MAX), Some(i64::MIN));
}

// R3–R5, R15
#[test]
fn total_exceeds_u64() {
    let mut c = Cache::new(vec![u64::MAX, u64::MAX]).unwrap();
    c.insert(0, 0, u64::MAX, false).unwrap();
    c.insert(1, 0, u64::MAX, false).unwrap();
    assert_eq!(c.used(), 2 * u64::MAX as u128);
    assert_eq!(c.trim(u64::MAX as u128), Ok(vec![0]));
}

// R8–R10, R27, R33
#[test]
fn get_pinned_retains_touch_after_unpin() {
    let mut c = Cache::new(vec![4, 4]).unwrap();
    c.insert(2, 2, 1, true).unwrap();
    c.insert(1, 1, 1, false).unwrap();
    c.get(2);
    c.set_pin(2, false).unwrap();
    assert_eq!(c.trim(1), Ok(vec![1]));
}

// R10, R33
#[test]
fn pin_changes_do_not_touch() {
    let mut c = Cache::new(vec![4]).unwrap();
    for k in [10, 20, 30] {
        c.insert(k, 0, 1, false).unwrap();
    }
    c.set_pin(10, true).unwrap();
    c.set_pin(10, false).unwrap();
    assert_eq!(c.trim(2), Ok(vec![10]));
}

// R10
#[test]
fn redundant_pin_no_touch() {
    let mut c = Cache::new(vec![3]).unwrap();
    c.insert(80, 0, 1, false).unwrap();
    c.insert(90, 0, 1, false).unwrap();
    c.set_pin(80, false).unwrap();
    assert_eq!(c.trim(1), Ok(vec![80]));
}

// R10–R12
#[test]
fn missing_pin_and_remove() {
    let mut c = Cache::new(vec![3]).unwrap();
    assert_eq!(c.set_pin(55, true), Err(Error::Missing { key: 55 }));
    assert_eq!(c.remove(55), Err(Error::Missing { key: 55 }));
    assert!(c.snapshot().is_empty());
}

// R11–R12, R20
#[test]
fn pinned_remove_atomic() {
    let mut c = Cache::new(vec![3]).unwrap();
    c.insert(51, -2, 2, true).unwrap();
    let before = c.snapshot();
    assert_eq!(c.remove(51), Err(Error::Pinned { key: 51 }));
    assert_eq!(c.snapshot(), before);
    assert_eq!(c.usage(), vec![2]);
}

// R12–R13
#[test]
fn zero_weight_precedence() {
    let mut c = Cache::new(vec![0]).unwrap();
    assert_eq!(c.insert(11, 0, 0, true), Err(Error::ZeroWeight));
    assert!(c.snapshot().is_empty());
}

// R13
#[test]
fn too_heavy_precedes_blocked() {
    let mut c = Cache::new(vec![2]).unwrap();
    c.insert(1, 0, 2, true).unwrap();
    assert_eq!(
        c.insert(2, 0, 3, false),
        Err(Error::TooHeavy {
            weight: 3,
            capacity: 2
        })
    );
}

// R12–R13, R31
#[test]
fn blocked_insert_atomic() {
    let mut c = Cache::new(vec![5]).unwrap();
    c.insert(10, 10, 3, true).unwrap();
    c.insert(20, 20, 2, false).unwrap();
    let before = c.snapshot();
    assert_eq!(c.insert(20, 99, 3, true), Err(Error::Blocked { shard: 0 }));
    assert_eq!(c.snapshot(), before);
    assert_eq!(c.used(), 5);
}

// R14–R16, R20
#[test]
fn pinned_replacement_credits_old() {
    let mut c = Cache::new(vec![4]).unwrap();
    c.insert(10, 10, 4, true).unwrap();
    assert_eq!(c.insert(10, 11, 3, false), Ok(vec![]));
    assert_eq!(c.snapshot()[0].pinned, false);
    assert_eq!(c.remove(10), Ok(11));
}

// R14–R15
#[test]
fn pinned_replacement_other_pins() {
    let mut c = Cache::new(vec![6]).unwrap();
    c.insert(7, 1, 3, true).unwrap();
    c.insert(8, 2, 3, true).unwrap();
    assert_eq!(c.insert(7, 3, 3, true), Ok(vec![]));
    assert_eq!(c.used(), 6);
}

// R8, R17–R19
#[test]
fn local_oldest_not_newest() {
    let mut c = Cache::new(vec![5, 5]).unwrap();
    for k in [8, 6, 4] {
        c.insert(k, 0, 1, false).unwrap();
    }
    assert_eq!(c.insert(2, 0, 4, false), Ok(vec![8, 6]));
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![4, 2]
    );
}

// R17, R20
#[test]
fn local_skips_pinned() {
    let mut c = Cache::new(vec![5]).unwrap();
    c.insert(40, 0, 2, true).unwrap();
    c.insert(30, 0, 2, false).unwrap();
    assert_eq!(c.insert(20, 0, 3, false), Ok(vec![30]));
    assert_eq!(c.used(), 5);
}

// R13, R18
#[test]
fn no_cross_shard_borrowing() {
    let mut c = Cache::new(vec![2, 100]).unwrap();
    assert_eq!(
        c.insert(8, 0, 3, false),
        Err(Error::TooHeavy {
            weight: 3,
            capacity: 2
        })
    );
    c.insert(1, 0, 1, false).unwrap();
    c.insert(2, 0, 2, false).unwrap();
    assert_eq!(c.insert(4, 0, 2, false), Ok(vec![2]));
    assert_eq!(c.get(1), Some(0));
}

// R12, R21
#[test]
fn resize_bad_id_precedence() {
    let mut c = Cache::new(vec![3]).unwrap();
    c.insert(4, 0, 3, true).unwrap();
    assert_eq!(
        c.resize(usize::MAX, 0),
        Err(Error::BadShard { shard: usize::MAX })
    );
    assert_eq!(c.capacities(), vec![3]);
}

// R12, R22
#[test]
fn resize_pinned_capacity_atomic() {
    let mut c = Cache::new(vec![8]).unwrap();
    c.insert(31, 0, 2, false).unwrap();
    c.insert(32, 0, 5, true).unwrap();
    let before = c.snapshot();
    assert_eq!(
        c.resize(0, 4),
        Err(Error::PinnedCapacity {
            minimum: 5,
            target: 4
        })
    );
    assert_eq!(c.snapshot(), before);
    assert_eq!(c.capacities(), vec![8]);
}

// R23–R25, R30
#[test]
fn resize_exact_pinned_weight() {
    let mut c = Cache::new(vec![9]).unwrap();
    c.insert(61, 0, 3, true).unwrap();
    c.insert(62, 0, 2, false).unwrap();
    c.insert(63, 0, 2, false).unwrap();
    assert_eq!(c.resize(0, 3), Ok(vec![62, 63]));
    assert_eq!(c.usage(), vec![3]);
}

// R23–R24
#[test]
fn resize_preserves_other_shard() {
    let mut c = Cache::new(vec![6, 6]).unwrap();
    for k in [5, 4, 3, 2] {
        c.insert(k, 0, 2, false).unwrap();
    }
    assert_eq!(c.resize(0, 2), Ok(vec![4]));
    assert_eq!(c.capacities(), vec![2, 6]);
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![5, 3, 2]
    );
}

// R25
#[test]
fn resize_zero_empty() {
    let mut c = Cache::new(vec![3]).unwrap();
    c.insert(30, 0, 2, false).unwrap();
    assert_eq!(c.resize(0, 0), Ok(vec![30]));
    assert_eq!(c.used(), 0);
}

// R12, R26
#[test]
fn global_pinned_minimum() {
    let mut c = Cache::new(vec![5, 5]).unwrap();
    c.insert(8, 0, 2, true).unwrap();
    c.insert(9, 0, 3, true).unwrap();
    c.insert(10, 0, 1, false).unwrap();
    let before = c.snapshot();
    assert_eq!(
        c.trim(4),
        Err(Error::PinnedCapacity {
            minimum: 5,
            target: 4
        })
    );
    assert_eq!(c.snapshot(), before);
}

// R27–R30
#[test]
fn trim_exact_boundary_stops() {
    let mut c = Cache::new(vec![4, 4]).unwrap();
    c.insert(4, 0, 2, false).unwrap();
    c.insert(5, 0, 2, false).unwrap();
    assert_eq!(c.trim(2), Ok(vec![4]));
    assert_eq!(c.used(), 2);
    assert_eq!(c.capacities(), vec![4, 4]);
}

// R6, R27
#[test]
fn global_order_across_shards() {
    let mut c = Cache::new(vec![4, 4, 4]).unwrap();
    for k in [11, 9, 10, 8] {
        c.insert(k, 0, 1, false).unwrap();
    }
    c.get(11);
    assert_eq!(c.trim(1), Ok(vec![9, 10, 8]));
}

// R20, R27–R28
#[test]
fn global_skips_pins_and_overshoots() {
    let mut c = Cache::new(vec![10, 10]).unwrap();
    c.insert(2, 0, 3, true).unwrap();
    c.insert(1, 0, 5, false).unwrap();
    c.insert(4, 0, 2, false).unwrap();
    assert_eq!(c.trim(7), Ok(vec![1]));
    assert_eq!(c.used(), 5);
}

// R29
#[test]
fn trim_noop_and_max() {
    let mut c = Cache::new(vec![1]).unwrap();
    c.insert(7, 0, 1, true).unwrap();
    let before = c.snapshot();
    assert_eq!(c.trim(u128::MAX), Ok(vec![]));
    assert_eq!(c.trim(1), Ok(vec![]));
    assert_eq!(c.snapshot(), before);
}

// R8, R16, R19, R32
#[test]
fn replacement_updates_recency_and_pin() {
    let mut c = Cache::new(vec![3]).unwrap();
    c.insert(30, 0, 1, false).unwrap();
    c.insert(20, 0, 1, false).unwrap();
    c.insert(30, -1, 1, true).unwrap();
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![20, 30]
    );
    assert_eq!(c.trim(1), Ok(vec![20]));
}

// R4, R6–R7
#[test]
fn returned_copies_are_independent() {
    let mut c = Cache::new(vec![3]).unwrap();
    c.insert(11, 2, 1, false).unwrap();
    let mut caps = c.capacities();
    caps[0] = 0;
    let mut entries = c.snapshot();
    entries[0].pinned = true;
    assert_eq!(c.capacities(), vec![3]);
    assert!(!c.snapshot()[0].pinned);
}

// R34
#[test]
fn clone_is_independent() {
    let mut a = Cache::new(vec![3, 3]).unwrap();
    a.insert(6, 0, 2, false).unwrap();
    let mut b = a.clone();
    b.set_pin(6, true).unwrap();
    b.resize(1, 0).unwrap();
    assert!(!a.snapshot()[0].pinned);
    assert_eq!(a.capacities(), vec![3, 3]);
}

// R7–R9, R35
#[test]
fn miss_no_order_change() {
    let mut c = Cache::new(vec![3]).unwrap();
    c.insert(80, 0, 1, false).unwrap();
    c.insert(90, 0, 1, false).unwrap();
    assert_eq!(c.get(100), None);
    assert_eq!(c.trim(1), Ok(vec![80]));
}

// R5, R23, R27, R32, R35
#[test]
fn large_accounting_after_local_and_global() {
    let mut c = Cache::new(vec![100; 4]).unwrap();
    for k in 0..80 {
        c.insert(k, k as i64, 2, k % 10 == 0).unwrap();
    }
    c.resize(2, 20).unwrap();
    c.trim(40).unwrap();
    let mut sums = vec![0u64; 4];
    for e in c.snapshot() {
        sums[e.shard] += e.weight;
    }
    assert_eq!(c.usage(), sums);
    assert_eq!(c.used(), sums.iter().map(|n| *n as u128).sum::<u128>());
    assert!(c.used() <= 40);
}

// R1
#[test]
fn empty_capacity_vector_rejected() {
    assert!(matches!(Cache::new(vec![]), Err(Error::EmptyShards)));
    let c = Cache::new(vec![0, 0, 0]).unwrap();
    assert_eq!(c.usage(), vec![0, 0, 0]);
    assert_eq!(c.used(), 0);
}
