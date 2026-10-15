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

// R10, R19, R23-R24, R28
#[test]
fn public_direction_error() {
    let mut q = IndexedHeap::new();
    q.push("Juniper", 4, "v").unwrap();
    assert_eq!(q.decrease_key("juniper", 9), Err(Error::WrongDirection { current: 4, requested: 9 }));
    assert_eq!(q.push("JUNIPER", 0, ""), Err(Error::Duplicate { key: "juniper".into() }));
    assert_eq!(q.pop().unwrap().priority, 4);
}
