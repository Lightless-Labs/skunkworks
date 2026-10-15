fn hid_product(id: &str, price: u16) -> Product { Product { id: id.into(), price } }
fn hid_new(price: u16, tubes: [u16;4]) -> Machine {
    Machine::new(vec![hid_product("olive",price), hid_product("mango",7)], tubes).unwrap()
}
// Checks R1-R5.
#[test]
fn hidden_empty_catalog() {
    assert!(matches!(Machine::new(vec![], [101,101,101,101]),Err(Error::EmptyCatalog)));
}

// Checks R6, R7.
#[test]
fn hidden_id_bytes() {
    for id in ["", "Tea", "a1", "a b", "é", "abcdefghi", "tea\n"] {
        assert!(matches!(Machine::new(vec![hid_product(id,0)], [101,101,101,101]),Err(Error::BadId {id: bad}) if bad==id));
    }
    let good=Machine::new(vec![hid_product("abcdefgh",1)], [0;4]); assert!(good.is_ok());
}

// Checks R7, R8.
#[test]
fn hidden_duplicate_precedes_fields() {
    let mut duplicate=hid_product("kiwi",0);
    duplicate.price=0;
    assert!(matches!(Machine::new(vec![hid_product("kiwi",5),duplicate], [101,101,101,101]),Err(Error::DuplicateId {id}) if id=="kiwi"));
}

// Checks R7, R8.
#[test]
fn hidden_constructor_first_error() {
    assert!(matches!(Machine::new(vec![hid_product("plum",101),hid_product("BAD",1)], [101,101,101,101]),Err(Error::BadPrice {id}) if id=="plum"));
    assert!(matches!(Machine::new(vec![hid_product("BAD",0),hid_product("plum",0)], [101,101,101,101]),Err(Error::BadId {id}) if id=="BAD"));
}

// Checks R8.
#[test]
fn hidden_price_limits() {
    for price in [0,101,65535] {
        assert!(matches!(Machine::new(vec![hid_product("pear",price)], [0;4]),Err(Error::BadPrice {id}) if id=="pear"));
    }
    for price in [1,100] { let m=hid_new(price, [20,10,10,10]); assert_eq!(m.state().credit,0); }
}

// Checks R4, R9, R21.
#[test]
fn hidden_initial_owned_snapshot() {
    let m=hid_new(6, [20,10,10,10]); let mut st=m.state();
    assert_eq!(st.selected,None); assert!(st.escrow.is_empty());
    st.credit=77; st.escrow.push(10); st.selected=Some("fake".into());
    st.tubes=[99;4];
    assert_eq!(m.state().credit,0); assert!(m.state().escrow.is_empty());
    assert_eq!(m.state().tubes,[20,10,10,10]);
}

// Checks R3, R10, R11, R19.
#[test]
fn hidden_coin_error_priority() {
    let mut m=hid_new(6, [20,10,10,10]); for _ in 0..10 {m.insert(10).unwrap();}
    let st=m.state();
    for coin in [0,2,5,9,11,65535] {assert_eq!(m.insert(coin),Err(Error::BadCoin {coin}));}
    assert_eq!(m.insert(1),Err(Error::CreditLimit)); assert_eq!(m.state(),st);
}

// Checks R3, R10, R11, R16.
#[test]
fn hidden_inclusive_credit_limit() {
    let mut m=hid_new(100, [20,10,10,10]); for _ in 0..9 {m.insert(10).unwrap();}
    for _ in 0..3 {m.insert(3).unwrap();} m.insert(1).unwrap();
    assert_eq!(m.state().credit,100);
    
    m.select("olive").unwrap(); assert_eq!(m.vend().unwrap().price,100);
    assert_eq!(m.state().credit,0);
}

// Checks R6, R12, R20.
#[test]
fn hidden_lookup_not_syntax() {
    let mut m=hid_new(6, [20,10,10,10]); m.select("mango").unwrap();
    for id in ["Tea", " tea", "", "teas", "juice "] {assert_eq!(m.select(id),Err(Error::UnknownProduct {id:id.into()}));}
    assert_eq!(m.state().selected,Some("mango".into()));
}

// Checks R12, R13.
#[test]
fn hidden_replace_and_repeat() {
    let mut m=hid_new(6, [20,10,10,10]); m.select("mango").unwrap(); m.insert(3).unwrap();
    m.select("olive").unwrap(); let st=m.state();
    assert_eq!(m.select("olive"),Err(Error::SameSelection)); assert_eq!(m.state(),st);
    assert_eq!(m.state().credit,3); m.refund().unwrap(); m.select("olive").unwrap();
    assert_eq!(m.select("olive"),Err(Error::SameSelection));
}

// Checks R14, R19.
#[test]
fn hidden_missing_selection_priority() {
    let mut m=hid_new(99, [0;4]);
    assert_eq!(m.vend(),Err(Error::NoSelection)); m.insert(10).unwrap(); let st=m.state();
    assert_eq!(m.vend(),Err(Error::NoSelection)); assert_eq!(m.state(),st);
}

// Checks R14, R15, R19.
#[test]
fn hidden_underpaid_retains() {
    let mut m=hid_new(9, [0;4]); m.select("olive").unwrap();
    assert_eq!(m.vend(),Err(Error::Underpaid {price:9,credit:0}));
    m.insert(4).unwrap(); m.insert(4).unwrap(); let st=m.state();
    assert_eq!(m.vend(),Err(Error::Underpaid {price:9,credit:8})); assert_eq!(m.state(),st);
    m.insert(1).unwrap(); assert_eq!(m.vend().unwrap().price,9);
}

// Checks R9, R16, R21.
#[test]
fn hidden_successful_reset() {
    let mut m=hid_new(8, [20,10,10,10]); m.insert(4).unwrap(); m.select("olive").unwrap(); m.insert(4).unwrap();
    let r=m.vend().unwrap(); assert_eq!(r,Receipt {id:"olive".into(),price:8,change:vec![]});
    assert_eq!(m.state().selected,None); assert_eq!(m.state().escrow,vec![]);
    m.insert(10).unwrap(); assert_eq!(r.price,8); assert!(r.change.is_empty());
}

// Checks R17, R23-R27.
#[test]
fn hidden_overpayment() {
    let mut m=hid_new(2, [8,0,0,0]); m.select("olive").unwrap(); m.insert(10).unwrap();
    assert_eq!(m.vend().unwrap().change,vec![1;8]); assert_eq!(m.state().tubes,[0,0,0,1]);
}

// Checks R9, R18.
#[test]
fn hidden_reverse_escrow() {
    let mut m=hid_new(6, [20,10,10,10]); for coin in [3,10,1,4,3] {m.insert(coin).unwrap();}
    m.select("olive").unwrap(); assert_eq!(m.state().escrow,vec![3,10,1,4,3]);
    assert_eq!(m.refund().unwrap(),vec![3,4,1,10,3]); assert_eq!(m.state().credit,0);
}

// Checks R18, R21.
#[test]
fn hidden_empty_refund_clears() {
    let mut m=hid_new(6, [20,10,10,10]); m.select("olive").unwrap();
    assert_eq!(m.refund().unwrap(),Vec::<u16>::new()); assert_eq!(m.state().selected,None);
    assert_eq!(m.refund().unwrap(),Vec::<u16>::new());
}

// Checks R10-R15, R19.
#[test]
fn hidden_error_atomicity() {
    let mut m=hid_new(20, [20,10,10,10]); m.select("olive").unwrap(); m.insert(3).unwrap(); let st=m.state();
    assert!(m.insert(6).is_err()); assert!(m.select("absent").is_err());
    assert!(m.vend().is_err()); assert!(m.select("olive").is_err()); assert_eq!(m.state(),st);
}

// Checks R16, R20.
#[test]
fn hidden_lookup_and_catalog_persist() {
    let mut m=hid_new(7, [20,10,10,10]); m.select("mango").unwrap(); m.insert(3).unwrap(); m.insert(4).unwrap();
    assert_eq!(m.vend().unwrap().id,"mango");
    m.select("olive").unwrap(); m.insert(3).unwrap(); m.insert(4).unwrap();
    assert_eq!(m.vend().unwrap().price,7);
}

// Checks R16, R18, R21.
#[test]
fn hidden_refund_then_new_transaction() {
    let mut m=hid_new(4, [20,10,10,10]); m.insert(10).unwrap(); m.select("mango").unwrap();
    let returned=m.refund().unwrap(); m.insert(4).unwrap(); m.select("olive").unwrap();
    assert_eq!(m.vend().unwrap().change,vec![]); assert_eq!(returned,vec![10]);
}

// Checks R3, R9-R11, R21.
#[test]
fn hidden_all_coins_and_snapshot_history() {
    let mut m=hid_new(6, [20,10,10,10]); for coin in [1,3,4,10,4,3,1] {m.insert(coin).unwrap();}
    let old=m.state(); assert_eq!(old.credit,26); m.refund().unwrap();
    assert_eq!(old.escrow,vec![1,3,4,10,4,3,1]); assert_eq!(m.state().credit,0);
}

// Checks R4, R19-R21.
#[test]
fn hidden_independent_machines() {
    let products=vec![hid_product("fig",3)];
    let mut a=Machine::new(products.clone(), [0;4]).unwrap();
    let b=Machine::new(products, [0;4]).unwrap();
    a.insert(3).unwrap(); a.select("fig").unwrap(); a.vend().unwrap();
    assert_eq!(b.state().credit,0); assert_eq!(b.state().selected,None);
    
}

// Checks R7, R22.
#[test]
fn hidden_tube_constructor_precedence() {
    assert!(matches!(Machine::new(vec![hid_product("leaf",0)],[101;4]),Err(Error::BadPrice {id}) if id=="leaf"));
    for (ts,coin) in [([101,102,103,104],1),([100,101,103,104],3),([100,100,101,104],4),([100,100,100,101],10)] {
        assert!(matches!(Machine::new(vec![hid_product("leaf",1)],ts),Err(Error::TubeLimit {coin:c}) if c==coin));
    }
    assert!(Machine::new(vec![hid_product("leaf",1)],[100;4]).is_ok());
}

// Checks R23, R25, R27.
#[test]
fn hidden_mandatory_greedy() {
    let mut m=hid_new(4, [2,2,1,0]); m.insert(10).unwrap(); m.select("olive").unwrap();
    assert_eq!(m.vend().unwrap().change,vec![4,1,1]); // 3+3 is shorter but forbidden.
    assert_eq!(m.state().tubes,[0,2,0,1]);
    let mut n=hid_new(1, [1,2,2,2]); n.insert(10).unwrap(); n.insert(10).unwrap(); n.select("olive").unwrap();
    assert_eq!(n.vend().unwrap().change,vec![10,4,4,1]);
}

// Checks R24, R25.
#[test]
fn hidden_fallback_minimum_and_ties() {
    let mut m=hid_new(4, [0,2,1,0]); m.insert(10).unwrap(); m.select("olive").unwrap();
    assert_eq!(m.vend().unwrap().change,vec![3,3]); assert_eq!(m.state().tubes,[0,0,1,1]);
    let mut n=hid_new(8, [1,4,2,1]); n.insert(10).unwrap(); n.insert(10).unwrap(); n.select("olive").unwrap();
    // Greedy 10+1 fails; tied four-coin solutions are 4+4+3+1 and 3+3+3+3.
    assert_eq!(n.vend().unwrap().change,vec![4,4,3,1]);
    assert_eq!(n.state().tubes,[0,3,0,3]);
    let mut minimum=hid_new(8, [0,4,3,1]); minimum.insert(10).unwrap(); minimum.insert(10).unwrap(); minimum.select("olive").unwrap();
    // Greedy 10 leaves 2; fallback 4+4+4 beats 3+3+3+3.
    assert_eq!(minimum.vend().unwrap().change,vec![4,4,4]);
    assert_eq!(minimum.state().tubes,[0,4,0,3]);
    let mut z=hid_new(1, [0,0,0,0]); z.insert(1).unwrap(); z.select("olive").unwrap();
    assert!(z.vend().unwrap().change.is_empty());
}

// Checks R17, R24, R26-R28.
#[test]
fn hidden_escrow_excluded_and_nochange_atomic() {
    let mut m=hid_new(3, [0,0,0,100]); m.insert(4).unwrap(); m.insert(3).unwrap(); m.select("olive").unwrap();
    let st=m.state(); assert_eq!(m.vend(),Err(Error::NoChange {amount:4})); assert_eq!(m.state(),st);
    m.select("mango").unwrap(); assert!(m.vend().is_ok()); assert_eq!(m.state().tubes,[0,1,1,100]);
    let mut n=hid_new(9, [0,0,0,100]); n.insert(10).unwrap(); n.select("olive").unwrap();
    assert_eq!(n.vend(),Err(Error::NoChange {amount:1})); // Before the eventual ten-tube overflow.
    assert_eq!(n.refund().unwrap(),vec![10]); assert_eq!(n.state().tubes,[0,0,0,100]);
}

// Checks R22, R26-R28.
#[test]
fn hidden_capacity_order_and_post_change() {
    let mut m=hid_new(4, [100;4]); m.insert(1).unwrap(); m.insert(3).unwrap(); m.select("olive").unwrap();
    let st=m.state(); assert_eq!(m.vend(),Err(Error::TubeLimit {coin:1})); assert_eq!(m.state(),st);
    assert_eq!(m.refund().unwrap(),vec![3,1]);
    let mut n=hid_new(3, [0,0,100,0]); n.insert(4).unwrap(); n.insert(3).unwrap(); n.select("olive").unwrap();
    assert_eq!(n.vend().unwrap().change,vec![4]); assert_eq!(n.state().tubes,[0,1,100,0]);
    let mut q=hid_new(4, [100,100,100,0]); q.insert(3).unwrap(); q.insert(3).unwrap(); q.insert(4).unwrap(); q.select("olive").unwrap();
    let st=q.state(); assert_eq!(q.vend(),Err(Error::TubeLimit {coin:3})); assert_eq!(q.state(),st);
    // Greedy 4+1+1 overflows threes; alternative 3+3 would fit but must not be chosen.

}
