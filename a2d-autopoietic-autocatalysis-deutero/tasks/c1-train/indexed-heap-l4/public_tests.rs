// R1-R5, R8, R11-R13, R19
#[test]
fn public_empty_and_lookup() {
    let mut q = IndexedHeap::new();
    assert_eq!(q.len(), 0);
    assert!(q.is_empty());
    assert_eq!(q.peek(), None);
    assert_eq!(q.pop(), None);
    assert_eq!(q.get("Absent"), Ok(None));
    assert_eq!(q.push("   ", 4, "ignored"), Err(Error::EmptyKey));
}

// R5-R12, R15-R17
#[test]
fn public_rank_and_canonical_keys() {
    let mut q = IndexedHeap::new();
    q.push(" Birch ", 7, "aa").unwrap();
    q.push("CEDAR", -3, "long").unwrap();
    q.push("Dogwood", 7, "z").unwrap();
    assert_eq!(q.peek().unwrap().key, "cedar");
    assert_eq!(q.pop().unwrap().priority, -3);
    assert_eq!(q.pop().unwrap().key, "dogwood");
    assert_eq!(q.pop().unwrap().key, "birch");
    assert!(q.is_empty());
}

// R21-R28
#[test]
fn public_keyed_updates() {
    let mut q = IndexedHeap::new();
    q.push("Hazel", 8, "old").unwrap();
    q.push("Ivy", 3, "x").unwrap();
    assert_eq!(q.decrease_key(" HAZEL ", 1), Ok(8));
    assert_eq!(q.peek().unwrap().key, "hazel");
    assert_eq!(q.increase_key("hazel", 12), Ok(1));
    assert_eq!(q.replace_value("ivy", "new"), Ok("x".into()));
    assert_eq!(q.remove("IVY").unwrap().value, "new");
    assert_eq!(q.remove("missing"), Err(Error::Missing { key: "missing".into() }));
    assert_eq!(q.len(), 1);
}

// R29-R32, R37-R39, R43, R45-R48
#[test]
fn public_bounds_merge_and_snapshot() {
    let mut q = IndexedHeap::with_capacity(2);
    assert_eq!(q.capacity(), Some(2));
    assert_eq!(q.push("Maple", 8, "a"), Ok(Admission::Inserted));
    q.push("Nettle", 2, "b").unwrap();
    match q.push("Oak", 1, "c").unwrap() {
        Admission::Evicted(item) => assert_eq!(item.key, "maple"),
        other => panic!("unexpected {other:?}"),
    }
    let snapshot = q.iter();
    let mut donor = IndexedHeap::new();
    donor.push("Pine", 0, "d").unwrap();
    let report = q.merge(&mut donor, CollisionPolicy::KeepSelf).unwrap();
    assert_eq!((report.moved, report.replaced, report.discarded), (1, 0, 0));
    assert_eq!(report.evicted[0].key, "nettle");
    assert_eq!(snapshot.map(|i| i.key).collect::<Vec<_>>(), vec!["oak", "nettle"]);
    assert_eq!(q.set_capacity(Some(1))[0].key, "oak");
    assert!(q.set_capacity(None).is_empty());
    assert_eq!(q.capacity(), None);
}
