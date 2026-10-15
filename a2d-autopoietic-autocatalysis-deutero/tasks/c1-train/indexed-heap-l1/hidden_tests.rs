// R1-R2, R8, R11-R13, R19-R20
#[test]
fn hidden_empty_contract() {
    let mut q = IndexedHeap::new();
    for _ in 0..4 { assert_eq!(q.pop(), None); assert_eq!(q.peek(), None); }
    assert_eq!(q.get("  NOWHERE-9  "), Ok(None));
    assert_eq!(q.len(), 0); assert!(q.is_empty());
    let item = Item { key: "x".into(), priority: 3, value: "\0".into() };
    assert_eq!(item.clone(), item); assert!(!format!("{item:?}").is_empty());
    assert_eq!(Error::EmptyKey.clone(), Error::EmptyKey);
}

// R3-R5, R9, R13, R19
#[test]
fn hidden_original_byte_offsets() {
    let mut q = IndexedHeap::new();
    for (s, at) in [("   a/b  ", 4), ("  zé ", 3), ("  2abc ", 2), ("a b", 1), ("a\0b", 1), ("  \tname ", 2), ("\na", 0), ("  aé/", 3)] {
        assert_eq!(q.push(s, 0, ""), Err(Error::InvalidKey { at }), "{s:?}");
        assert_eq!(q.get(s), Err(Error::InvalidKey { at }), "{s:?}");
    }
    assert!(q.is_empty());
}

// R3-R5, R13, R19
#[test]
fn hidden_empty_and_unicode_whitespace() {
    let mut q = IndexedHeap::new();
    for s in ["", " ", "       "] { assert_eq!(q.push(s, 1, ""), Err(Error::EmptyKey)); assert_eq!(q.get(s), Err(Error::EmptyKey)); }
    for (s, at) in [("\u{a0}a", 0), ("  \u{2003}a", 2), ("a\t ", 1), ("a\n", 1)] {
        assert_eq!(q.push(s, 1, ""), Err(Error::InvalidKey { at }));
    }
}

// R4-R5, R9-R10, R13
#[test]
fn hidden_canonical_alias_and_long_key() {
    let mut q = IndexedHeap::new();
    q.push("  Z9--Tail-  ", 13, "v").unwrap();
    assert_eq!(q.get("z9--TAIL-").unwrap().unwrap().key, "z9--tail-");
    assert_eq!(q.push(" Z9--TAIL-", -9, ""), Err(Error::Duplicate { key: "z9--tail-".into() }));
    let long = format!("Q{}", "7-".repeat(2048));
    q.push(&long, 0, "long").unwrap();
    assert_eq!(q.get(&long.to_lowercase()).unwrap().unwrap().value, "long");
    assert_eq!(q.len(), 2);
}

// R6, R9, R12, R16
#[test]
fn hidden_signed_extremes() {
    let mut q = IndexedHeap::new();
    for (key, p) in [("max", i64::MAX), ("zero", 0), ("min", i64::MIN), ("neg", -1)] { q.push(key, p, "").unwrap(); }
    let out: Vec<_> = std::iter::from_fn(|| q.pop()).map(|i| i.priority).collect();
    assert_eq!(out, vec![i64::MIN, -1, 0, i64::MAX]);
}

// R6, R12, R15
#[test]
fn hidden_utf8_byte_length_rank() {
    let mut q = IndexedHeap::new();
    for (key, value) in [("u", "🦀"), ("v", "aaa"), ("w", "é"), ("x", "")] { q.push(key, -8, value).unwrap(); }
    let out: Vec<_> = std::iter::from_fn(|| q.pop()).map(|i| i.key).collect();
    assert_eq!(out, vec!["x", "w", "v", "u"]);
}

// R6-R7, R12
#[test]
fn hidden_fifo_beats_key_order() {
    let mut q = IndexedHeap::new();
    for key in ["z", "y", "a", "m"] { q.push(key, 17, "x").unwrap(); }
    for key in ["z", "y", "a", "m"] { assert_eq!(q.pop().unwrap().key, key); }
}

// R7, R12-R13, R17
#[test]
fn hidden_reinsert_is_youngest() {
    let mut q = IndexedHeap::new();
    q.push("older", 1, "").unwrap(); q.push("younger", 1, "").unwrap();
    assert_eq!(q.pop().unwrap().key, "older");
    assert_eq!(q.get("OLDER"), Ok(None));
    q.push("OLDER", 1, "").unwrap();
    assert_eq!(q.pop().unwrap().key, "younger");
    assert_eq!(q.pop().unwrap().key, "older");
}

// R11, R13-R15
#[test]
fn hidden_owned_reads_and_arbitrary_values() {
    let mut q = IndexedHeap::new();
    let value = " \0é\nZ\t ";
    q.push("snap", 4, value).unwrap();
    let mut a = q.peek().unwrap(); a.key.clear(); a.value.clear(); a.priority = -999;
    let mut b = q.get("SNAP").unwrap().unwrap(); b.value.push_str("bad");
    for _ in 0..5 { assert_eq!(q.get("snap").unwrap().unwrap().value, value); }
    assert_eq!(q.pop().unwrap(), Item { key: "snap".into(), priority: 4, value: value.into() });
}

// R5, R7, R10, R19
#[test]
fn hidden_duplicate_failure_keeps_age() {
    let mut q = IndexedHeap::new();
    q.push("zz", 2, "q").unwrap(); q.push("aa", 2, "w").unwrap();
    assert_eq!(q.push(" ZZ ", i64::MIN, ""), Err(Error::Duplicate { key: "zz".into() }));
    assert_eq!(q.push("zz/", 0, ""), Err(Error::InvalidKey { at: 2 }));
    assert_eq!(q.len(), 2);
    assert_eq!(q.pop().unwrap().key, "zz");
    assert_eq!(q.pop().unwrap().key, "aa");
}

// R8, R18-R20
#[test]
fn hidden_independent_queues() {
    let mut a = IndexedHeap::new(); let mut b = IndexedHeap::new();
    a.push("same", 31, "a").unwrap(); b.push("same", -31, "b").unwrap();
    assert_eq!(a.pop().unwrap().value, "a");
    assert_eq!(b.get("same").unwrap().unwrap().priority, -31);
    a.push("same", 0, "c").unwrap();
    assert_eq!(b.pop().unwrap().value, "b");
    assert_eq!(a.len(), 1);
}

// R6-R9, R12-R13, R17, R20
#[test]
fn hidden_many_items_index_integrity() {
    let mut q = IndexedHeap::new();
    let mut expected = Vec::new();
    for n in 0..240usize {
        let key = format!("k{n}"); let priority = ((n * 71) % 59) as i64 - 29;
        let value = "x".repeat((n * 13) % 9);
        q.push(&key, priority, &value).unwrap();
        expected.push((priority, value.len(), n, key, value));
    }
    expected.sort_by_key(|e| (e.0, e.1, e.2));
    for (left, (p, _, _, key, value)) in expected.into_iter().enumerate() {
        assert_eq!(q.get(&key).unwrap().unwrap().priority, p);
        assert_eq!(q.pop().unwrap(), Item { key: key.clone(), priority: p, value });
        assert_eq!(q.get(&key), Ok(None)); assert_eq!(q.len(), 239 - left);
    }
}

// R3-R5, R9, R19
#[test]
fn hidden_digit_cannot_start_key() {
    let mut q = IndexedHeap::new();
    for (s, at) in [("1", 0), (" 7good ", 1), ("-okay", 0), ("_a", 0)] { assert_eq!(q.push(s, 2, "v"), Err(Error::InvalidKey { at })); }
    q.push("A1-", 2, "v").unwrap(); assert_eq!(q.pop().unwrap().key, "a1-");
}

// R6-R7, R15
#[test]
fn hidden_tie_lengths_equal_bytes() {
    let mut q = IndexedHeap::new();
    q.push("zebra", 6, "é").unwrap(); q.push("ant", 6, "ab").unwrap();
    assert_eq!(q.pop().unwrap().key, "zebra"); assert_eq!(q.pop().unwrap().key, "ant");
}

// R6, R12, R15-R16
#[test]
fn hidden_priority_precedes_payload() {
    let mut q = IndexedHeap::new();
    q.push("small", 2, "").unwrap(); q.push("large", 1, &"x".repeat(300)).unwrap();
    assert_eq!(q.pop().unwrap().key, "large");
}

// R3-R5, R7-R10, R19
#[test]
fn hidden_fail_then_recover() {
    let mut q = IndexedHeap::new();
    assert_eq!(q.push("  broken!", -8, ""), Err(Error::InvalidKey { at: 8 }));
    q.push("recovered", 3, "ok").unwrap(); assert_eq!(q.len(), 1);
    assert_eq!(q.pop().unwrap().key, "recovered"); assert!(q.is_empty());
}

// R6-R7, R11, R13-R14
#[test]
fn hidden_reads_preserve_age() {
    let mut q = IndexedHeap::new();
    q.push("q", 7, "v").unwrap(); q.push("p", 7, "w").unwrap();
    for _ in 0..20 { assert_eq!(q.peek().unwrap().key, "q"); assert!(q.get("P").unwrap().is_some()); }
    assert_eq!(q.pop().unwrap().key, "q");
}

// R3-R5, R13, R19
#[test]
fn hidden_validation_on_nonempty() {
    let mut q = IndexedHeap::new(); q.push("valid", 3, "ok").unwrap();
    assert_eq!(q.get(" valid! "), Err(Error::InvalidKey { at: 6 }));
    assert_eq!(q.get("  "), Err(Error::EmptyKey));
    assert_eq!(q.peek().unwrap().key, "valid"); assert_eq!(q.len(), 1);
}

// R5, R9, R13, R15
#[test]
fn hidden_spelling_does_not_change_value() {
    let mut q = IndexedHeap::new();
    q.push(" MiXeD-8 ", -12, " MiXeD-8 ").unwrap();
    assert_eq!(q.get(" MIXED-8 ").unwrap().unwrap(), Item { key: "mixed-8".into(), priority: -12, value: " MiXeD-8 ".into() });
}

// R7-R8, R12, R17-R20
#[test]
fn hidden_repeated_heap_reuse() {
    let mut q = IndexedHeap::new();
    for round in 0..30i64 {
        q.push("r", round + 1, "xx").unwrap(); q.push("s", round, "z").unwrap();
        assert_eq!(q.pop().unwrap().key, "s"); assert_eq!(q.pop().unwrap().key, "r");
        assert_eq!(q.len(), 0); assert_eq!(q.peek(), None);
    }
}
