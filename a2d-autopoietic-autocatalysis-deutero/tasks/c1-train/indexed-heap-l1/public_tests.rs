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

// R6-R7, R10, R13-R14
#[test]
fn public_fifo_and_detached_reads() {
    let mut q = IndexedHeap::new();
    q.push("Elm", 5, "ok").unwrap();
    q.push("Fir", 5, "no").unwrap();
    let mut snapshot = q.get(" ELM ").unwrap().unwrap();
    snapshot.value.clear();
    assert_eq!(q.push("elm", 1, ""), Err(Error::Duplicate { key: "elm".into() }));
    assert_eq!(q.peek().unwrap().value, "ok");
    assert_eq!(q.pop().unwrap().key, "elm");
    assert_eq!(q.pop().unwrap().key, "fir");
}

// R7-R8, R17-R20
#[test]
fn public_reuse_and_independence() {
    let mut q = IndexedHeap::new();
    let mut other = IndexedHeap::new();
    q.push("Grove", 0, "").unwrap();
    other.push("Grove", 9, "else").unwrap();
    assert_eq!(q.pop().unwrap().key, "grove");
    assert_eq!(q.get("grove"), Ok(None));
    q.push("GROVE", -1, "again").unwrap();
    assert_eq!(q.len(), 1);
    assert_eq!(other.peek().unwrap().priority, 9);
}
