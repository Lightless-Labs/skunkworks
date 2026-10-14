#[test]
fn hidden_commit_without_begin() {
    let mut s = Store::new();
    s.set("keep", "intact");
    assert_eq!(s.commit(), Err(Error::NoTransaction));
    assert_eq!(s.get("keep"), Some("intact".into()));
}

#[test]
fn hidden_rollback_without_begin() {
    let mut s = Store::new();
    assert_eq!(s.rollback(), Err(Error::NoTransaction));
    s.set("keep-r", "intact-r");
    assert_eq!(s.rollback(), Err(Error::NoTransaction));
    assert_eq!(s.count("intact-r"), 1);
}

#[test]
fn hidden_nested_innermost_only() {
    let mut s = Store::new();
    s.set("phase", "base");
    s.begin();
    s.set("phase", "outer-state");
    s.begin();
    s.set("phase", "inner-state");
    assert_eq!(s.rollback(), Ok(()));
    assert_eq!(s.get("phase"), Some("outer-state".into()));
    assert_eq!(s.rollback(), Ok(()));
    assert_eq!(s.get("phase"), Some("base".into()));
}

#[test]
fn hidden_rollback_restores_delete() {
    let mut s = Store::new();
    s.set("removed", "restored");
    s.begin();
    assert!(s.delete("removed"));
    assert_eq!(s.count("restored"), 0);
    s.rollback().unwrap();
    assert_eq!(s.get("removed"), Some("restored".into()));
}

#[test]
fn hidden_rollback_removes_insert() {
    let mut s = Store::new();
    s.begin();
    s.set("transient", "temp");
    s.rollback().unwrap();
    assert_eq!(s.get("transient"), None);
    assert_eq!(s.count("temp"), 0);
}

#[test]
fn hidden_commit_keeps_deletes() {
    let mut s = Store::new();
    s.set("gone", "gone-value");
    s.begin();
    s.delete("gone");
    s.begin();
    s.set("new-key", "new-value");
    s.commit().unwrap();
    assert_eq!(s.get("gone"), None);
    assert_eq!(s.get("new-key"), Some("new-value".into()));
    assert_eq!(s.commit(), Err(Error::NoTransaction));
}

#[test]
fn hidden_count_overwrite_and_delete() {
    let mut s = Store::new();
    s.set("a", "match");
    s.set("b", "match");
    s.begin();
    s.set("a", "different");
    s.delete("b");
    assert_eq!(s.count("match"), 0);
    assert_eq!(s.count("different"), 1);
    s.rollback().unwrap();
    assert_eq!(s.count("match"), 2);
}

#[test]
fn hidden_deep_nesting() {
    let mut s = Store::new();
    s.set("depth", "start");
    for i in 0..80 {
        s.begin();
        s.set("depth", &i.to_string());
    }
    for i in (0..80).rev() {
        assert_eq!(s.get("depth"), Some(i.to_string()));
        s.rollback().unwrap();
    }
    assert_eq!(s.get("depth"), Some("start".into()));
    assert_eq!(s.rollback(), Err(Error::NoTransaction));
}

#[test]
fn hidden_empty_scopes() {
    let mut s = Store::new();
    for _ in 0..5 {
        s.begin();
    }
    for _ in 0..5 {
        assert_eq!(s.rollback(), Ok(()));
    }
    assert_eq!(s.rollback(), Err(Error::NoTransaction));
}

#[test]
fn hidden_empty_strings_in_scopes() {
    let mut s = Store::new();
    s.set("", "");
    s.begin();
    s.set("", "filled");
    s.begin();
    s.delete("");
    s.rollback().unwrap();
    assert_eq!(s.get(""), Some("filled".into()));
    s.rollback().unwrap();
    assert_eq!(s.count(""), 1);
}

#[test]
fn hidden_begin_after_commit() {
    let mut s = Store::new();
    s.begin();
    s.set("epoch", "first-epoch");
    s.commit().unwrap();
    s.begin();
    s.set("epoch", "second-epoch");
    s.rollback().unwrap();
    assert_eq!(s.get("epoch"), Some("first-epoch".into()));
}

#[test]
fn hidden_begin_after_rollback() {
    let mut s = Store::new();
    s.set("cycle", "ground");
    s.begin();
    s.set("cycle", "discarded");
    s.rollback().unwrap();
    s.begin();
    s.set("cycle", "kept");
    s.commit().unwrap();
    assert_eq!(s.get("cycle"), Some("kept".into()));
}

#[test]
fn hidden_child_rollback_parent_commit() {
    let mut s = Store::new();
    s.begin();
    s.set("ancestor", "yes-parent");
    s.begin();
    s.set("descendant", "no-child");
    s.rollback().unwrap();
    s.commit().unwrap();
    assert_eq!(s.get("ancestor"), Some("yes-parent".into()));
    assert_eq!(s.get("descendant"), None);
}

#[test]
fn hidden_delete_missing_in_transaction() {
    let mut s = Store::new();
    s.set("survivor", "safe");
    s.begin();
    assert!(!s.delete("never"));
    s.rollback().unwrap();
    assert_eq!(s.get("survivor"), Some("safe".into()));
}

#[test]
fn hidden_snapshot_at_begin_not_alias() {
    let mut s = Store::new();
    for i in 0..64 {
        s.set(&format!("seed-{i}"), "seeded");
    }
    s.begin();
    for i in 0..64 {
        s.delete(&format!("seed-{i}"));
    }
    s.rollback().unwrap();
    assert_eq!(s.count("seeded"), 64);
}

#[test]
fn hidden_many_scopes_commit() {
    let mut s = Store::new();
    for i in 0..40 {
        s.begin();
        s.set(&format!("scope-{i}"), "committed");
    }
    s.commit().unwrap();
    assert_eq!(s.count("committed"), 40);
    assert_eq!(s.rollback(), Err(Error::NoTransaction));
}
