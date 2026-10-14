// R1–R8, R26, R31–R32
#[test]
fn clean_lru_and_log() {
    let mut c = Cache::new(5);
    let mut f = |_, _| true;
    c.put(1, 10, 2, false, &mut f).unwrap();
    c.put(2, 20, 2, false, &mut f).unwrap();
    assert_eq!(c.get(1), Some(10));
    assert_eq!(c.get(9), None);
    assert_eq!(c.put(3, 30, 2, false, &mut f), Ok(vec![2]));
    assert_eq!(c.capacity(), 5);
    assert_eq!(c.used(), 4);
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![1, 3]
    );
    assert!(c.events().contains(&Event::Evict { key: 2 }));
    assert!(!c.drain_events().is_empty());
    assert!(c.events().is_empty());
}

// R10, R12, R16–R22
#[test]
fn dirty_flush_and_mark() {
    let mut c = Cache::new(8);
    let mut calls = vec![];
    let mut f = |k, v| {
        calls.push((k, v));
        true
    };
    c.put(4, 40, 3, true, &mut f).unwrap();
    assert_eq!(c.flush(1, &mut f), Ok(vec![4]));
    c.mark_dirty(4).unwrap();
    assert_eq!(c.flush(5, &mut f), Ok(vec![4]));
    assert_eq!(calls, vec![(4, 40), (4, 40)]);
    assert!(!c.snapshot()[0].dirty);
}

// R14–R15, R24–R26, R34–R38
#[test]
fn resize_evict_remove() {
    let mut c = Cache::new(10);
    let mut f = |_, _| true;
    c.put(8, 80, 3, true, &mut f).unwrap();
    c.put(9, 90, 3, false, &mut f).unwrap();
    assert_eq!(c.resize(4, &mut f), Ok(vec![8]));
    assert_eq!(c.remove(9), Ok(90));
    c.put(10, 100, 2, false, &mut f).unwrap();
    assert_eq!(c.evict_to(0, &mut f), Ok(vec![10]));
    assert_eq!(c.capacity(), 4);
}

// R9, R29–R32
#[test]
fn replacement_and_validation() {
    let mut c = Cache::new(5);
    let mut f = |_, _| true;
    assert_eq!(c.put(1, 0, 0, false, &mut f), Err(Error::ZeroWeight));
    c.put(1, 1, 2, false, &mut f).unwrap();
    assert_eq!(c.put(1, 2, 3, true, &mut f), Ok(vec![]));
    assert_eq!(c.get(1), Some(2));
    assert_eq!(c.remove(1), Err(Error::Dirty { key: 1 }));
}
