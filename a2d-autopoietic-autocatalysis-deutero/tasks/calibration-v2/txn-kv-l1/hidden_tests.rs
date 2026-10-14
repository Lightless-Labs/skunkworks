// R1, R2–R5, R13
#[test]
fn h_empty() {
    let mut s = Store::new();
    assert_eq!(s.get("absent"), Ok(None));
    assert_eq!(s.delete("absent"), Ok(false));
    assert_eq!(s.count(""), Ok(0));
}

// R3–R5
#[test]
fn h_overwrite() {
    let mut s = Store::new();
    s.set("unit", "old").unwrap();
    s.set("unit", "new").unwrap();
    assert_eq!(s.count("old"), Ok(0));
    assert_eq!(s.count("new"), Ok(1));
}

// R2, R5, R13
#[test]
fn h_empty_values() {
    let mut s = Store::new();
    for k in ["a", "b", "c"] {
        s.set(k, "").unwrap();
    }
    assert_eq!(s.count(""), Ok(3));
}

// R13–R14
#[test]
fn h_key_errors() {
    let mut s = Store::new();
    assert_eq!(s.set("", "v"), Err(Error::KeyEmpty));
    assert_eq!(s.get(""), Err(Error::KeyEmpty));
    assert_eq!(s.delete(""), Err(Error::KeyEmpty));
    assert_eq!(s.count("v"), Ok(0));
}

// R11–R16
#[test]
fn h_names() {
    let mut s = Store::new();
    assert_eq!(s.snapshot(""), Err(Error::NameEmpty));
    assert_eq!(s.restore(""), Err(Error::NameEmpty));
    assert_eq!(s.forget(""), Err(Error::NameEmpty));
    assert_eq!(
        s.restore("lost"),
        Err(Error::SnapshotMissing("lost".into()))
    );
    assert_eq!(s.forget("lost"), Err(Error::SnapshotMissing("lost".into())));
    for (name, at) in [
        ("A", 0),
        ("2a", 0),
        ("a-", 1),
        ("aé", 1),
        ("é", 0),
        ("aa b", 2),
    ] {
        assert_eq!(s.snapshot(name), Err(Error::NameInvalid { at }));
        assert_eq!(s.restore(name), Err(Error::NameInvalid { at }));
        assert_eq!(s.forget(name), Err(Error::NameInvalid { at }));
    }
    for name in ["a", "b2", "c_"] {
        s.snapshot(name).unwrap();
    }
    assert_eq!(s.snapshot("d"), Err(Error::SnapshotLimit));
    assert_eq!(s.snapshot("a-"), Err(Error::NameInvalid { at: 1 }));
    s.snapshot("b2").unwrap();
    s.begin().unwrap();
    s.forget("a").unwrap();
    s.rollback().unwrap();
    s.snapshot("d").unwrap();
    assert_eq!(s.restore("a"), Err(Error::SnapshotMissing("a".into())));
    s.set("quota", "replacement").unwrap();
    s.snapshot("b2").unwrap();
    s.delete("quota").unwrap();
    s.restore("b2").unwrap();
    assert_eq!(s.get("quota"), Ok(Some("replacement".into())));
}

// R6, R14
#[test]
fn h_depth_limit() {
    let mut s = Store::new();
    for _ in 0..4 {
        s.begin().unwrap();
    }
    assert_eq!(s.begin(), Err(Error::DepthLimit));
    assert_eq!(s.depth(), Ok(4));
    for _ in 0..4 {
        s.commit().unwrap();
    }
    assert_eq!(s.depth(), Ok(0));
}

// R7–R8, R14
#[test]
fn h_no_scopes() {
    let mut s = Store::new();
    s.set("x", "v").unwrap();
    assert_eq!(s.rollback(), Err(Error::NoTransaction));
    assert_eq!(s.commit(), Err(Error::NoTransaction));
    assert_eq!(s.get("x"), Ok(Some("v".into())));
}

// R3, R5
#[test]
fn h_working_count() {
    let mut s = Store::new();
    s.set("x", "yes").unwrap();
    s.begin().unwrap();
    s.set("x", "no").unwrap();
    s.set("y", "no").unwrap();
    assert_eq!(s.count("yes"), Ok(0));
    assert_eq!(s.count("no"), Ok(2));
}

// R3–R4, R8
#[test]
fn h_delete_isolation() {
    let mut s = Store::new();
    s.set("x", "kept").unwrap();
    s.begin().unwrap();
    assert_eq!(s.delete("x"), Ok(true));
    assert_eq!(s.delete("x"), Ok(false));
    s.rollback().unwrap();
    assert_eq!(s.get("x"), Ok(Some("kept".into())));
}

// R6–R8
#[test]
fn h_one_rollback() {
    let mut s = Store::new();
    s.begin().unwrap();
    s.set("x", "outer").unwrap();
    s.begin().unwrap();
    s.set("x", "inner").unwrap();
    s.rollback().unwrap();
    assert_eq!(s.depth(), Ok(1));
    assert_eq!(s.get("x"), Ok(Some("outer".into())));
}

// R7–R8
#[test]
fn h_nested_commit_undo() {
    let mut s = Store::new();
    s.set("x", "base").unwrap();
    s.begin().unwrap();
    s.begin().unwrap();
    s.delete("x").unwrap();
    s.commit().unwrap();
    assert_eq!(s.get("x"), Ok(None));
    s.rollback().unwrap();
    assert_eq!(s.get("x"), Ok(Some("base".into())));
}

// R4, R7
#[test]
fn h_commit_replaces() {
    let mut s = Store::new();
    s.set("x", "base").unwrap();
    s.begin().unwrap();
    s.delete("x").unwrap();
    s.set("y", "new").unwrap();
    s.commit().unwrap();
    assert_eq!(s.get("x"), Ok(None));
    assert_eq!(s.get("y"), Ok(Some("new".into())));
}

// R9–R10
#[test]
fn h_restore_depth() {
    let mut s = Store::new();
    s.set("x", "saved").unwrap();
    s.snapshot("s").unwrap();
    s.begin().unwrap();
    s.begin().unwrap();
    s.restore("s").unwrap();
    assert_eq!(s.depth(), Ok(2));
    s.rollback().unwrap();
    assert_eq!(s.depth(), Ok(1));
}

// R8–R10
#[test]
fn h_restore_parent() {
    let mut s = Store::new();
    s.set("x", "snapshot").unwrap();
    s.snapshot("s").unwrap();
    s.set("x", "parent").unwrap();
    s.begin().unwrap();
    s.restore("s").unwrap();
    assert_eq!(s.get("x"), Ok(Some("snapshot".into())));
    s.rollback().unwrap();
    assert_eq!(s.get("x"), Ok(Some("parent".into())));
}

// R7, R10
#[test]
fn h_restore_commit() {
    let mut s = Store::new();
    s.set("x", "saved").unwrap();
    s.snapshot("s").unwrap();
    s.set("x", "old").unwrap();
    s.begin().unwrap();
    s.restore("s").unwrap();
    s.commit().unwrap();
    assert_eq!(s.get("x"), Ok(Some("saved".into())));
}

// R8–R9, R15
#[test]
fn h_snapshot_survives() {
    let mut s = Store::new();
    s.begin().unwrap();
    s.set("x", "ghost").unwrap();
    s.snapshot("s").unwrap();
    s.rollback().unwrap();
    assert_eq!(s.get("x"), Ok(None));
    s.restore("s").unwrap();
    assert_eq!(s.get("x"), Ok(Some("ghost".into())));
}

// R8–R9, R15
#[test]
fn h_snapshot_replace_survives() {
    let mut s = Store::new();
    s.snapshot("s").unwrap();
    s.begin().unwrap();
    s.set("x", "inside").unwrap();
    s.snapshot("s").unwrap();
    s.rollback().unwrap();
    s.restore("s").unwrap();
    assert_eq!(s.get("x"), Ok(Some("inside".into())));
}

// R8, R11
#[test]
fn h_forget_survives() {
    let mut s = Store::new();
    s.snapshot("s").unwrap();
    s.begin().unwrap();
    s.forget("s").unwrap();
    s.rollback().unwrap();
    assert_eq!(s.restore("s"), Err(Error::SnapshotMissing("s".into())));
}

// R9–R10, R15
#[test]
fn h_saved_immutable() {
    let mut s = Store::new();
    s.set("x", "original").unwrap();
    s.snapshot("s").unwrap();
    s.restore("s").unwrap();
    s.set("x", "edited").unwrap();
    s.restore("s").unwrap();
    assert_eq!(s.get("x"), Ok(Some("original".into())));
}

// R9–R10
#[test]
fn h_restore_empty() {
    let mut s = Store::new();
    s.snapshot("blank").unwrap();
    s.set("x", "gone").unwrap();
    s.restore("blank").unwrap();
    assert_eq!(s.count("gone"), Ok(0));
}

// R11, R14
#[test]
fn h_missing_restore_atomic() {
    let mut s = Store::new();
    s.begin().unwrap();
    s.set("x", "present").unwrap();
    assert_eq!(
        s.restore("missing"),
        Err(Error::SnapshotMissing("missing".into()))
    );
    assert_eq!(s.depth(), Ok(1));
    assert_eq!(s.get("x"), Ok(Some("present".into())));
}

// R14
#[test]
fn h_independent() {
    let mut a = Store::new();
    let mut b = Store::new();
    a.set("x", "only-a").unwrap();
    a.snapshot("s").unwrap();
    assert_eq!(b.get("x"), Ok(None));
    assert_eq!(b.restore("s"), Err(Error::SnapshotMissing("s".into())));
}

// R2, R12, R16
#[test]
fn h_exact_strings() {
    let mut s = Store::new();
    for k in ["é", "e\u{301}", " "] {
        s.set(k, k).unwrap();
        s.snapshot("saved").unwrap();
    }
    s.set("\0", "nul").unwrap();
    assert_eq!(s.count("é"), Ok(1));
    assert_eq!(s.count("e\u{301}"), Ok(1));
    s.restore("saved").unwrap();
    assert_eq!(s.get("\0"), Ok(None));
}

// R3–R8
#[test]
fn h_many_keys() {
    let mut s = Store::new();
    for i in 0..300 {
        s.set(&format!("k{i}"), "v").unwrap();
    }
    s.begin().unwrap();
    for i in 0..100 {
        s.delete(&format!("k{i}")).unwrap();
    }
    assert_eq!(s.count("v"), Ok(200));
    s.rollback().unwrap();
    assert_eq!(s.count("v"), Ok(300));
}

// R6–R8
#[test]
fn h_four_layers() {
    let mut s = Store::new();
    for i in 0..4 {
        s.begin().unwrap();
        s.set("x", &i.to_string()).unwrap();
    }
    s.rollback().unwrap();
    assert_eq!(s.get("x"), Ok(Some("2".into())));
    s.commit().unwrap();
    s.rollback().unwrap();
    assert_eq!(s.get("x"), Ok(Some("0".into())));
    s.commit().unwrap();
    assert_eq!(s.depth(), Ok(0));
}
