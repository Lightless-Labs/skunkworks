#[test]
fn spec_r1_single_file_rust_api() {
    let q = IndexedHeap::new();
    assert!(q.is_empty());
}

#[test]
fn spec_r2_public_types_and_exact_error_variants() {
    let item = Item {
        key: "a".to_owned(),
        priority: 0,
        value: String::new(),
    };
    let cloned = item.clone();
    assert_eq!(item, cloned);
    assert_eq!(format!("{item:?}"), "Item { key: \"a\", priority: 0, value: \"\" }");

    let empty = Error::EmptyKey;
    let invalid = Error::InvalidKey { at: 0 };
    let duplicate = Error::Duplicate { key: "a".to_owned() };
    assert_eq!(empty, empty.clone());
    assert_eq!(invalid, Error::InvalidKey { at: 0 });
    assert_eq!(duplicate, Error::Duplicate { key: "a".to_owned() });
    assert_eq!(format!("{empty:?}"), "EmptyKey");
}

#[test]
fn spec_r3_trims_only_ascii_spaces() {
    let mut q = IndexedHeap::new();
    q.push("  Maple  ", 0, "").unwrap();
    assert_eq!(q.get("Maple").unwrap().unwrap().key, "maple");
    assert_eq!(q.get("\tMaple").unwrap_err(), Error::InvalidKey { at: 0 });
    assert_eq!(q.get("Maple\n").unwrap_err(), Error::InvalidKey { at: 5 });
    assert_eq!(q.get("\u{00a0}Maple").unwrap_err(), Error::InvalidKey { at: 0 });
}

#[test]
fn spec_r4_key_validation_reports_original_byte_offsets() {
    let mut q = IndexedHeap::new();
    assert_eq!(q.push("1abc", 0, ""), Err(Error::InvalidKey { at: 0 }));
    assert_eq!(q.push("A_b", 0, ""), Err(Error::InvalidKey { at: 1 }));
    assert_eq!(q.push("Ab!", 0, ""), Err(Error::InvalidKey { at: 2 }));
    assert_eq!(q.push(" Aé", 0, ""), Err(Error::InvalidKey { at: 2 }));
    assert_eq!(q.push("A-", 0, ""), Ok(()));
    assert_eq!(q.push("A--", 0, ""), Ok(()));
}

#[test]
fn spec_r5_canonical_keys_and_error_keys() {
    let mut q = IndexedHeap::new();
    q.push("  aBc-9  ", 2, "v").unwrap();
    assert_eq!(q.peek().unwrap().key, "abc-9");
    assert_eq!(
        q.push("ABC-9", 2, "v"),
        Err(Error::Duplicate {
            key: "abc-9".to_owned()
        })
    );
    assert_eq!(q.get(" AbC-9 ").unwrap().unwrap().key, "abc-9");
    assert_eq!(q.get(" ").unwrap_err(), Error::EmptyKey);
}

#[test]
fn spec_r6_full_ordering_priority_value_length_and_age() {
    let mut q = IndexedHeap::new();
    q.push("Alpha", 0, "zz").unwrap();
    q.push("Bravo", 0, "a").unwrap();
    q.push("Charlie", 0, "b").unwrap();
    q.push("Delta", -1, "long").unwrap();
    assert_eq!(q.pop().unwrap().key, "delta");
    assert_eq!(q.pop().unwrap().key, "bravo");
    assert_eq!(q.pop().unwrap().key, "charlie");
    assert_eq!(q.pop().unwrap().key, "alpha");
}

#[test]
fn spec_r7_removal_and_reinsertion_refreshes_age() {
    let mut q = IndexedHeap::new();
    q.push("Alpha", 1, "").unwrap();
    q.push("Bravo", 1, "").unwrap();
    assert_eq!(q.pop().unwrap().key, "alpha");
    q.push("ALPHA", 1, "").unwrap();
    assert_eq!(q.pop().unwrap().key, "bravo");
    assert_eq!(q.pop().unwrap().key, "alpha");
}

#[test]
fn spec_r8_empty_length_and_independent_new_queues() {
    let mut a = IndexedHeap::new();
    let b = IndexedHeap::new();
    assert_eq!(a.len(), 0);
    assert_eq!(a.is_empty(), a.len() == 0);
    a.push("A", 0, "").unwrap();
    assert_eq!(a.len(), 1);
    assert!(!a.is_empty());
    assert_eq!(b.len(), 0);
    assert!(b.is_empty());
}

#[test]
fn spec_r9_push_retains_item_and_accepts_valid_key() {
    let mut q = IndexedHeap::new();
    assert_eq!(q.push("Zebra-2", -4, "payload"), Ok(()));
    assert_eq!(
        q.get("zebra-2").unwrap(),
        Some(Item {
            key: "zebra-2".to_owned(),
            priority: -4,
            value: "payload".to_owned()
        })
    );
}

#[test]
fn spec_r10_duplicate_push_does_not_replace_item() {
    let mut q = IndexedHeap::new();
    q.push("Oak", 5, "original").unwrap();
    assert_eq!(
        q.push(" OAK ", -9, "replacement"),
        Err(Error::Duplicate {
            key: "oak".to_owned()
        })
    );
    assert_eq!(
        q.get("oak").unwrap(),
        Some(Item {
            key: "oak".to_owned(),
            priority: 5,
            value: "original".to_owned()
        })
    );
}

#[test]
fn spec_r11_peek_returns_best_owned_item_without_removing_it() {
    let mut q = IndexedHeap::new();
    q.push("Later", 4, "x").unwrap();
    q.push("Sooner", 2, "y").unwrap();
    let mut item = q.peek().unwrap();
    item.key.clear();
    item.value.push('!');
    assert_eq!(q.len(), 2);
    assert_eq!(q.peek().unwrap().key, "sooner");
    assert_eq!(q.peek().unwrap().value, "y");
}

#[test]
fn spec_r12_pop_drains_in_best_to_worst_order() {
    let mut q = IndexedHeap::new();
    q.push("A", 2, "x").unwrap();
    q.push("B", -1, "x").unwrap();
    q.push("C", 2, "").unwrap();
    assert_eq!(q.pop().unwrap().key, "b");
    assert_eq!(q.pop().unwrap().key, "c");
    assert_eq!(q.pop().unwrap().key, "a");
    assert_eq!(q.pop(), None);
}

#[test]
fn spec_r13_get_validates_even_when_absent_or_empty() {
    let q = IndexedHeap::new();
    assert_eq!(q.get("Missing"), Ok(None));
    assert_eq!(q.get(""), Err(Error::EmptyKey));
    assert_eq!(q.get("9missing"), Err(Error::InvalidKey { at: 0 }));
}

#[test]
fn spec_r14_reads_and_mutated_snapshots_do_not_change_queue() {
    let mut q = IndexedHeap::new();
    q.push("First", 0, "one").unwrap();
    q.push("Second", 0, "two").unwrap();
    for _ in 0..3 {
        let _ = q.peek();
        let _ = q.get("SECOND");
    }
    let mut snapshot = q.get("first").unwrap().unwrap();
    snapshot.priority = i64::MIN;
    snapshot.key = "changed".to_owned();
    snapshot.value.clear();
    assert_eq!(q.pop().unwrap().key, "first");
    assert_eq!(q.pop().unwrap().key, "second");
}

#[test]
fn spec_r15_values_are_preserved_byte_for_byte() {
    let mut q = IndexedHeap::new();
    let value = " \tÉ\u{0000}ß\n ";
    q.push("Value", 0, value).unwrap();
    assert_eq!(q.get("value").unwrap().unwrap().value.as_bytes(), value.as_bytes());
    q.push("Empty", 1, "").unwrap();
    assert_eq!(q.get("empty").unwrap().unwrap().value, "");
}

#[test]
fn spec_r16_all_signed_i64_priorities_order_without_overflow() {
    let mut q = IndexedHeap::new();
    q.push("Maximum", i64::MAX, "").unwrap();
    q.push("Zero", 0, "").unwrap();
    q.push("Minimum", i64::MIN, "").unwrap();
    q.push("Negative", -1, "").unwrap();
    assert_eq!(q.pop().unwrap().priority, i64::MIN);
    assert_eq!(q.pop().unwrap().priority, -1);
    assert_eq!(q.pop().unwrap().priority, 0);
    assert_eq!(q.pop().unwrap().priority, i64::MAX);
}

#[test]
fn spec_r17_pop_removal_reuse_and_moved_indices() {
    let mut q = IndexedHeap::new();
    for (key, priority) in [("A", 4), ("B", 1), ("C", 3), ("D", 2)] {
        q.push(key, priority, key).unwrap();
    }
    assert_eq!(q.pop().unwrap().key, "b");
    assert_eq!(q.get("b"), Ok(None));
    assert_eq!(q.get("a").unwrap().unwrap().value, "A");
    assert_eq!(q.get("c").unwrap().unwrap().value, "C");
    assert_eq!(q.get("d").unwrap().unwrap().value, "D");
    q.push("B", 0, "reused").unwrap();
    assert_eq!(q.pop().unwrap().key, "b");
    assert_eq!(q.pop().unwrap().key, "d");
    assert_eq!(q.pop().unwrap().key, "c");
    assert_eq!(q.pop().unwrap().key, "a");
}

#[test]
fn spec_r18_queues_have_independent_key_spaces_and_histories() {
    let mut a = IndexedHeap::new();
    let mut b = IndexedHeap::new();
    a.push("Shared", 1, "a").unwrap();
    b.push("SHARED", 9, "b").unwrap();
    a.push("OnlyA", 0, "").unwrap();
    assert_eq!(b.len(), 1);
    assert_eq!(b.peek().unwrap().priority, 9);
    assert_eq!(a.pop().unwrap().key, "onlya");
    assert_eq!(b.pop().unwrap().value, "b");
    assert_eq!(a.pop().unwrap().value, "a");
}

#[test]
fn spec_r19_errors_and_empty_none_leave_receiver_unchanged() {
    let mut q = IndexedHeap::new();
    q.push("First", 1, "keep").unwrap();
    q.push("Second", 2, "keep2").unwrap();
    let before_len = q.len();
    let before_peek = q.peek();
    assert_eq!(q.push("1bad", -100, "x"), Err(Error::InvalidKey { at: 0 }));
    assert_eq!(
        q.push("FIRST", -100, "x"),
        Err(Error::Duplicate {
            key: "first".to_owned()
        })
    );
    assert_eq!(q.len(), before_len);
    assert_eq!(q.peek(), before_peek);

    let mut empty = IndexedHeap::new();
    assert_eq!(empty.pop(), None);
    assert_eq!(empty.peek(), None);
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());
}

#[test]
fn spec_r20_only_retained_items_contribute_to_length_and_order() {
    let mut q = IndexedHeap::new();
    q.push("A", 10, "").unwrap();
    q.push("B", -1, "").unwrap();
    assert_eq!(q.len(), 2);
    assert_eq!(q.pop().unwrap().key, "b");
    assert_eq!(q.len(), 1);
    assert_eq!(q.peek().unwrap().key, "a");
    assert_eq!(q.pop().unwrap().key, "a");
    assert_eq!(q.len(), 0);
    assert!(q.is_empty());
}
