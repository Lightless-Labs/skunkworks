#[test]
fn hidden_snapshot_uncommitted() {
    let mut s = Store::new();
    s.set("stage", "before");
    s.begin();
    s.set("stage", "uncommitted");
    s.snapshot(100);
    s.rollback().unwrap();
    s.restore(100).unwrap();
    assert_eq!(s.get("stage"), Some("uncommitted".into()));
    assert_eq!(s.rollback(), Err(Error::NoTransaction));
}

#[test]
fn hidden_restore_preserves_frames() {
    let mut s = Store::new();
    s.set("flow", "snap");
    s.snapshot(101);
    s.set("flow", "begin-value");
    s.begin();
    s.set("flow", "edit");
    s.restore(101).unwrap();
    assert_eq!(s.get("flow"), Some("snap".into()));
    s.rollback().unwrap();
    assert_eq!(s.get("flow"), Some("begin-value".into()));
}

#[test]
fn hidden_snapshot_overwrite() {
    let mut s = Store::new();
    s.set("version", "v1");
    s.snapshot(102);
    s.set("version", "v2");
    s.snapshot(102);
    s.set("version", "v3");
    s.restore(102).unwrap();
    assert_eq!(s.get("version"), Some("v2".into()));
}

#[test]
fn hidden_count_nested_deletion() {
    let mut s = Store::new();
    s.set("d-a", "target");
    s.set("d-b", "target");
    s.begin();
    s.delete("d-a");
    assert_eq!(s.count("target"), 1);
    s.begin();
    s.delete("d-b");
    assert_eq!(s.count("target"), 0);
    s.rollback().unwrap();
    assert_eq!(s.count("target"), 1);
    s.rollback().unwrap();
    assert_eq!(s.count("target"), 2);
}

#[test]
fn hidden_empty_snapshot_clears() {
    let mut s = Store::new();
    s.snapshot(0);
    s.set("extraneous", "remove-me");
    s.restore(0).unwrap();
    assert_eq!(s.get("extraneous"), None);
    assert_eq!(s.count("remove-me"), 0);
}

#[test]
fn hidden_unknown_snapshot_atomic() {
    let mut s = Store::new();
    s.set("atomic", "base-a");
    s.begin();
    s.set("atomic", "edited-a");
    assert_eq!(s.restore(103), Err(Error::UnknownSnapshot(103)));
    assert_eq!(s.get("atomic"), Some("edited-a".into()));
    s.rollback().unwrap();
    assert_eq!(s.get("atomic"), Some("base-a".into()));
}

#[test]
fn hidden_snapshots_survive_commit() {
    let mut s = Store::new();
    s.begin();
    s.set("persist", "saved-c");
    s.snapshot(104);
    s.set("persist", "commit-c");
    s.commit().unwrap();
    s.restore(104).unwrap();
    assert_eq!(s.get("persist"), Some("saved-c".into()));
}

#[test]
fn hidden_snapshots_survive_rollback() {
    let mut s = Store::new();
    s.begin();
    s.set("persist-r", "saved-r");
    s.snapshot(105);
    s.rollback().unwrap();
    s.restore(105).unwrap();
    assert_eq!(s.get("persist-r"), Some("saved-r".into()));
}

#[test]
fn hidden_restore_reusable_copies() {
    let mut s = Store::new();
    s.set("repeat", "frozen");
    s.snapshot(106);
    for i in 0..8 {
        s.restore(106).unwrap();
        assert_eq!(s.get("repeat"), Some("frozen".into()));
        s.set("repeat", &i.to_string());
        s.delete("repeat");
    }
}

#[test]
fn hidden_snapshot_ids_independent() {
    let mut s = Store::new();
    s.set("id", "low");
    s.snapshot(0);
    s.set("id", "high");
    s.snapshot(u64::MAX);
    s.restore(0).unwrap();
    assert_eq!(s.get("id"), Some("low".into()));
    s.restore(u64::MAX).unwrap();
    assert_eq!(s.get("id"), Some("high".into()));
}

#[test]
fn hidden_restore_all_nested_boundaries() {
    let mut s = Store::new();
    s.set("nest", "initial");
    s.snapshot(107);
    s.set("nest", "outer-begin");
    s.begin();
    s.set("nest", "inner-begin");
    s.begin();
    s.set("nest", "deep");
    s.restore(107).unwrap();
    s.rollback().unwrap();
    assert_eq!(s.get("nest"), Some("inner-begin".into()));
    s.rollback().unwrap();
    assert_eq!(s.get("nest"), Some("outer-begin".into()));
}

#[test]
fn hidden_restore_deletes_absent_keys() {
    let mut s = Store::new();
    s.set("kept", "snapshot-val");
    s.snapshot(108);
    s.delete("kept");
    s.set("extra-1", "extra");
    s.set("extra-2", "extra");
    s.restore(108).unwrap();
    assert_eq!(s.count("extra"), 0);
    assert_eq!(s.get("kept"), Some("snapshot-val".into()));
}

#[test]
fn hidden_snapshot_does_not_save_registry() {
    let mut s = Store::new();
    s.snapshot(109);
    s.set("registry", "later");
    s.snapshot(110);
    s.restore(109).unwrap();
    s.restore(110).unwrap();
    assert_eq!(s.get("registry"), Some("later".into()));
}

#[test]
fn hidden_empty_key_snapshot_delete() {
    let mut s = Store::new();
    s.set("", "");
    s.snapshot(111);
    s.begin();
    s.delete("");
    assert_eq!(s.count(""), 0);
    s.restore(111).unwrap();
    assert_eq!(s.count(""), 1);
    s.rollback().unwrap();
    assert_eq!(s.get(""), Some(String::new()));
}

#[test]
fn hidden_large_snapshot_and_counts() {
    let mut s = Store::new();
    for i in 0..160 {
        s.set(&format!("bulk-{i}"), "bulk-value");
    }
    s.snapshot(112);
    s.begin();
    for i in 0..80 {
        s.delete(&format!("bulk-{i}"));
    }
    assert_eq!(s.count("bulk-value"), 80);
    s.restore(112).unwrap();
    assert_eq!(s.count("bulk-value"), 160);
    s.commit().unwrap();
    assert_eq!(s.count("bulk-value"), 160);
}

#[test]
fn hidden_commit_after_restored_data() {
    let mut s = Store::new();
    s.set("commit-restored", "snapshot-state");
    s.snapshot(113);
    s.begin();
    s.begin();
    s.set("commit-restored", "temporary-state");
    s.restore(113).unwrap();
    s.commit().unwrap();
    assert_eq!(s.get("commit-restored"), Some("snapshot-state".into()));
    assert_eq!(s.rollback(), Err(Error::NoTransaction));
}
