#[test]
fn hidden_absent_delete() {
    let mut s = Store::new();
    assert!(!s.delete("ghost"));
    s.set("live", "v");
    assert!(!s.delete("ghost"));
    assert_eq!(s.get("live"), Some("v".into()));
}

#[test]
fn hidden_empty_key() {
    let mut s = Store::new();
    s.set("", "zero");
    assert_eq!(s.get(""), Some("zero".into()));
    assert_eq!(s.count("zero"), 1);
    assert!(s.delete(""));
    assert_eq!(s.get(""), None);
}

#[test]
fn hidden_empty_value() {
    let mut s = Store::new();
    s.set("void-a", "");
    s.set("void-b", "");
    assert_eq!(s.get("void-a"), Some(String::new()));
    assert_eq!(s.count(""), 2);
}

#[test]
fn hidden_replacement_moves_counts() {
    let mut s = Store::new();
    s.set("switch", "old");
    s.set("switch", "new");
    assert_eq!(s.count("old"), 0);
    assert_eq!(s.count("new"), 1);
    assert_eq!(s.get("switch"), Some("new".into()));
}

#[test]
fn hidden_idempotent_set() {
    let mut s = Store::new();
    for _ in 0..50 {
        s.set("same", "same-value");
    }
    assert_eq!(s.count("same-value"), 1);
}

#[test]
fn hidden_repeated_delete() {
    let mut s = Store::new();
    s.set("twice", "erase");
    assert!(s.delete("twice"));
    assert!(!s.delete("twice"));
    assert_eq!(s.count("erase"), 0);
}

#[test]
fn hidden_count_many_keys() {
    let mut s = Store::new();
    for i in 0..128 {
        s.set(&format!("k{i}"), if i % 3 == 0 { "group" } else { "other" });
    }
    assert_eq!(s.count("group"), 43);
    assert_eq!(s.count("other"), 85);
}

#[test]
fn hidden_unicode_exact() {
    let mut s = Store::new();
    s.set("é", "雪");
    s.set("e\u{301}", "雪");
    assert_eq!(s.count("雪"), 2);
    assert_eq!(s.get("é"), Some("雪".into()));
    assert!(s.delete("e\u{301}"));
    assert_eq!(s.get("é"), Some("雪".into()));
}

#[test]
fn hidden_case_sensitive() {
    let mut s = Store::new();
    s.set("KEY", "Value");
    s.set("key", "value");
    assert_eq!(s.count("Value"), 1);
    assert_eq!(s.count("VALUE"), 0);
    assert_eq!(s.get("Key"), None);
}

#[test]
fn hidden_owned_inputs() {
    let mut s = Store::new();
    let mut k = String::from("owned-key");
    let mut v = String::from("owned-value");
    s.set(&k, &v);
    k.clear();
    v.clear();
    assert_eq!(s.get("owned-key"), Some("owned-value".into()));
}

#[test]
fn hidden_owned_output() {
    let mut s = Store::new();
    s.set("clone", "stable");
    let mut v = s.get("clone").unwrap();
    v.push_str("-changed");
    assert_eq!(s.get("clone"), Some("stable".into()));
}

#[test]
fn hidden_independent_stores() {
    let mut a = Store::new();
    let mut b = Store::new();
    a.set("private", "A");
    b.set("private", "B");
    assert_eq!(a.get("private"), Some("A".into()));
    assert_eq!(b.get("private"), Some("B".into()));
}

#[test]
fn hidden_delete_then_reinsert() {
    let mut s = Store::new();
    s.set("return", "before");
    assert!(s.delete("return"));
    s.set("return", "after");
    assert_eq!(s.count("before"), 0);
    assert_eq!(s.get("return"), Some("after".into()));
}

#[test]
fn hidden_whitespace_nul() {
    let mut s = Store::new();
    s.set(" x ", "a\0b");
    s.set("x", "a b");
    assert_eq!(s.get(" x "), Some("a\0b".into()));
    assert_eq!(s.count("a\0b"), 1);
}

#[test]
fn hidden_remove_one_of_duplicates() {
    let mut s = Store::new();
    for k in ["u", "v", "w", "z"] {
        s.set(k, "shared");
    }
    assert!(s.delete("v"));
    assert_eq!(s.count("shared"), 3);
    assert_eq!(s.get("w"), Some("shared".into()));
}

#[test]
fn hidden_large_value() {
    let mut s = Store::new();
    let v = "λ".repeat(4096);
    s.set("long", &v);
    assert_eq!(s.get("long"), Some(v.clone()));
    assert_eq!(s.count(&v), 1);
}
