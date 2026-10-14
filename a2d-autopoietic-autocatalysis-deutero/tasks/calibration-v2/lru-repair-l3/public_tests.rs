// R1–R6, R17–R19
#[test]
fn shards_local_eviction() {
    let mut c = Cache::new(vec![3, 4]).unwrap();
    c.insert(2, 20, 2, false).unwrap();
    c.insert(1, 10, 2, false).unwrap();
    assert_eq!(c.insert(4, 40, 2, false), Ok(vec![2]));
    assert_eq!(c.usage(), vec![2, 2]);
    assert_eq!(c.used(), 4);
    assert_eq!(c.capacities(), vec![3, 4]);
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![1, 4]
    );
}

// R9–R11, R20
#[test]
fn pins_and_removal() {
    let mut c = Cache::new(vec![5]).unwrap();
    c.insert(8, 80, 2, true).unwrap();
    assert_eq!(c.get(8), Some(80));
    assert_eq!(c.remove(8), Err(Error::Pinned { key: 8 }));
    c.set_pin(8, false).unwrap();
    assert_eq!(c.remove(8), Ok(80));
}

// R21–R29
#[test]
fn resize_and_trim() {
    let mut c = Cache::new(vec![9, 9]).unwrap();
    c.insert(2, 0, 2, false).unwrap();
    c.insert(1, 0, 3, false).unwrap();
    assert_eq!(c.trim(4), Ok(vec![2]));
    assert_eq!(c.resize(1, 2), Ok(vec![1]));
    assert_eq!(c.used(), 0);
}

// R1, R13–R16, R19
#[test]
fn validation_and_replacement() {
    assert!(matches!(Cache::new(vec![]), Err(Error::EmptyShards)));
    let mut c = Cache::new(vec![5]).unwrap();
    assert_eq!(c.insert(1, 0, 0, false), Err(Error::ZeroWeight));
    c.insert(1, 1, 2, false).unwrap();
    assert_eq!(c.insert(1, 2, 3, false), Ok(vec![]));
    assert_eq!(c.get(1), Some(2));
}
