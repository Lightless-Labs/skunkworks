#[test]
fn public_write_read_replace() {
    let mut s = Store::new();
    s.set("name", "Ada");
    assert_eq!(s.get("name"), Some("Ada".into()));
    s.set("name", "Grace");
    assert_eq!(s.get("name"), Some("Grace".into()));
}

#[test]
fn public_delete_existing() {
    let mut s = Store::new();
    s.set("fruit", "pear");
    assert!(s.delete("fruit"));
    assert_eq!(s.get("fruit"), None);
}

#[test]
fn public_count_shared_values() {
    let mut s = Store::new();
    s.set("first", "blue");
    s.set("second", "blue");
    s.set("third", "red");
    assert_eq!(s.count("blue"), 2);
    assert_eq!(s.count("red"), 1);
}

#[test]
fn public_empty_store() {
    let s = Store::new();
    assert_eq!(s.get("absent"), None);
    assert_eq!(s.count("unseen"), 0);
}
