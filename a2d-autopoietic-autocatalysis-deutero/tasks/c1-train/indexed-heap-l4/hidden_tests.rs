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

// R16, R21-R24, R27
#[test]
fn hidden_direction_equality_and_extremes() {
    let mut q = IndexedHeap::new(); q.push("edge", 0, "x").unwrap();
    assert_eq!(q.decrease_key("edge", 0), Err(Error::WrongDirection { current: 0, requested: 0 }));
    assert_eq!(q.increase_key("edge", 0), Err(Error::WrongDirection { current: 0, requested: 0 }));
    assert_eq!(q.decrease_key("edge", i64::MIN), Ok(0));
    assert_eq!(q.increase_key("edge", i64::MAX), Ok(i64::MIN));
    assert_eq!(q.decrease_key("edge", i64::MIN), Ok(i64::MAX));
    assert_eq!(q.peek().unwrap().priority, i64::MIN);
}

// R3-R5, R19, R24, R28
#[test]
fn hidden_update_error_precedence() {
    let mut q = IndexedHeap::new();
    assert_eq!(q.decrease_key("  !oops", i64::MAX), Err(Error::InvalidKey { at: 2 }));
    assert_eq!(q.increase_key("  Lost-7 ", i64::MIN), Err(Error::Missing { key: "lost-7".into() }));
    assert_eq!(q.decrease_key(" LOST-7 ", 0), Err(Error::Missing { key: "lost-7".into() }));
    assert_eq!(q.replace_value("  ", "new"), Err(Error::EmptyKey));
    assert_eq!(q.remove("x\t"), Err(Error::InvalidKey { at: 1 }));
    assert_eq!(q.replace_value(" GONE ", ""), Err(Error::Missing { key: "gone".into() }));
    assert_eq!(q.remove(" GONE "), Err(Error::Missing { key: "gone".into() }));
    assert_eq!(q.increase_key(" ", 0), Err(Error::EmptyKey));
    assert_eq!(q.decrease_key(" ", 0), Err(Error::EmptyKey));
    assert!(q.is_empty());
}

// R6-R7, R21-R23, R27
#[test]
fn hidden_priority_round_trip_preserves_age() {
    let mut q = IndexedHeap::new(); q.push("zold", 9, "x").unwrap(); q.push("anew", 9, "y").unwrap();
    q.decrease_key("zold", -9).unwrap(); q.increase_key("zold", 9).unwrap();
    q.increase_key("zold", 100).unwrap(); q.decrease_key("zold", 9).unwrap();
    assert_eq!(q.len(), 2); assert_eq!(q.pop().unwrap().key, "zold");
}

// R6, R15, R25, R27
#[test]
fn hidden_value_round_trip_and_utf8() {
    let mut q = IndexedHeap::new(); q.push("first", 2, "ab").unwrap(); q.push("second", 2, "cd").unwrap();
    assert_eq!(q.replace_value(" FIRST ", "🦀"), Ok("ab".into()));
    assert_eq!(q.peek().unwrap().key, "second");
    assert_eq!(q.replace_value("first", "é"), Ok("🦀".into()));
    assert_eq!(q.peek().unwrap().key, "first");
    assert_eq!(q.replace_value("first", ""), Ok("é".into()));
    assert_eq!(q.replace_value("first", ""), Ok("".into()));
    assert_eq!(q.remove("first").unwrap().value, "");
}

// R13, R17, R26, R28
#[test]
fn hidden_remove_all_slot_shapes() {
    let mut q = IndexedHeap::new();
    for n in 0..65 { q.push(&format!("r{n}"), ((n * 19) % 67) as i64, "v").unwrap(); }
    for n in (0..65).step_by(2) { assert_eq!(q.remove(&format!(" R{n} ")).unwrap().key, format!("r{n}")); }
    for n in 0..65 {
        let found = q.get(&format!("r{n}")).unwrap();
        assert_eq!(found.is_some(), n % 2 == 1);
    }
    let mut priorities = Vec::new(); while let Some(i) = q.pop() { priorities.push(i.priority); }
    assert_eq!(priorities.len(), 32); assert!(priorities.windows(2).all(|w| w[0] <= w[1]));
}

// R7, R19, R21-R24, R27-R28
#[test]
fn hidden_direction_failure_atomicity() {
    let mut q = IndexedHeap::new(); q.push("zfirst", -3, "a").unwrap(); q.push("alast", -3, "b").unwrap();
    assert_eq!(q.increase_key(" ZFIRST ", -4), Err(Error::WrongDirection { current: -3, requested: -4 }));
    assert_eq!(q.decrease_key("alast", 4), Err(Error::WrongDirection { current: -3, requested: 4 }));
    assert_eq!(q.replace_value("zfirst!", ""), Err(Error::InvalidKey { at: 6 }));
    assert_eq!(q.remove("unknown-dead"), Err(Error::Missing { key: "unknown-dead".into() }));
    assert_eq!(q.pop().unwrap().key, "zfirst"); assert_eq!(q.pop().unwrap().key, "alast");
}

// R7, R25-R28
#[test]
fn hidden_remove_and_reinsert_age() {
    let mut q = IndexedHeap::new(); q.push("zzold", 5, "u").unwrap(); q.push("bbstay", 5, "v").unwrap();
    assert_eq!(q.remove(" ZZOLD ").unwrap().priority, 5);
    q.push("zzold", 5, "w").unwrap();
    assert_eq!(q.get("zzold").unwrap().unwrap().value, "w");
    assert_eq!(q.pop().unwrap().key, "bbstay"); assert_eq!(q.pop().unwrap().key, "zzold");
}

// R29-R30, R33
#[test]
fn hidden_reject_uses_source_rank_and_is_atomic() {
    let mut a = IndexedHeap::new(); let mut b = IndexedHeap::new();
    a.push("alpha", 40, "a").unwrap(); a.push("zulu", 40, "b").unwrap();
    b.push("free", -90, "new").unwrap(); b.push("alpha", 2, "x").unwrap(); b.push("zulu", -5, "z").unwrap();
    assert_eq!(a.merge(&mut b, CollisionPolicy::Reject), Err(Error::Collision { key: "zulu".into() }));
    assert_eq!((a.len(), b.len()), (2, 3)); assert_eq!(a.get("free"), Ok(None));
    assert_eq!(a.get("zulu").unwrap().unwrap().priority, 40);
    assert_eq!(b.pop().unwrap().key, "free"); assert_eq!(b.pop().unwrap().key, "zulu");
    assert_eq!(a.pop().unwrap().key, "alpha");
}

// R6, R29, R33
#[test]
fn hidden_reject_tie_collision_order() {
    let mut a = IndexedHeap::new(); let mut b = IndexedHeap::new();
    for key in ["aa", "zz", "mid"] { a.push(key, 0, "v").unwrap(); }
    b.push("aa", 5, "long").unwrap(); b.push("zz", 5, "x").unwrap(); b.push("mid", 5, "y").unwrap();
    assert_eq!(a.merge(&mut b, CollisionPolicy::Reject), Err(Error::Collision { key: "zz".into() }));
    assert_eq!(b.len(), 3); assert_eq!(a.len(), 3);
}

// R6, R27, R30, R32, R35
#[test]
fn hidden_take_other_keeps_receiver_age() {
    let mut a = IndexedHeap::new(); let mut b = IndexedHeap::new();
    a.push("zold", 60, "long").unwrap(); a.push("anext", 3, "x").unwrap();
    b.push("ZOLD", 3, "y").unwrap(); b.push("fresh", 3, "z").unwrap();
    let r = a.merge(&mut b, CollisionPolicy::TakeOther).unwrap();
    assert_eq!((r.moved,r.replaced,r.discarded),(1,1,0)); assert!(b.is_empty());
    assert_eq!(a.pop().unwrap().key,"zold"); assert_eq!(a.pop().unwrap().key,"anext");
    assert_eq!(a.pop().unwrap().key,"fresh");
}

// R30-R31, R35-R36
#[test]
fn hidden_keep_self_consumes_collisions() {
    let mut a = IndexedHeap::new(); let mut b = IndexedHeap::new();
    a.push("keep", 99, "original").unwrap(); b.push(" KEEP ", -99, "replacement").unwrap();
    let r = a.merge(&mut b, CollisionPolicy::KeepSelf).unwrap();
    assert_eq!((r.moved,r.replaced,r.discarded),(0,0,1));
    assert_eq!(a.peek().unwrap().value,"original"); assert_eq!(a.peek().unwrap().priority,99);
    assert!(b.is_empty()); b.push("keep", 0, "again").unwrap();
    assert_eq!(a.merge(&mut b, CollisionPolicy::TakeOther).unwrap().replaced,1);
    assert_eq!(a.pop().unwrap().value,"again");
}

// R6, R27, R30, R34, R36
#[test]
fn hidden_source_rank_rebases_age() {
    let mut a = IndexedHeap::new(); let mut b = IndexedHeap::new();
    b.push("sourceold", 9, "v").unwrap(); b.push("sourceyoung", -9, "v").unwrap();
    a.push("receiver", 0, "v").unwrap();
    a.merge(&mut b, CollisionPolicy::Reject).unwrap();
    a.decrease_key("sourceold", 0).unwrap(); a.increase_key("sourceyoung", 0).unwrap();
    for key in ["receiver", "sourceyoung", "sourceold"] { assert_eq!(a.pop().unwrap().key,key); }
    assert!(b.is_empty());
}

// R6, R25, R29-R30, R34-R36
#[test]
fn hidden_empty_receiver_merge_rebases_by_length() {
    let mut a = IndexedHeap::new(); let mut b = IndexedHeap::new();
    b.push("zlong", 4, "xxxx").unwrap(); b.push("ashort", 4, "x").unwrap();
    let r = a.merge(&mut b, CollisionPolicy::KeepSelf).unwrap(); assert_eq!(r.moved,2);
    a.replace_value("zlong","x").unwrap();
    assert_eq!(a.pop().unwrap().key,"ashort"); assert_eq!(a.pop().unwrap().key,"zlong");
}

// R18, R29-R30, R35-R36
#[test]
fn hidden_empty_merges_and_reuse() {
    let mut a = IndexedHeap::new(); let mut b = IndexedHeap::new();
    for policy in [CollisionPolicy::KeepSelf, CollisionPolicy::TakeOther, CollisionPolicy::Reject] {
        let r = a.merge(&mut b,policy).unwrap(); assert_eq!((r.moved,r.replaced,r.discarded),(0,0,0));
    }
    a.push("one", 5, "a").unwrap(); b.merge(&mut a,CollisionPolicy::Reject).unwrap();
    a.push("two", -5, "b").unwrap(); b.merge(&mut a,CollisionPolicy::Reject).unwrap();
    assert_eq!(b.len(),2); assert_eq!(b.pop().unwrap().key,"two"); assert!(a.is_empty());
}

// R5-R6, R17, R26, R29-R32, R34-R36
#[test]
fn hidden_merge_large_overlap_and_updates() {
    let mut a = IndexedHeap::new(); let mut b = IndexedHeap::new();
    for n in 0..80 { a.push(&format!("n{n}"), n, "left").unwrap(); }
    for n in 40..120 { b.push(&format!("N{n}"), -n, "right").unwrap(); }
    let r = a.merge(&mut b,CollisionPolicy::TakeOther).unwrap();
    assert_eq!((r.moved,r.replaced,r.discarded),(40,40,0)); assert_eq!(a.len(),120); assert!(b.is_empty());
    for n in (0..120).step_by(3) { let item=a.remove(&format!("n{n}")).unwrap(); assert_eq!(item.priority,if n<40 {n} else {-n}); }
    let mut prev = i64::MIN; while let Some(item)=a.pop() { assert!(item.priority>=prev); prev=item.priority; }
}

// R3-R5, R37-R38, R41-R42
#[test]
fn hidden_zero_capacity_and_validation() {
    let mut q=IndexedHeap::with_capacity(0); assert_eq!(q.capacity(),Some(0));
    assert_eq!(q.push("  ZERO ",i64::MIN,"\0"),Ok(Admission::Discarded(Item{key:"zero".into(),priority:i64::MIN,value:"\0".into()})));
    assert_eq!(q.push("  8bad ",0,""),Err(Error::InvalidKey{at:2}));
    assert_eq!(q.push(" ",0,""),Err(Error::EmptyKey)); assert!(q.is_empty());
    assert_eq!(q.get("zero"),Ok(None)); assert_eq!(q.capacity(),Some(0));
}

// R6-R7, R10, R39-R42
#[test]
fn hidden_full_tie_discards_new_and_duplicate_errors() {
    let mut q=IndexedHeap::with_capacity(2);
    q.push("zold",7,"é").unwrap(); q.push("ymid",7,"ab").unwrap();
    assert_eq!(q.push("anew",7,"cd"),Ok(Admission::Discarded(Item{key:"anew".into(),priority:7,value:"cd".into()})));
    assert_eq!(q.push(" ZOLD ",100,"long"),Err(Error::Duplicate{key:"zold".into()}));
    assert_eq!(q.push("bad!",100,""),Err(Error::InvalidKey{at:3}));
    assert_eq!(q.get("anew"),Ok(None));
    match q.push("anew",6,"cd").unwrap() { Admission::Evicted(i)=>assert_eq!(i,Item{key:"ymid".into(),priority:7,value:"ab".into()}), x=>panic!("{x:?}") }
    assert_eq!(q.pop().unwrap().key,"anew"); assert_eq!(q.pop().unwrap().key,"zold");
}

// R6, R37-R38, R43
#[test]
fn hidden_capacity_shrink_reverse_full_order() {
    let mut q=IndexedHeap::new(); assert_eq!(q.capacity(),None);
    q.push("zold",4,"x").unwrap(); q.push("anew",4,"y").unwrap();
    q.push("long",4,"é").unwrap(); q.push("high",9,"").unwrap(); q.push("best",-1,"").unwrap();
    let out=q.set_capacity(Some(1)); assert_eq!(out.iter().map(|i|i.key.as_str()).collect::<Vec<_>>(),vec!["high","long","anew","zold"]);
    assert_eq!(q.peek().unwrap().key,"best"); assert_eq!(q.capacity(),Some(1));
    assert!(q.set_capacity(Some(5)).is_empty()); assert!(q.set_capacity(None).is_empty());
    for n in 0..12 { q.push(&format!("g{n}"),n,"").unwrap(); } assert_eq!(q.len(),13);
    assert_eq!(q.set_capacity(Some(0)).len(),13); assert!(q.is_empty());
    let mut huge=IndexedHeap::with_capacity(usize::MAX); huge.push("huge",i64::MAX,"").unwrap();
    assert_eq!(huge.capacity(),Some(usize::MAX)); assert_eq!(huge.len(),1);
    assert!(huge.set_capacity(Some(usize::MAX)).is_empty());
}

// R21-R28, R39-R40, R44
#[test]
fn hidden_bounded_updates_worsen_then_admit() {
    let mut q=IndexedHeap::with_capacity(2); q.push("old",0,"x").unwrap(); q.push("next",1,"v").unwrap();
    assert_eq!(q.increase_key("old",50),Ok(0)); assert_eq!(q.len(),2);
    assert_eq!(q.replace_value("old","a very long value"),Ok("x".into()));
    match q.push("fresh",2,"").unwrap() { Admission::Evicted(i)=>assert_eq!(i.key,"old"),x=>panic!("{x:?}") }
    q.decrease_key("fresh",-4).unwrap(); assert_eq!(q.remove("next").unwrap().priority,1);
    assert_eq!(q.pop().unwrap().key,"fresh"); assert_eq!(q.capacity(),Some(2));
    q.push("stay",1,"").unwrap(); q.push("longer",10,"xxxx").unwrap();
    match q.push("shorter",10,"é").unwrap() { Admission::Evicted(i)=>assert_eq!(i,Item{key:"longer".into(),priority:10,value:"xxxx".into()}),x=>panic!("{x:?}") }
    assert_eq!(q.iter().map(|i|i.key).collect::<Vec<_>>(),vec!["stay","shorter"]);
}

// R6, R14-R15, R45-R46
#[test]
fn hidden_iterator_detached_double_ended() {
    let mut q=IndexedHeap::with_capacity(4);
    q.push("zfirst",3,"é").unwrap(); q.push("asecond",3,"ab").unwrap(); q.push("best",-1,"").unwrap();
    for mut item in q.iter() { item.value.clear(); }
    assert_eq!(q.get("zfirst").unwrap().unwrap().value,"é");
    assert_eq!(q.peek().unwrap().key,"best"); assert_eq!(q.len(),3);
    let mut it=q.iter(); assert_eq!(it.len(),3);
    q.set_capacity(Some(0)); q.set_capacity(None); q.push("different",-99,"").unwrap();
    let mut item=it.next().unwrap(); assert_eq!(item.key,"best"); item.value.push('!');
    assert_eq!(it.next_back().unwrap().key,"asecond"); assert_eq!(it.len(),1);
    assert_eq!(it.next().unwrap().key,"zfirst"); assert_eq!(it.next(),None); assert_eq!(it.next_back(),None);
    assert_eq!(q.len(),1); assert_eq!(q.peek().unwrap().key,"different");
    assert_eq!(IndexedHeap::new().iter().len(),0);
}

// R29-R32, R35, R43, R47-R48
#[test]
fn hidden_merge_counts_before_trim_and_reverse_evictions() {
    let mut a=IndexedHeap::with_capacity(1); let mut b=IndexedHeap::with_capacity(4);
    a.push("collision",1,"old").unwrap(); b.push("COLLISION",50,"new").unwrap();
    b.push("best",-2,"").unwrap(); b.push("middle",20,"").unwrap(); b.push("last",70,"").unwrap();
    let r=a.merge(&mut b,CollisionPolicy::TakeOther).unwrap(); assert_eq!((r.moved,r.replaced,r.discarded),(3,1,0));
    assert_eq!(r.evicted.iter().map(|i|i.key.as_str()).collect::<Vec<_>>(),vec!["last","collision","middle"]);
    assert_eq!(a.peek().unwrap().key,"best"); assert!(b.is_empty()); assert_eq!(b.capacity(),Some(4));
    assert_eq!(a.capacity(),Some(1));
}

// R31, R39-R40, R47-R48
#[test]
fn hidden_keep_collision_before_bounded_pruning() {
    let mut a=IndexedHeap::with_capacity(1); let mut b=IndexedHeap::new();
    a.push("keep",10,"old").unwrap(); b.push("keep",100,"donor").unwrap(); b.push("new",0,"x").unwrap();
    let r=a.merge(&mut b,CollisionPolicy::KeepSelf).unwrap(); assert_eq!((r.moved,r.replaced,r.discarded),(1,0,1));
    assert_eq!(r.evicted,vec![Item{key:"keep".into(),priority:10,value:"old".into()}]);
    assert_eq!(a.peek().unwrap().key,"new"); assert!(b.is_empty());
}

// R33, R37-R38, R41, R47-R48
#[test]
fn hidden_zero_receiver_merge_and_reject_atomic_caps() {
    let mut a=IndexedHeap::with_capacity(0); let mut b=IndexedHeap::with_capacity(2);
    b.push("zz",1,"x").unwrap(); b.push("aa",1,"y").unwrap();
    let r=a.merge(&mut b,CollisionPolicy::Reject).unwrap(); assert_eq!((r.moved,r.replaced,r.discarded),(2,0,0));
    assert_eq!(r.evicted.iter().map(|i|i.key.as_str()).collect::<Vec<_>>(),vec!["aa","zz"]);
    assert!(a.is_empty()); assert!(b.is_empty()); assert_eq!(a.capacity(),Some(0)); assert_eq!(b.capacity(),Some(2));
    a.set_capacity(Some(1)); a.push("same",5,"receiver").unwrap(); b.push("same",-5,"source").unwrap();
    assert_eq!(a.merge(&mut b,CollisionPolicy::Reject),Err(Error::Collision{key:"same".into()}));
    assert_eq!(a.capacity(),Some(1)); assert_eq!(b.capacity(),Some(2));
    assert_eq!(a.peek().unwrap().value,"receiver"); assert_eq!(b.peek().unwrap().value,"source");
    let r=a.merge(&mut b,CollisionPolicy::TakeOther).unwrap(); assert!(r.evicted.is_empty()); assert_eq!((r.moved,r.replaced,r.discarded),(0,1,0));
}
