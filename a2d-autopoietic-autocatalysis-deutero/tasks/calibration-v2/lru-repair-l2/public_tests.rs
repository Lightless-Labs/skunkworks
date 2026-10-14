// R2–R7, R13, R16
#[test]
fn lfu_basic_path() {
    let mut c = Cache::new(2, 100).unwrap();
    c.put(1, 10).unwrap();
    c.put(2, 20).unwrap();
    assert_eq!(c.get(1), Some(10));
    assert_eq!(c.put(3, 30), Ok(vec![2]));
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![3, 1]
    );
    assert_eq!(c.capacity(), 2);
}

// R8–R11, R19–R20
#[test]
fn age_and_pending() {
    let mut c = Cache::new(3, 10).unwrap();
    c.put(4, 40).unwrap();
    c.get(4);
    c.get(4);
    c.get(4);
    assert_eq!(c.pending(), 4);
    c.age(1);
    assert_eq!(c.snapshot()[0].frequency, 2);
    assert_eq!(c.pending(), 4);
}

// R15, R18, R21–R24
#[test]
fn update_remove_resize() {
    let mut c = Cache::new(4, 100).unwrap();
    c.put(8, 80).unwrap();
    assert_eq!(c.put(8, 81), Ok(vec![]));
    assert_eq!(c.remove(8), Some(81));
    c.put(9, 90).unwrap();
    c.put(10, 100).unwrap();
    assert_eq!(c.resize(1), vec![9]);
    assert_eq!(c.get(10), Some(100));
}

// R1–R2, R17
#[test]
fn constructor_and_disabled() {
    assert!(matches!(Cache::new(0, 0), Err(Error::ZeroPeriod)));
    let mut c = Cache::new(0, 3).unwrap();
    assert_eq!(c.put(1, 0), Err(Error::Disabled));
    assert_eq!(c.pending(), 0);
}
