// R1, R3, R6–R8, R35
#[test]
fn empty_zero_and_misses() {
    let mut c = Cache::new(0);
    let mut f = |_, _| panic!("unexpected callback");
    assert_eq!(c.get(77), None);
    assert_eq!(c.events(), vec![Event::Miss { key: 77 }]);
    assert_eq!(c.evict_to(0, &mut f), Ok(vec![]));
    assert_eq!(c.used(), 0);
}

// R9, R28, R42
#[test]
fn validation_precedence_no_effects() {
    let mut c = Cache::new(4);
    let mut f = |_, _| true;
    c.put(31, 3, 2, true, &mut f).unwrap();
    let before = c.snapshot();
    let events = c.events();
    let mut never = |_, _| panic!("validation called flush");
    assert_eq!(c.put(31, 9, 0, false, &mut never), Err(Error::ZeroWeight));
    assert_eq!(
        c.put(31, 9, 5, false, &mut never),
        Err(Error::TooHeavy {
            weight: 5,
            capacity: 4
        })
    );
    assert_eq!(c.snapshot(), before);
    assert_eq!(c.events(), events);
}

// R13–R15, R42
#[test]
fn missing_and_dirty_remove_atomic() {
    let mut c = Cache::new(4);
    let mut f = |_, _| true;
    c.put(11, 3, 2, true, &mut f).unwrap();
    let before = c.snapshot();
    let events = c.events();
    assert_eq!(c.mark_dirty(12), Err(Error::Missing { key: 12 }));
    assert_eq!(c.remove(12), Err(Error::Missing { key: 12 }));
    assert_eq!(c.remove(11), Err(Error::Dirty { key: 11 }));
    assert_eq!(c.snapshot(), before);
    assert_eq!(c.events(), events);
}

// R6–R7, R11, R15, R32, R41
#[test]
fn exact_hit_miss_store_remove_log() {
    let mut c = Cache::new(4);
    let mut f = |_, _| true;
    c.put(91, -2, 2, false, &mut f).unwrap();
    c.get(91);
    c.get(92);
    assert_eq!(c.remove(91), Ok(-2));
    assert_eq!(
        c.events(),
        vec![
            Event::Store {
                key: 91,
                dirty: false
            },
            Event::Hit { key: 91 },
            Event::Miss { key: 92 },
            Event::Remove { key: 91 }
        ]
    );
}

// R5, R10, R21, R40
#[test]
fn dirty_order_independent_of_reads() {
    let mut c = Cache::new(9);
    let mut calls = vec![];
    let mut f = |k, v| {
        calls.push((k, v));
        true
    };
    c.put(31, 3, 2, true, &mut f).unwrap();
    c.put(21, 2, 2, true, &mut f).unwrap();
    c.get(31);
    assert_eq!(c.flush(9, &mut f), Ok(vec![31, 21]));
    assert_eq!(calls, vec![(31, 3), (21, 2)]);
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![21, 31]
    );
}

// R5, R12, R21, R40
#[test]
fn mark_dirty_reorders_only_dirty_order() {
    let mut c = Cache::new(9);
    let mut f = |_, _| true;
    c.put(31, 3, 2, true, &mut f).unwrap();
    c.put(21, 2, 2, true, &mut f).unwrap();
    c.mark_dirty(31).unwrap();
    assert_eq!(c.events().last(), Some(&Event::Dirty { key: 31 }));
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![31, 21]
    );
    assert_eq!(c.flush(9, &mut f), Ok(vec![21, 31]));
}

// R10, R29–R32, R39
#[test]
fn dirty_replacement_coalesces_and_reorders() {
    let mut c = Cache::new(9);
    let mut f = |_, _| true;
    c.put(31, 3, 2, true, &mut f).unwrap();
    c.put(21, 2, 2, true, &mut f).unwrap();
    let mut never = |_, _| panic!("coalesced data was flushed");
    assert_eq!(c.put(31, 30, 3, true, &mut never), Ok(vec![]));
    let mut calls = vec![];
    let mut collect = |k, v| {
        calls.push((k, v));
        true
    };
    assert_eq!(c.flush(9, &mut collect), Ok(vec![21, 31]));
    assert_eq!(calls, vec![(21, 2), (31, 30)]);
    assert_eq!(c.used(), 5);
}

// R16–R18, R21, R41
#[test]
fn flush_exact_event_sequence() {
    let mut c = Cache::new(8);
    let mut f = |_, _| true;
    c.put(51, -5, 2, true, &mut f).unwrap();
    c.drain_events();
    assert_eq!(c.flush(1, &mut f), Ok(vec![51]));
    assert_eq!(
        c.events(),
        vec![
            Event::FlushStart { key: 51, value: -5 },
            Event::FlushOk { key: 51 }
        ]
    );
    assert_eq!(c.snapshot()[0].weight, 2);
}

// R17, R19, R23, R42
#[test]
fn failed_flush_stays_dirty_and_logs() {
    let mut c = Cache::new(8);
    let mut yes = |_, _| true;
    c.put(61, 6, 2, true, &mut yes).unwrap();
    c.drain_events();
    let before = c.snapshot();
    let mut no = |_, _| false;
    assert_eq!(c.flush(9, &mut no), Err(Error::FlushFailed { key: 61 }));
    assert_eq!(c.snapshot(), before);
    assert_eq!(
        c.events(),
        vec![
            Event::FlushStart { key: 61, value: 6 },
            Event::FlushErr { key: 61 }
        ]
    );
}

// R19, R21, R23
#[test]
fn partial_flush_then_retry() {
    let mut c = Cache::new(9);
    let mut yes = |_, _| true;
    for k in [70, 50, 60] {
        c.put(k, k as i64, 2, true, &mut yes).unwrap();
    }
    let mut calls = vec![];
    let mut f = |k, _| {
        calls.push(k);
        k != 50
    };
    assert_eq!(c.flush(9, &mut f), Err(Error::FlushFailed { key: 50 }));
    assert_eq!(calls, vec![70, 50]);
    assert!(!c.snapshot()[0].dirty);
    assert!(c.snapshot()[1].dirty);
    assert_eq!(c.flush(9, &mut yes), Ok(vec![50, 60]));
}

// R20–R22
#[test]
fn zero_limit_and_clean_no_callbacks() {
    let mut c = Cache::new(5);
    let mut yes = |_, _| true;
    c.put(88, 0, 1, false, &mut yes).unwrap();
    c.put(89, 0, 1, true, &mut yes).unwrap();
    let events = c.events();
    let mut never = |_, _| panic!("unexpected flush");
    assert_eq!(c.flush(0, &mut never), Ok(vec![]));
    assert_eq!(c.events(), events);
    assert_eq!(c.flush(usize::MAX, &mut yes), Ok(vec![89]));
    assert_eq!(c.flush(usize::MAX, &mut never), Ok(vec![]));
}

// R20–R22
#[test]
fn flush_limit_counts_dirty_only() {
    let mut c = Cache::new(10);
    let mut f = |_, _| true;
    c.put(41, 0, 1, false, &mut f).unwrap();
    c.put(42, 0, 1, true, &mut f).unwrap();
    c.put(43, 0, 1, true, &mut f).unwrap();
    assert_eq!(c.flush(1, &mut f), Ok(vec![42]));
    assert_eq!(c.flush(1, &mut f), Ok(vec![43]));
    assert_eq!(c.snapshot().len(), 3);
}

// R12, R24–R26
#[test]
fn eviction_uses_lru_not_dirty_order() {
    let mut c = Cache::new(6);
    let mut f = |_, _| true;
    c.put(81, 1, 2, true, &mut f).unwrap();
    c.put(82, 2, 2, true, &mut f).unwrap();
    c.mark_dirty(81).unwrap();
    c.drain_events();
    assert_eq!(c.evict_to(2, &mut f), Ok(vec![81]));
    assert_eq!(
        c.events(),
        vec![
            Event::FlushStart { key: 81, value: 1 },
            Event::FlushOk { key: 81 },
            Event::Evict { key: 81 }
        ]
    );
}

// R20, R26, R34
#[test]
fn clean_eviction_no_flush_events() {
    let mut c = Cache::new(6);
    let mut yes = |_, _| true;
    c.put(77, 0, 2, false, &mut yes).unwrap();
    c.drain_events();
    let mut never = |_, _| panic!("clean callback");
    assert_eq!(c.evict_to(0, &mut never), Ok(vec![77]));
    assert_eq!(c.events(), vec![Event::Evict { key: 77 }]);
}

// R19, R25–R27
#[test]
fn failed_eviction_has_no_evict_event() {
    let mut c = Cache::new(5);
    let mut yes = |_, _| true;
    c.put(71, 7, 2, true, &mut yes).unwrap();
    c.put(72, 8, 2, false, &mut yes).unwrap();
    c.drain_events();
    let before = c.snapshot();
    let mut no = |_, _| false;
    assert_eq!(c.evict_to(0, &mut no), Err(Error::FlushFailed { key: 71 }));
    assert_eq!(c.snapshot(), before);
    assert_eq!(
        c.events(),
        vec![
            Event::FlushStart { key: 71, value: 7 },
            Event::FlushErr { key: 71 }
        ]
    );
}

// R26–R27, R34, R42
#[test]
fn partial_eviction_then_failure() {
    let mut c = Cache::new(9);
    let mut yes = |_, _| true;
    c.put(1, 1, 2, false, &mut yes).unwrap();
    c.put(2, 2, 2, true, &mut yes).unwrap();
    c.put(3, 3, 2, true, &mut yes).unwrap();
    c.drain_events();
    let mut no = |_, _| false;
    assert_eq!(c.evict_to(0, &mut no), Err(Error::FlushFailed { key: 2 }));
    assert_eq!(c.used(), 4);
    assert_eq!(
        c.events(),
        vec![
            Event::Evict { key: 1 },
            Event::FlushStart { key: 2, value: 2 },
            Event::FlushErr { key: 2 }
        ]
    );
}

// R11, R16, R28, R30–R32
#[test]
fn clean_replacement_flushes_old_value() {
    let mut c = Cache::new(6);
    let mut yes = |_, _| true;
    c.put(71, 7, 2, true, &mut yes).unwrap();
    c.drain_events();
    let mut calls = vec![];
    let mut f = |k, v| {
        calls.push((k, v));
        true
    };
    assert_eq!(c.put(71, 70, 3, false, &mut f), Ok(vec![]));
    assert_eq!(calls, vec![(71, 7)]);
    assert_eq!(
        c.events(),
        vec![
            Event::FlushStart { key: 71, value: 7 },
            Event::FlushOk { key: 71 },
            Event::Store {
                key: 71,
                dirty: false
            }
        ]
    );
    assert!(!c.snapshot()[0].dirty);
}

// R19, R28, R33
#[test]
fn clean_replacement_failure_preserves_old() {
    let mut c = Cache::new(6);
    let mut yes = |_, _| true;
    c.put(71, 7, 2, true, &mut yes).unwrap();
    c.put(72, 8, 2, false, &mut yes).unwrap();
    let before = c.snapshot();
    let mut no = |_, _| false;
    assert_eq!(
        c.put(71, 70, 3, false, &mut no),
        Err(Error::FlushFailed { key: 71 })
    );
    assert_eq!(c.snapshot(), before);
    assert_eq!(c.used(), 4);
}

// R30–R32, R39
#[test]
fn replacement_excluded_from_eviction() {
    let mut c = Cache::new(6);
    let mut yes = |_, _| true;
    c.put(80, 8, 2, true, &mut yes).unwrap();
    c.put(90, 9, 2, false, &mut yes).unwrap();
    let mut never = |_, _| panic!("replacement evicted");
    assert_eq!(c.put(80, 80, 5, true, &mut never), Ok(vec![90]));
    assert_eq!(c.used(), 5);
    assert_eq!(c.snapshot()[0].value, 80);
}

// R19, R27, R29–R30, R33
#[test]
fn failed_put_preserves_old_dirty_value() {
    let mut c = Cache::new(6);
    let mut yes = |_, _| true;
    c.put(20, 2, 2, true, &mut yes).unwrap();
    c.put(30, 3, 2, true, &mut yes).unwrap();
    c.put(40, 4, 2, false, &mut yes).unwrap();
    c.drain_events();
    let before = c.snapshot();
    let mut no = |_, _| false;
    assert_eq!(
        c.put(40, 40, 5, true, &mut no),
        Err(Error::FlushFailed { key: 20 })
    );
    assert_eq!(c.snapshot(), before);
    assert_eq!(
        c.events(),
        vec![
            Event::FlushStart { key: 20, value: 2 },
            Event::FlushErr { key: 20 }
        ]
    );
}

// R28, R30, R33, R42
#[test]
fn preliminary_flush_commits_on_later_failure() {
    let mut c = Cache::new(6);
    let mut yes = |_, _| true;
    c.put(10, 1, 2, true, &mut yes).unwrap();
    c.put(20, 2, 2, true, &mut yes).unwrap();
    c.put(30, 3, 2, false, &mut yes).unwrap();
    c.drain_events();
    let mut calls = vec![];
    let mut f = |k, _| {
        calls.push(k);
        k == 10
    };
    assert_eq!(
        c.put(10, 100, 5, false, &mut f),
        Err(Error::FlushFailed { key: 20 })
    );
    assert_eq!(calls, vec![10, 20]);
    let s = c.snapshot();
    assert_eq!(s[0].value, 1);
    assert!(!s[0].dirty);
    assert!(s[1].dirty);
    assert_eq!(c.used(), 6);
    assert_eq!(
        c.events(),
        vec![
            Event::FlushStart { key: 10, value: 1 },
            Event::FlushOk { key: 10 },
            Event::FlushStart { key: 20, value: 2 },
            Event::FlushErr { key: 20 }
        ]
    );
}

// R27, R31–R33
#[test]
fn new_put_partial_eviction_progress() {
    let mut c = Cache::new(6);
    let mut yes = |_, _| true;
    c.put(10, 1, 2, false, &mut yes).unwrap();
    c.put(20, 2, 2, true, &mut yes).unwrap();
    c.put(30, 3, 2, false, &mut yes).unwrap();
    let mut no = |_, _| false;
    assert_eq!(
        c.put(40, 4, 5, true, &mut no),
        Err(Error::FlushFailed { key: 20 })
    );
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![20, 30]
    );
    assert_eq!(c.used(), 4);
    assert!(!c.events().contains(&Event::Store {
        key: 40,
        dirty: true
    }));
}

// R19, R36–R37, R42
#[test]
fn resize_failure_retains_capacity() {
    let mut c = Cache::new(8);
    let mut yes = |_, _| true;
    c.put(11, 1, 2, false, &mut yes).unwrap();
    c.put(12, 2, 2, true, &mut yes).unwrap();
    c.put(13, 3, 2, false, &mut yes).unwrap();
    c.drain_events();
    let mut no = |_, _| false;
    assert_eq!(c.resize(1, &mut no), Err(Error::FlushFailed { key: 12 }));
    assert_eq!(c.capacity(), 8);
    assert_eq!(c.used(), 4);
    assert!(!c.events().iter().any(|e| matches!(e, Event::Resize { .. })));
}

// R26, R36, R41
#[test]
fn resize_success_logs_last() {
    let mut c = Cache::new(6);
    let mut f = |_, _| true;
    c.put(24, 2, 2, true, &mut f).unwrap();
    c.put(25, 3, 2, false, &mut f).unwrap();
    c.drain_events();
    assert_eq!(c.resize(0, &mut f), Ok(vec![24, 25]));
    assert_eq!(
        c.events(),
        vec![
            Event::FlushStart { key: 24, value: 2 },
            Event::FlushOk { key: 24 },
            Event::Evict { key: 24 },
            Event::Evict { key: 25 },
            Event::Resize { capacity: 0 }
        ]
    );
}

// R38
#[test]
fn resize_growth_and_equal() {
    let mut c = Cache::new(6);
    let mut f = |_, _| true;
    c.put(11, 0, 2, true, &mut f).unwrap();
    c.drain_events();
    let mut never = |_, _| panic!("growth callback");
    c.resize(10, &mut never).unwrap();
    c.resize(10, &mut never).unwrap();
    assert_eq!(
        c.events(),
        vec![
            Event::Resize { capacity: 10 },
            Event::Resize { capacity: 10 }
        ]
    );
    assert!(c.snapshot()[0].dirty);
}

// R31, R34–R35
#[test]
fn exact_boundary_and_overshoot() {
    let mut c = Cache::new(10);
    let mut f = |_, _| true;
    c.put(51, 1, 3, false, &mut f).unwrap();
    c.put(52, 2, 3, false, &mut f).unwrap();
    assert_eq!(c.evict_to(3, &mut f), Ok(vec![51]));
    assert_eq!(c.used(), 3);
    c.put(53, 3, 2, false, &mut f).unwrap();
    assert_eq!(c.evict_to(4, &mut f), Ok(vec![52]));
    assert_eq!(c.used(), 2);
    assert_eq!(c.capacity(), 10);
}

// R2–R3, R30–R31, R45
#[test]
fn full_u64_capacity_and_values() {
    let mut c = Cache::new(u64::MAX);
    let mut f = |_, _| true;
    c.put(u64::MAX, i64::MIN, u64::MAX, false, &mut f).unwrap();
    assert_eq!(
        c.put(0, i64::MAX, u64::MAX, false, &mut f),
        Ok(vec![u64::MAX])
    );
    assert_eq!(c.used(), u64::MAX);
    assert_eq!(c.get(0), Some(i64::MAX));
}

// R12, R18, R21, R43
#[test]
fn rejoin_dirty_order_after_flush() {
    let mut c = Cache::new(10);
    let mut f = |_, _| true;
    for k in [81, 82, 83] {
        c.put(k, 0, 1, true, &mut f).unwrap();
    }
    c.flush(1, &mut f).unwrap();
    c.mark_dirty(81).unwrap();
    assert_eq!(c.flush(10, &mut f), Ok(vec![82, 83, 81]));
    assert!(c.snapshot().iter().all(|e| !e.dirty));
}

// R4, R8, R44
#[test]
fn drain_and_owned_vectors() {
    let mut c = Cache::new(5);
    let mut f = |_, _| true;
    c.put(55, 5, 2, true, &mut f).unwrap();
    let mut s = c.snapshot();
    s[0].dirty = false;
    let mut events = c.events();
    events.clear();
    let drained = c.drain_events();
    assert_eq!(
        drained,
        vec![Event::Store {
            key: 55,
            dirty: true
        }]
    );
    assert!(c.events().is_empty());
    assert!(c.snapshot()[0].dirty);
    assert_eq!(c.flush(1, &mut f), Ok(vec![55]));
}

// R44
#[test]
fn clone_independent_orders_and_logs() {
    let mut a = Cache::new(9);
    let mut f = |_, _| true;
    a.put(81, 0, 1, true, &mut f).unwrap();
    a.put(82, 0, 1, true, &mut f).unwrap();
    let mut b = a.clone();
    b.get(81);
    b.mark_dirty(81).unwrap();
    b.flush(1, &mut f).unwrap();
    b.drain_events();
    assert_eq!(a.flush(1, &mut f), Ok(vec![81]));
    assert_eq!(
        a.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![81, 82]
    );
    assert!(!a.events().is_empty());
}

// R18, R24, R40
#[test]
fn successful_flush_does_not_touch_lru() {
    let mut c = Cache::new(6);
    let mut f = |_, _| true;
    c.put(11, 0, 2, true, &mut f).unwrap();
    c.put(12, 0, 2, false, &mut f).unwrap();
    c.flush(1, &mut f).unwrap();
    assert_eq!(c.evict_to(2, &mut f), Ok(vec![11]));
}

// R19, R23, R40, R43
#[test]
fn failure_retry_preserves_write_order() {
    let mut c = Cache::new(9);
    let mut f = |_, _| true;
    for k in [31, 32, 33] {
        c.put(k, 0, 1, true, &mut f).unwrap();
    }
    c.mark_dirty(31).unwrap();
    let mut no = |_, _| false;
    assert_eq!(c.flush(9, &mut no), Err(Error::FlushFailed { key: 32 }));
    assert_eq!(c.flush(9, &mut f), Ok(vec![32, 33, 31]));
}

// R3–R5, R26, R39, R43, R45
#[test]
fn larger_mixed_accounting() {
    let mut c = Cache::new(100);
    let mut f = |_, _| true;
    for k in 0..40 {
        c.put(k, k as i64, 2, k % 2 == 0, &mut f).unwrap();
    }
    for k in 0..20 {
        c.mark_dirty(k).unwrap();
    }
    c.flush(8, &mut f).unwrap();
    c.evict_to(30, &mut f).unwrap();
    assert_eq!(c.used(), 30);
    assert_eq!(c.snapshot().iter().map(|e| e.weight).sum::<u64>(), 30);
    assert_eq!(c.flush(usize::MAX, &mut f), Ok(vec![36, 38]));
    assert_eq!(c.snapshot().len(), 15);
}

// R19–R20, R34–R35
#[test]
fn nonempty_evict_noop_does_not_flush() {
    let mut c = Cache::new(9);
    let mut yes = |_, _| true;
    c.put(99, 9, 3, true, &mut yes).unwrap();
    let before = c.snapshot();
    let events = c.events();
    let mut never = |_, _| panic!("no-op callback");
    assert_eq!(c.evict_to(3, &mut never), Ok(vec![]));
    assert_eq!(c.evict_to(u64::MAX, &mut never), Ok(vec![]));
    assert_eq!(c.snapshot(), before);
    assert_eq!(c.events(), events);
}

// R16–R18, R24–R26, R31–R32, R41
#[test]
fn put_multiple_victims_complete_event_order() {
    let mut c = Cache::new(8);
    let mut f = |_, _| true;
    c.put(91, 1, 2, true, &mut f).unwrap();
    c.put(92, 2, 2, false, &mut f).unwrap();
    c.put(93, 3, 2, true, &mut f).unwrap();
    c.drain_events();
    assert_eq!(c.put(94, 4, 7, false, &mut f), Ok(vec![91, 92, 93]));
    assert_eq!(
        c.events(),
        vec![
            Event::FlushStart { key: 91, value: 1 },
            Event::FlushOk { key: 91 },
            Event::Evict { key: 91 },
            Event::Evict { key: 92 },
            Event::FlushStart { key: 93, value: 3 },
            Event::FlushOk { key: 93 },
            Event::Evict { key: 93 },
            Event::Store {
                key: 94,
                dirty: false
            }
        ]
    );
    assert_eq!(c.used(), 7);
}
