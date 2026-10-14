// R1–R3, R11, R15–R16
#[test]
fn basic_weighted_lru() {
    let mut c = Cache::new(5);
    assert_eq!(c.insert(1, 10, 2, None, 1), Ok(vec![]));
    c.insert(2, 20, 2, None, 2).unwrap();
    assert_eq!(c.get(1, 3), Ok(Some(10)));
    assert_eq!(c.insert(3, 30, 2, None, 4), Ok(vec![2]));
    assert_eq!(c.used(), 4);
    assert_eq!(c.capacity(), 5);
    assert_eq!(c.clock(), 4);
    assert_eq!(
        c.snapshot().iter().map(|e| e.key).collect::<Vec<_>>(),
        vec![1, 3]
    );
}

// R7–R10
#[test]
fn ttl_and_advance() {
    let mut c = Cache::new(9);
    c.insert(4, 40, 3, Some(5), 1).unwrap();
    assert_eq!(c.advance(7), Ok(vec![4]));
    assert_eq!(c.get(4, 7), Ok(None));
}

// R12–R13, R18
#[test]
fn replace_remove_resize() {
    let mut c = Cache::new(10);
    c.insert(1, 5, 2, None, 0).unwrap();
    c.insert(1, 9, 4, None, 1).unwrap();
    c.insert(2, 8, 3, None, 1).unwrap();
    assert_eq!(c.resize(4, 2), Ok(vec![1]));
    assert_eq!(c.remove(2, 3), Ok(Some(8)));
    assert_eq!(c.used(), 0);
}

// R4–R6
#[test]
fn validation_basics() {
    let mut c = Cache::new(2);
    assert_eq!(c.insert(1, 0, 0, None, 0), Err(Error::ZeroWeight));
    assert_eq!(
        c.insert(1, 0, 3, None, 0),
        Err(Error::TooHeavy {
            weight: 3,
            capacity: 2
        })
    );
    c.advance(10).unwrap();
    assert_eq!(c.get(2, 9), Err(Error::BackwardTime { last: 10, now: 9 }));
}
