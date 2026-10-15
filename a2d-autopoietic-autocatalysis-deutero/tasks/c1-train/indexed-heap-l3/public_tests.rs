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

// R29-R36
#[test]
fn public_merge_policies() {
    let mut a = IndexedHeap::new();
    let mut b = IndexedHeap::new();
    a.push("Kauri", 10, "a").unwrap();
    b.push("KAURI", 2, "b").unwrap();
    b.push("Larch", 1, "c").unwrap();
    assert_eq!(a.merge(&mut b, CollisionPolicy::Reject), Err(Error::Collision { key: "kauri".into() }));
    let r = a.merge(&mut b, CollisionPolicy::TakeOther).unwrap();
    assert_eq!((r.moved, r.replaced, r.discarded), (1, 1, 0));
    assert!(b.is_empty());
    b.push("LARCH", 0, "d").unwrap();
    assert_eq!(a.merge(&mut b, CollisionPolicy::KeepSelf).unwrap().discarded, 1);
    assert_eq!(a.pop().unwrap().key, "larch");
    assert_eq!(a.pop().unwrap().value, "b");
}
