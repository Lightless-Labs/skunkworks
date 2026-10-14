#[test]
fn hidden_exact_expiry_boundary() {
    let mut s = Store::new();
    s.set("boundary", "edge", Some(4), 20).unwrap();
    assert_eq!(s.get("boundary", 23), Ok(Some("edge".into())));
    assert_eq!(s.get("boundary", 24), Ok(None));
    assert_eq!(s.count("edge", 24), Ok(0));
    assert_eq!(s.delete("boundary", 24), Ok(false));
}

#[test]
fn hidden_zero_ttl_and_empty_strings() {
    let mut s = Store::new();
    s.set("", "", None, 0).unwrap();
    assert_eq!(s.get("", 0), Ok(Some(String::new())));
    s.set("", "replacement", Some(0), 0).unwrap();
    assert_eq!(s.get("", 0), Ok(None));
    assert_eq!(s.count("replacement", 0), Ok(0));
    s.set("new-zero", "never-visible", Some(0), 0).unwrap();
    assert_eq!(s.delete("new-zero", 0), Ok(false));
}

#[test]
fn hidden_ttl_replacement_resets() {
    let mut s = Store::new();
    s.set("replace", "short", Some(2), 10).unwrap();
    s.set("replace", "permanent", None, 11).unwrap();
    assert_eq!(s.get("replace", 12), Ok(Some("permanent".into())));
    s.set("replace", "longer", Some(7), 12).unwrap();
    s.set("replace", "latest", Some(2), 13).unwrap();
    assert_eq!(s.count("longer", 14), Ok(0));
    assert_eq!(s.get("replace", 15), Ok(None));
}

#[test]
fn hidden_commit_error_advances_clock() {
    let mut s = Store::new();
    s.set("aging", "expires", Some(5), 10).unwrap();
    assert_eq!(s.commit(16), Err(Error::NoTransaction));
    assert_eq!(
        s.get("aging", 15),
        Err(Error::ClockWentBackwards { last: 16, now: 15 })
    );
    assert_eq!(s.get("aging", 16), Ok(None));
}

#[test]
fn hidden_other_errors_advance_clock() {
    let mut s = Store::new();
    s.set("age-r", "temporary", Some(3), 30).unwrap();
    assert_eq!(s.rollback(33), Err(Error::NoTransaction));
    assert_eq!(
        s.begin(32),
        Err(Error::ClockWentBackwards { last: 33, now: 32 })
    );
    assert_eq!(s.restore(800, 40), Err(Error::UnknownSnapshot(800)));
    assert_eq!(
        s.count("temporary", 39),
        Err(Error::ClockWentBackwards { last: 40, now: 39 })
    );
    assert_eq!(s.get("age-r", 40), Ok(None));
}

#[test]
fn hidden_overflow_preserves_live_value() {
    let mut s = Store::new();
    s.set("overflow", "safe-max", None, 0).unwrap();
    s.set("aging-max", "expiring-max", Some(2), 0).unwrap();
    assert_eq!(
        s.set("overflow", "bad-max", Some(3), u64::MAX - 1),
        Err(Error::DeadlineOverflow)
    );
    assert_eq!(
        s.get("overflow", u64::MAX - 2),
        Err(Error::ClockWentBackwards {
            last: u64::MAX - 1,
            now: u64::MAX - 2
        })
    );
    assert_eq!(s.get("overflow", u64::MAX - 1), Ok(Some("safe-max".into())));
    assert_eq!(s.count("expiring-max", u64::MAX - 1), Ok(0));
    assert_eq!(s.count("bad-max", u64::MAX - 1), Ok(0));
}

#[test]
fn hidden_max_deadline_and_permanent() {
    let mut s = Store::new();
    s.set("finite-max", "finite", Some(2), u64::MAX - 2)
        .unwrap();
    s.set("forever-max", "forever", None, u64::MAX - 2).unwrap();
    assert_eq!(s.get("finite-max", u64::MAX - 1), Ok(Some("finite".into())));
    assert_eq!(s.get("finite-max", u64::MAX), Ok(None));
    assert_eq!(s.get("forever-max", u64::MAX), Ok(Some("forever".into())));
    s.set("forever-max", "zero", Some(0), u64::MAX).unwrap();
    assert_eq!(s.get("forever-max", u64::MAX), Ok(None));
    assert_eq!(
        s.set("overflow-new", "v", Some(1), u64::MAX),
        Err(Error::DeadlineOverflow)
    );
}

#[test]
fn hidden_rollback_never_revives_expired() {
    let mut s = Store::new();
    s.set("rb-expired", "old-rb", Some(5), 50).unwrap();
    s.begin(51).unwrap();
    s.set("rb-expired", "permanent-rb", None, 52).unwrap();
    s.rollback(55).unwrap();
    assert_eq!(s.get("rb-expired", 55), Ok(None));
    assert_eq!(s.count("old-rb", 55), Ok(0));
    assert_eq!(
        s.get("rb-expired", 54),
        Err(Error::ClockWentBackwards { last: 55, now: 54 })
    );
}

#[test]
fn hidden_live_saved_value_reappears() {
    let mut s = Store::new();
    s.set("rb-live", "saved-live", Some(12), 60).unwrap();
    s.begin(61).unwrap();
    s.set("rb-live", "replacement-short", Some(1), 62).unwrap();
    assert_eq!(s.get("rb-live", 63), Ok(None));
    s.rollback(64).unwrap();
    assert_eq!(s.get("rb-live", 71), Ok(Some("saved-live".into())));
    assert_eq!(s.get("rb-live", 72), Ok(None));
}

#[test]
fn hidden_snapshot_deadline_not_rebased() {
    let mut s = Store::new();
    s.set("absolute", "snap-live", Some(10), 70).unwrap();
    s.snapshot(801, 72).unwrap();
    s.delete("absolute", 73).unwrap();
    s.restore(801, 78).unwrap();
    assert_eq!(s.get("absolute", 79), Ok(Some("snap-live".into())));
    assert_eq!(s.get("absolute", 80), Ok(None));
    s.restore(801, 81).unwrap();
    assert_eq!(s.get("absolute", 81), Ok(None));
}

#[test]
fn hidden_restore_preserves_nested_scopes() {
    let mut s = Store::new();
    s.set("scope", "snap-old", None, 90).unwrap();
    s.snapshot(802, 90).unwrap();
    s.set("scope", "outer-start", Some(8), 91).unwrap();
    s.begin(92).unwrap();
    s.set("scope", "inner-start", None, 93).unwrap();
    s.begin(94).unwrap();
    s.restore(802, 95).unwrap();
    s.rollback(96).unwrap();
    assert_eq!(s.get("scope", 96), Ok(Some("inner-start".into())));
    s.rollback(97).unwrap();
    assert_eq!(s.get("scope", 98), Ok(Some("outer-start".into())));
    assert_eq!(s.get("scope", 99), Ok(None));
}

#[test]
fn hidden_commit_closes_all_with_expiry() {
    let mut s = Store::new();
    s.set("commit-exp", "soon", Some(4), 100).unwrap();
    s.begin(101).unwrap();
    s.set("outer-t", "stable-t", None, 101).unwrap();
    s.begin(102).unwrap();
    s.set("inner-t", "stable-t", None, 102).unwrap();
    s.commit(104).unwrap();
    assert_eq!(s.count("soon", 104), Ok(0));
    assert_eq!(s.count("stable-t", 104), Ok(2));
    assert_eq!(s.rollback(104), Err(Error::NoTransaction));
}

#[test]
fn hidden_snapshot_overwrite_and_extreme_ids() {
    let mut s = Store::new();
    s.set("ids-t", "first-t", None, 110).unwrap();
    s.snapshot(0, 110).unwrap();
    s.set("ids-t", "second-t", None, 111).unwrap();
    s.snapshot(0, 111).unwrap();
    s.snapshot(u64::MAX, 111).unwrap();
    s.delete("ids-t", 112).unwrap();
    s.restore(0, 113).unwrap();
    assert_eq!(s.get("ids-t", 113), Ok(Some("second-t".into())));
    s.delete("ids-t", 113).unwrap();
    s.restore(u64::MAX, 114).unwrap();
    assert_eq!(s.get("ids-t", 114), Ok(Some("second-t".into())));
}

#[test]
fn hidden_snapshots_persist_across_tx() {
    let mut s = Store::new();
    s.snapshot(803, 120).unwrap();
    s.begin(120).unwrap();
    s.set("persist-t", "captured-t", None, 121).unwrap();
    s.snapshot(804, 121).unwrap();
    s.rollback(122).unwrap();
    s.restore(804, 123).unwrap();
    assert_eq!(s.get("persist-t", 123), Ok(Some("captured-t".into())));
    s.begin(123).unwrap();
    s.delete("persist-t", 124).unwrap();
    s.commit(124).unwrap();
    s.restore(803, 125).unwrap();
    s.restore(804, 126).unwrap();
    assert_eq!(s.count("captured-t", 126), Ok(1));
}

#[test]
fn hidden_clock_precedence_all_operations() {
    let mut s = Store::new();
    s.snapshot(805, 130).unwrap();
    let e = || Error::ClockWentBackwards {
        last: 130,
        now: 129,
    };
    assert_eq!(s.get("x-t", 129), Err(e()));
    assert_eq!(s.count("x-t", 129), Err(e()));
    assert_eq!(s.delete("x-t", 129), Err(e()));
    assert_eq!(s.set("x-t", "x-t", Some(u64::MAX), 129), Err(e()));
    assert_eq!(s.begin(129), Err(e()));
    assert_eq!(s.commit(129), Err(e()));
    assert_eq!(s.rollback(129), Err(e()));
    assert_eq!(s.snapshot(805, 129), Err(e()));
    assert_eq!(s.restore(9999, 129), Err(e()));
    assert_eq!(s.restore(805, 130), Ok(()));
    assert_eq!(s.rollback(130), Err(Error::NoTransaction));
}

#[test]
fn hidden_backward_operations_are_atomic() {
    let mut s = Store::new();
    s.set("atomic-t", "snapshot-t", Some(20), 140).unwrap();
    s.snapshot(806, 140).unwrap();
    s.begin(141).unwrap();
    s.set("atomic-t", "current-t", None, 142).unwrap();
    let e = || Error::ClockWentBackwards {
        last: 142,
        now: 141,
    };
    assert_eq!(s.set("atomic-t", "bad-t", None, 141), Err(e()));
    assert_eq!(s.delete("atomic-t", 141), Err(e()));
    assert_eq!(s.snapshot(806, 141), Err(e()));
    assert_eq!(s.restore(806, 141), Err(e()));
    assert_eq!(s.rollback(141), Err(e()));
    assert_eq!(s.commit(141), Err(e()));
    assert_eq!(s.begin(141), Err(e()));
    assert_eq!(s.get("atomic-t", 142), Ok(Some("current-t".into())));
    s.rollback(143).unwrap();
    assert_eq!(s.get("atomic-t", 143), Ok(Some("snapshot-t".into())));
    s.delete("atomic-t", 143).unwrap();
    s.restore(806, 144).unwrap();
    assert_eq!(s.get("atomic-t", 144), Ok(Some("snapshot-t".into())));
    assert_eq!(s.get("atomic-t", 160), Ok(None));
    assert_eq!(s.rollback(160), Err(Error::NoTransaction));
}

#[test]
fn hidden_unknown_restore_keeps_scopes() {
    let mut s = Store::new();
    s.set("unknown-t", "original-t", None, 170).unwrap();
    s.begin(171).unwrap();
    s.set("unknown-t", "edited-t", None, 172).unwrap();
    assert_eq!(s.restore(807, 173), Err(Error::UnknownSnapshot(807)));
    assert_eq!(s.get("unknown-t", 173), Ok(Some("edited-t".into())));
    s.rollback(174).unwrap();
    assert_eq!(s.get("unknown-t", 174), Ok(Some("original-t".into())));
}

#[test]
fn hidden_mixed_counts_nested_deletes() {
    let mut s = Store::new();
    s.set("c-a", "same-t", Some(5), 180).unwrap();
    s.set("c-b", "same-t", None, 180).unwrap();
    s.set("c-c", "same-t", Some(10), 180).unwrap();
    s.begin(181).unwrap();
    s.delete("c-b", 182).unwrap();
    s.begin(183).unwrap();
    s.set("c-c", "other-t", None, 183).unwrap();
    assert_eq!(s.count("same-t", 185), Ok(0));
    s.rollback(186).unwrap();
    assert_eq!(s.count("same-t", 186), Ok(1));
    s.rollback(187).unwrap();
    assert_eq!(s.count("same-t", 187), Ok(2));
    assert_eq!(s.count("same-t", 190), Ok(1));
}

#[test]
fn hidden_large_equal_time_and_owned_data() {
    let mut s = Store::new();
    let mut value = String::from("雪-t");
    for i in 0..128 {
        s.set(
            &format!("bulk-t-{i}"),
            &value,
            if i % 2 == 0 { Some(2) } else { None },
            200,
        )
        .unwrap();
    }
    value.clear();
    assert_eq!(s.count("雪-t", 200), Ok(128));
    for _ in 0..48 {
        s.begin(200).unwrap();
    }
    s.snapshot(808, 200).unwrap();
    for i in 0..128 {
        s.delete(&format!("bulk-t-{i}"), 201).unwrap();
    }
    s.restore(808, 202).unwrap();
    assert_eq!(s.count("雪-t", 202), Ok(64));
    for _ in 0..48 {
        s.rollback(202).unwrap();
    }
    assert_eq!(s.count("雪-t", 202), Ok(64));
    let mut cloned = s.get("bulk-t-1", 202).unwrap().unwrap();
    cloned.clear();
    assert_eq!(s.get("bulk-t-1", 202), Ok(Some("雪-t".into())));
}

#[test]
fn hidden_expired_before_begin_and_snapshot() {
    let mut s = Store::new();
    s.set("before-boundary", "expired-before", Some(1), 210)
        .unwrap();
    s.begin(211).unwrap();
    s.snapshot(809, 211).unwrap();
    s.set("extra-after", "extra-after", None, 212).unwrap();
    s.rollback(212).unwrap();
    assert_eq!(s.get("before-boundary", 212), Ok(None));
    assert_eq!(s.get("extra-after", 212), Ok(None));
    s.set("later-after", "later-after", None, 213).unwrap();
    s.restore(809, 214).unwrap();
    assert_eq!(s.count("later-after", 214), Ok(0));
    assert_eq!(s.count("expired-before", 214), Ok(0));
}
