#[test]
fn public_core_operations() {
    let mut s = Store::new();
    s.set("item", "green");
    assert_eq!(s.get("item"), Some("green".into()));
    assert_eq!(s.count("green"), 1);
    assert!(s.delete("item"));
}

#[test]
fn public_single_rollback() {
    let mut s = Store::new();
    s.set("root", "original");
    s.begin();
    s.set("root", "edited");
    assert_eq!(s.get("root"), Some("edited".into()));
    assert_eq!(s.rollback(), Ok(()));
    assert_eq!(s.get("root"), Some("original".into()));
}

#[test]
fn public_single_commit() {
    let mut s = Store::new();
    s.begin();
    s.set("saved", "yes");
    assert_eq!(s.commit(), Ok(()));
    assert_eq!(s.get("saved"), Some("yes".into()));
}

#[test]
fn public_nested_commit_closes_all() {
    let mut s = Store::new();
    s.begin();
    s.set("outer", "one");
    s.begin();
    s.set("inner", "two");
    assert_eq!(s.commit(), Ok(()));
    assert_eq!(s.get("outer"), Some("one".into()));
    assert_eq!(s.get("inner"), Some("two".into()));
    assert_eq!(s.rollback(), Err(Error::NoTransaction));
}
