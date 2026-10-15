fn pub_product(id: &str, price: u16) -> Product { Product { id: id.into(), price } }
fn pub_new(price: u16, tubes: [u16;4]) -> Machine {
    Machine::new(vec![pub_product("tea",price), pub_product("juice",7)], tubes).unwrap()
}
// Checks R9, R10, R12, R14-R16.
#[test]
fn public_exact_sale() {
    let mut m = pub_new(6, [20,10,10,10]);
    m.select("tea").unwrap();
    m.insert(3).unwrap(); m.insert(3).unwrap();
    assert_eq!(m.vend().unwrap(), Receipt { id: "tea".into(), price: 6, change: vec![] });
    assert_eq!(m.state().credit,0); assert_eq!(m.state().selected,None);
}
// Checks R12, R18.
#[test]
fn public_refund_and_selection() {
    let mut m = pub_new(6, [20,10,10,10]);
    m.select("juice").unwrap(); m.insert(1).unwrap(); m.insert(4).unwrap();
    assert_eq!(m.refund().unwrap(),vec![4,1]);
    assert_eq!(m.state().escrow,Vec::<u16>::new());
}
// Checks R17, R23, R25, R27.
#[test]
fn public_change() {
    let mut m = pub_new(6, [20,10,10,10]);
    m.insert(10).unwrap(); m.select("tea").unwrap();
    assert_eq!(m.vend().unwrap().change,vec![4]);
    assert_eq!(m.state().tubes,[20,10,9,11]);
}
