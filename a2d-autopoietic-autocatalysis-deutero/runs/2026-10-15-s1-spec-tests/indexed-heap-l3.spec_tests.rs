#[test]
fn spec_r1_single_file_api_compiles() {
    let _ = IndexedHeap::new();
}

#[test]
fn spec_r2_public_types_and_exact_error_variants() {
    fn assert_traits<T: std::fmt::Debug + Clone + PartialEq + Eq>() {}
    assert_traits::<Item>();
    assert_traits::<Error>();
    assert_eq!(Error::EmptyKey, Error::EmptyKey);
    assert_eq!(Error::InvalidKey { at: 0 }, Error::InvalidKey { at: 0 });
    assert_eq!(
        Error::Duplicate { key: "x".into() },
        Error::Duplicate { key: "x".into() }
    );
    assert_eq!(
        Error::Missing { key: "x".into() },
        Error::Missing { key: "x".into() }
    );
    assert_eq!(
        Error::WrongDirection {
            current: i64::MIN,
            requested: i64::MAX
        },
        Error::WrongDirection {
            current: i64::MIN,
            requested: i64::MAX
        }
    );
    assert_eq!(
        Error::Collision { key: "x".into() },
        Error::Collision { key: "x".into() }
    );

    fn public_item_fields(item: &Item) -> (&str, i64, &str) {
        (&item.key, item.priority, &item.value)
    }
    let item = Item {
        key: "k".into(),
        priority: 0,
        value: "v".into(),
    };
    assert_eq!(public_item_fields(&item), ("k", 0, "v"));
}

#[test]
fn spec_r3_trim_only_ascii_spaces() {
    let mut q = IndexedHeap::new();
    q.push("  Alpha  ", 0, "").unwrap();
    assert_eq!(q.get(" Alpha "), Ok(Some(Item {
        key: "alpha".into(),
        priority: 0,
        value: "".into(),
    })));
    assert_eq!(q.get("\t"), Err(Error::InvalidKey { at: 0 }));
    assert_eq!(q.get("\nAlpha"), Err(Error::InvalidKey { at: 0 }));
    assert_eq!(q.get("Alpha "), Ok(Some(Item {
        key: "alpha".into(),
        priority: 0,
        value: "".into(),
    })));
}

#[test]
fn spec_r4_key_validation_offsets_and_allowed_hyphens() {
    let mut q = IndexedHeap::new();
    q.push("A--9-", 0, "").unwrap();
    assert_eq!(q.get("A--9-").unwrap().unwrap().key, "a--9-");
    assert_eq!(q.push("9abc", 0, ""), Err(Error::InvalidKey { at: 0 }));
    assert_eq!(q.push("a_b", 0, ""), Err(Error::InvalidKey { at: 1 }));
    assert_eq!(q.push(" Aé", 0, ""), Err(Error::InvalidKey { at: 2 }));
    assert_eq!(q.push(" Aé", 0, ""), Err(Error::InvalidKey { at: 2 }));
    assert_eq!(q.push("a".repeat(4096).as_str(), 0, ""), Ok(()));
}

#[test]
fn spec_r5_canonical_storage_and_error_keys() {
    let mut q = IndexedHeap::new();
    q.push("  MiXeD ", 1, "v").unwrap();
    assert_eq!(q.get("mixed").unwrap().unwrap().key, "mixed");
    assert_eq!(
        q.push(" MIXED ", 2, "other"),
        Err(Error::Duplicate { key: "mixed".into() })
    );
    assert_eq!(
        q.remove(" absent "),
        Err(Error::Missing { key: "absent".into() })
    );
}

#[test]
fn spec_r6_order_priority_value_length_then_age_without_key_tiebreak() {
    let mut q = IndexedHeap::new();
    q.push("z", 0, "bb").unwrap();
    q.push("a", 0, "aa").unwrap();
    q.push("long", 0, "z").unwrap();
    q.push("later", 0, "aa").unwrap();
    assert_eq!(q.pop().unwrap().key, "long");
    assert_eq!(q.pop().unwrap().key, "z");
    assert_eq!(q.pop().unwrap().key, "a");
    assert_eq!(q.pop().unwrap().key, "later");
}

#[test]
fn spec_r7_reinserted_key_gets_new_age_and_failures_do_not_refresh_age() {
    let mut q = IndexedHeap::new();
    q.push("older", 4, "x").unwrap();
    q.push("removed", 4, "x").unwrap();
    assert_eq!(q.remove("removed").unwrap().key, "removed");
    q.push("removed", 4, "x").unwrap();
    assert_eq!(
        q.push("OLDER", 4, "x"),
        Err(Error::Duplicate { key: "older".into() })
    );
    assert_eq!(q.pop().unwrap().key, "older");
    assert_eq!(q.pop().unwrap().key, "removed");
}

#[test]
fn spec_r8_new_len_and_is_empty() {
    let mut first = IndexedHeap::new();
    let second = IndexedHeap::new();
    assert_eq!(first.len(), 0);
    assert_eq!(first.is_empty(), first.len() == 0);
    first.push("a", 0, "").unwrap();
    assert_eq!(first.len(), 1);
    assert!(!first.is_empty());
    assert_eq!(second.len(), 0);
    assert!(second.is_empty());
}

#[test]
fn spec_r9_push_inserts_new_item() {
    let mut q = IndexedHeap::new();
    assert_eq!(q.push("Key-1", -2, "value"), Ok(()));
    assert_eq!(q.len(), 1);
    assert_eq!(q.get("key-1").unwrap(), Some(Item {
        key: "key-1".into(),
        priority: -2,
        value: "value".into(),
    }));
}

#[test]
fn spec_r10_duplicate_push_keeps_existing_item_and_age() {
    let mut q = IndexedHeap::new();
    q.push("First", 1, "x").unwrap();
    q.push("Second", 1, "x").unwrap();
    assert_eq!(
        q.push(" FIRST ", 1, "x"),
        Err(Error::Duplicate { key: "first".into() })
    );
    assert_eq!(q.get("first").unwrap().unwrap().value, "x");
    assert_eq!(q.pop().unwrap().key, "first");
    assert_eq!(q.pop().unwrap().key, "second");
}

#[test]
fn spec_r11_peek_returns_best_owned_item_without_removal() {
    let mut q = IndexedHeap::new();
    q.push("later", 3, "v").unwrap();
    q.push("best", 1, "v").unwrap();
    let mut item = q.peek().unwrap();
    item.key = "changed".into();
    item.priority = 99;
    item.value = "changed".into();
    assert_eq!(q.len(), 2);
    assert_eq!(q.peek().unwrap(), Item {
        key: "best".into(),
        priority: 1,
        value: "v".into(),
    });
}

#[test]
fn spec_r12_pop_removes_in_full_rank_order() {
    let mut q = IndexedHeap::new();
    q.push("priority", -1, "zz").unwrap();
    q.push("short", 0, "a").unwrap();
    q.push("same1", 0, "bb").unwrap();
    q.push("same2", 0, "bb").unwrap();
    assert_eq!(q.pop().unwrap().key, "priority");
    assert_eq!(q.pop().unwrap().key, "short");
    assert_eq!(q.pop().unwrap().key, "same1");
    assert_eq!(q.pop().unwrap().key, "same2");
    assert_eq!(q.pop(), None);
}

#[test]
fn spec_r13_get_validates_even_when_empty_and_returns_owned_clone() {
    let mut q = IndexedHeap::new();
    assert_eq!(q.get("9bad"), Err(Error::InvalidKey { at: 0 }));
    assert_eq!(q.get("missing"), Ok(None));
    q.push("present", 2, "original").unwrap();
    let mut item = q.get("PRESENT").unwrap().unwrap();
    item.value.push_str(" changed");
    assert_eq!(q.get("present").unwrap().unwrap().value, "original");
}

#[test]
fn spec_r14_reads_and_returned_items_do_not_change_queue() {
    let mut q = IndexedHeap::new();
    q.push("a", 0, "x").unwrap();
    q.push("b", 0, "x").unwrap();
    for _ in 0..4 {
        let _ = q.peek();
        let _ = q.get("a").unwrap();
    }
    assert_eq!(q.pop().unwrap().key, "a");
    assert_eq!(q.pop().unwrap().key, "b");
}

#[test]
fn spec_r15_values_preserve_arbitrary_string_bytes() {
    let value = " \t\nÉ\u{0000}e\u{301} ";
    let mut q = IndexedHeap::new();
    q.push("value", 0, value).unwrap();
    assert_eq!(q.get("value").unwrap().unwrap().value.as_bytes(), value.as_bytes());
    assert_eq!(q.replace_value("value", "").unwrap().as_bytes(), value.as_bytes());
    assert_eq!(q.get("value").unwrap().unwrap().value, "");
}

#[test]
fn spec_r16_all_signed_priorities_compare_without_overflow() {
    let mut q = IndexedHeap::new();
    q.push("max", i64::MAX, "").unwrap();
    q.push("zero", 0, "").unwrap();
    q.push("min", i64::MIN, "").unwrap();
    q.push("negative", -1, "").unwrap();
    assert_eq!(q.pop().unwrap().key, "min");
    assert_eq!(q.pop().unwrap().key, "negative");
    assert_eq!(q.pop().unwrap().key, "zero");
    assert_eq!(q.pop().unwrap().key, "max");
}

#[test]
fn spec_r17_pop_then_get_and_reinsert_key() {
    let mut q = IndexedHeap::new();
    q.push("reusable", 0, "old").unwrap();
    assert_eq!(q.pop().unwrap().key, "reusable");
    assert_eq!(q.get("reusable"), Ok(None));
    assert_eq!(q.push("REUSABLE", 1, "new"), Ok(()));
    assert_eq!(q.get("reusable").unwrap().unwrap().value, "new");
}

#[test]
fn spec_r18_queues_have_independent_keys_and_histories() {
    let mut a = IndexedHeap::new();
    let mut b = IndexedHeap::new();
    a.push("shared", 0, "a").unwrap();
    b.push("SHARED", 10, "b").unwrap();
    b.push("other", -1, "b").unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 2);
    assert_eq!(a.get("shared").unwrap().unwrap().value, "a");
    assert_eq!(b.get("shared").unwrap().unwrap().value, "b");
    assert_eq!(b.pop().unwrap().key, "other");
    assert_eq!(a.len(), 1);
}

#[test]
fn spec_r19_errors_and_empty_none_are_atomic() {
    let mut q = IndexedHeap::new();
    assert_eq!(q.pop(), None);
    assert_eq!(q.peek(), None);
    assert_eq!(q.len(), 0);
    assert_eq!(q.push("a", 1, "x"), Ok(()));
    let before = q.get("a").unwrap();
    assert!(q.push(" A ", 2, "y").is_err());
    assert!(q.decrease_key("a", 1).is_err());
    assert!(q.increase_key("a", 1).is_err());
    assert!(q.replace_value("missing", "z").is_err());
    assert!(q.remove("missing").is_err());
    assert_eq!(q.get("a").unwrap(), before);
    assert_eq!(q.len(), 1);
}

#[test]
fn spec_r20_only_retained_items_affect_length_and_rank() {
    let mut q = IndexedHeap::new();
    q.push("a", 10, "").unwrap();
    q.push("b", 0, "").unwrap();
    q.push("c", 5, "").unwrap();
    assert_eq!(q.len(), 3);
    q.remove("b").unwrap();
    assert_eq!(q.len(), 2);
    assert_eq!(q.peek().unwrap().key, "c");
    q.pop();
    assert_eq!(q.len(), 1);
    assert_eq!(q.peek().unwrap().key, "a");
}

#[test]
fn spec_r21_decrease_key_lowers_existing_priority_and_returns_old() {
    let mut q = IndexedHeap::new();
    q.push("a", 8, "kept").unwrap();
    q.push("b", 3, "").unwrap();
    assert_eq!(q.decrease_key("A", 1), Ok(8));
    assert_eq!(q.get("a").unwrap().unwrap(), Item {
        key: "a".into(),
        priority: 1,
        value: "kept".into(),
    });
    assert_eq!(q.peek().unwrap().key, "a");
}

#[test]
fn spec_r22_increase_key_raises_existing_priority_and_returns_old() {
    let mut q = IndexedHeap::new();
    q.push("a", 1, "kept").unwrap();
    assert_eq!(q.increase_key("a", 9), Ok(1));
    assert_eq!(q.get("a").unwrap().unwrap().priority, 9);
    assert_eq!(q.get("a").unwrap().unwrap().value, "kept");
}

#[test]
fn spec_r23_wrong_direction_includes_equality_and_handles_extremes() {
    let mut q = IndexedHeap::new();
    q.push("min", i64::MIN, "").unwrap();
    q.push("max", i64::MAX, "").unwrap();
    assert_eq!(
        q.decrease_key("min", i64::MIN),
        Err(Error::WrongDirection { current: i64::MIN, requested: i64::MIN })
    );
    assert_eq!(
        q.increase_key("max", i64::MAX),
        Err(Error::WrongDirection { current: i64::MAX, requested: i64::MAX })
    );
    assert_eq!(
        q.increase_key("min", i64::MIN),
        Err(Error::WrongDirection { current: i64::MIN, requested: i64::MIN })
    );
    assert_eq!(
        q.decrease_key("max", i64::MAX),
        Err(Error::WrongDirection { current: i64::MAX, requested: i64::MAX })
    );
}

#[test]
fn spec_r24_key_update_error_precedence_and_atomicity() {
    let mut q = IndexedHeap::new();
    q.push("a", 5, "v").unwrap();
    assert_eq!(q.decrease_key("bad_key", 1), Err(Error::InvalidKey { at: 3 }));
    assert_eq!(
        q.increase_key("missing", 10),
        Err(Error::Missing { key: "missing".into() })
    );
    assert_eq!(
        q.decrease_key("a", 5),
        Err(Error::WrongDirection { current: 5, requested: 5 })
    );
    assert_eq!(q.get("a").unwrap().unwrap().priority, 5);
    assert_eq!(q.get("a").unwrap().unwrap().value, "v");
}

#[test]
fn spec_r25_replace_value_returns_old_and_reorders_by_byte_length() {
    let mut q = IndexedHeap::new();
    q.push("a", 0, "long").unwrap();
    q.push("b", 0, "x").unwrap();
    assert_eq!(q.replace_value("a", "z"), Ok("long".into()));
    assert_eq!(q.peek().unwrap().key, "a");
    assert_eq!(q.replace_value("a", "É"), Ok("z".into()));
    assert_eq!(q.get("a").unwrap().unwrap().value.as_bytes(), "É".as_bytes());
    assert_eq!(q.replace_value("a", "É"), Ok("É".into()));
}

#[test]
fn spec_r26_remove_any_rank_and_keep_moved_slots_accessible() {
    let mut q = IndexedHeap::new();
    q.push("a", 0, "").unwrap();
    q.push("b", 1, "").unwrap();
    q.push("c", 2, "").unwrap();
    q.push("d", 3, "").unwrap();
    assert_eq!(q.remove("c").unwrap().key, "c");
    assert_eq!(q.get("a").unwrap().unwrap().key, "a");
    assert_eq!(q.get("b").unwrap().unwrap().key, "b");
    assert_eq!(q.get("d").unwrap().unwrap().key, "d");
    assert_eq!(q.remove("d").unwrap().key, "d");
    assert_eq!(q.len(), 2);
}

#[test]
fn spec_r27_updates_preserve_age_across_priority_and_length_ties() {
    let mut q = IndexedHeap::new();
    q.push("first", 0, "x").unwrap();
    q.push("second", 0, "x").unwrap();
    q.decrease_key("first", -1).unwrap();
    q.increase_key("first", 0).unwrap();
    q.replace_value("first", "long").unwrap();
    q.replace_value("first", "x").unwrap();
    assert_eq!(q.pop().unwrap().key, "first");
    assert_eq!(q.len(), 1);
    assert_eq!(q.pop().unwrap().key, "second");
}

#[test]
fn spec_r28_replace_and_remove_validate_before_lookup_and_keep_errors_atomic() {
    let mut q = IndexedHeap::new();
    q.push("present", 0, "v").unwrap();
    let before = q.get("present").unwrap();
    assert_eq!(q.replace_value("9bad", "new"), Err(Error::InvalidKey { at: 0 }));
    assert_eq!(q.remove("9bad"), Err(Error::InvalidKey { at: 0 }));
    assert_eq!(
        q.replace_value("absent", "new"),
        Err(Error::Missing { key: "absent".into() })
    );
    assert_eq!(
        q.remove("absent"),
        Err(Error::Missing { key: "absent".into() })
    );
    assert_eq!(q.get("present").unwrap(), before);
}

#[test]
fn spec_r29_merge_public_types_and_report_counts() {
    fn assert_policy_traits<T: std::fmt::Debug + Clone + Copy + PartialEq + Eq>() {}
    fn assert_report_traits<T: std::fmt::Debug + Clone + PartialEq + Eq>() {}
    assert_policy_traits::<CollisionPolicy>();
    assert_report_traits::<MergeReport>();

    let mut receiver = IndexedHeap::new();
    let mut source = IndexedHeap::new();
    receiver.push("same", 5, "old").unwrap();
    source.push("same", 1, "new").unwrap();
    source.push("moved", 0, "").unwrap();
    let report = receiver.merge(&mut source, CollisionPolicy::KeepSelf).unwrap();
    assert_eq!(
        report,
        MergeReport { moved: 1, replaced: 0, discarded: 1 }
    );
}

#[test]
fn spec_r30_successful_merge_empties_source_and_preserves_unmentioned_receiver_items() {
    let mut receiver = IndexedHeap::new();
    let mut source = IndexedHeap::new();
    receiver.push("keep", 4, "receiver").unwrap();
    receiver.push("collision", 8, "receiver value").unwrap();
    source.push("collision", 1, "source value").unwrap();
    source.push("new", 2, "transferred").unwrap();
    receiver.merge(&mut source, CollisionPolicy::KeepSelf).unwrap();
    assert!(source.is_empty());
    assert_eq!(source.len(), 0);
    assert_eq!(receiver.get("keep").unwrap().unwrap(), Item {
        key: "keep".into(),
        priority: 4,
        value: "receiver".into(),
    });
    assert_eq!(receiver.get("collision").unwrap().unwrap().value, "receiver value");
    assert_eq!(receiver.get("new").unwrap().unwrap().value, "transferred");
}

#[test]
fn spec_r31_keep_self_discards_collision_without_changing_receiver_item() {
    let mut receiver = IndexedHeap::new();
    let mut source = IndexedHeap::new();
    receiver.push("same", 7, "receiver").unwrap();
    source.push("SAME", -5, "source").unwrap();
    let report = receiver.merge(&mut source, CollisionPolicy::KeepSelf).unwrap();
    assert_eq!(report, MergeReport { moved: 0, replaced: 0, discarded: 1 });
    assert_eq!(receiver.get("same").unwrap().unwrap(), Item {
        key: "same".into(),
        priority: 7,
        value: "receiver".into(),
    });
}

#[test]
fn spec_r32_take_other_replaces_data_but_retains_receiver_age() {
    let mut receiver = IndexedHeap::new();
    let mut source = IndexedHeap::new();
    receiver.push("collision", 10, "old").unwrap();
    receiver.push("later", 10, "old").unwrap();
    source.push("COLLISION", 1, "new").unwrap();
    let report = receiver.merge(&mut source, CollisionPolicy::TakeOther).unwrap();
    assert_eq!(report, MergeReport { moved: 0, replaced: 1, discarded: 0 });
    assert_eq!(receiver.get("collision").unwrap().unwrap(), Item {
        key: "collision".into(),
        priority: 1,
        value: "new".into(),
    });
    assert_eq!(receiver.pop().unwrap().key, "collision");
    assert_eq!(receiver.pop().unwrap().key, "later");
}

#[test]
fn spec_r33_reject_reports_first_collision_in_source_rank_order_and_is_atomic() {
    let mut receiver = IndexedHeap::new();
    let mut source = IndexedHeap::new();
    receiver.push("zeta", 0, "").unwrap();
    receiver.push("alpha", 0, "").unwrap();
    source.push("zeta", 4, "long").unwrap();
    source.push("alpha", 0, "x").unwrap();
    source.push("omega", -1, "").unwrap();
    let before_receiver = (receiver.len(), receiver.get("zeta").unwrap(), receiver.get("alpha").unwrap());
    let before_source = (source.len(), source.get("zeta").unwrap(), source.get("alpha").unwrap(), source.get("omega").unwrap());
    assert_eq!(
        receiver.merge(&mut source, CollisionPolicy::Reject),
        Err(Error::Collision { key: "alpha".into() })
    );
    assert_eq!(receiver.len(), before_receiver.0);
    assert_eq!(receiver.get("zeta").unwrap(), before_receiver.1);
    assert_eq!(receiver.get("alpha").unwrap(), before_receiver.2);
    assert_eq!(source.len(), before_source.0);
    assert_eq!(source.get("zeta").unwrap(), before_source.1);
    assert_eq!(source.get("alpha").unwrap(), before_source.2);
    assert_eq!(source.get("omega").unwrap(), before_source.3);
}

#[test]
fn spec_r34_merge_transfers_by_source_rank_with_fresh_receiver_ages() {
    let mut receiver = IndexedHeap::new();
    let mut source = IndexedHeap::new();
    receiver.push("old", 0, "x").unwrap();
    source.push("source-young", 0, "x").unwrap();
    source.push("source-old", 0, "x").unwrap();
    receiver.merge(&mut source, CollisionPolicy::KeepSelf).unwrap();
    assert_eq!(receiver.pop().unwrap().key, "old");
    assert_eq!(receiver.pop().unwrap().key, "source-old");
    assert_eq!(receiver.pop().unwrap().key, "source-young");
}

#[test]
fn spec_r35_merge_report_counts_partition_source_items_and_empty_merge_zero() {
    let mut receiver = IndexedHeap::new();
    let mut source = IndexedHeap::new();
    receiver.push("keep", 0, "").unwrap();
    receiver.push("replace", 0, "").unwrap();
    source.push("move", -1, "").unwrap();
    source.push("replace", -2, "").unwrap();
    source.push("discard", -3, "").unwrap();
    receiver.push("discard", 0, "").unwrap();

    let report = receiver.merge(&mut source, CollisionPolicy::TakeOther).unwrap();
    assert_eq!(report, MergeReport { moved: 1, replaced: 2, discarded: 0 });
    assert_eq!(report.moved + report.replaced + report.discarded, 3);

    let empty_report = receiver.merge(&mut source, CollisionPolicy::Reject).unwrap();
    assert_eq!(empty_report, MergeReport { moved: 0, replaced: 0, discarded: 0 });
}

#[test]
fn spec_r36_emptied_source_is_reusable_and_empty_receiver_rebases_source_rank() {
    let mut receiver = IndexedHeap::new();
    let mut source = IndexedHeap::new();
    source.push("young", 0, "x").unwrap();
    source.push("old", 0, "x").unwrap();
    receiver.merge(&mut source, CollisionPolicy::Reject).unwrap();
    assert!(source.is_empty());
    source.push("again", 0, "x").unwrap();
    assert_eq!(source.get("again").unwrap().unwrap().key, "again");
    assert_eq!(source.merge(&mut receiver, CollisionPolicy::Reject).unwrap().moved, 2);
    assert!(receiver.is_empty());
    assert_eq!(source.pop().unwrap().key, "old");
    assert_eq!(source.pop().unwrap().key, "young");
    assert_eq!(source.pop().unwrap().key, "again");
}
