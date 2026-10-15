fn pub_product(id: &str, price: u16) -> Product { Product { id: id.into(), price } }
fn pub_new(price: u16) -> Machine {
    Machine::new(vec![pub_product("tea",price), pub_product("juice",7)]).unwrap()
}
// Checks R9, R10, R12, R14-R16.
#[test]
fn public_exact_sale() {
    let mut m = pub_new(6);
    m.select("tea").unwrap();
    m.insert(1).unwrap(); m.insert(4).unwrap(); m.insert(1).unwrap();
    assert_eq!(m.vend().unwrap(), Receipt { id: "tea".into(), price: 6, change: vec![] });
    assert_eq!(m.state().credit,0); assert_eq!(m.state().selected,None);
}
// Checks R12, R18.
#[test]
fn public_refund_and_selection() {
    let mut m = pub_new(6);
    m.select("juice").unwrap(); m.insert(1).unwrap(); m.insert(4).unwrap();
    assert_eq!(m.refund().unwrap(),vec![4,1]);
    assert_eq!(m.state().escrow,Vec::<u16>::new());
}
// Checks R8, R10, R14, R15.
#[test]
fn public_errors() {
    let mut m = pub_new(6);
    assert_eq!(m.vend(),Err(Error::NoSelection));
    assert_eq!(m.insert(2),Err(Error::BadCoin { coin:2 }));
    m.select("tea").unwrap(); m.insert(4).unwrap();
    assert_eq!(m.vend(),Err(Error::Underpaid { price:6, credit:4 }));
}
