fn pub_product(id: &str, price: u16) -> Product { Product { id: id.into(), price, stock: 5 } }
fn pub_new(price: u16, tubes: [u16;4]) -> Machine {
    Machine::new(vec![pub_product("tea",price), pub_product("juice",7)], tubes).unwrap()
}
fn pub_admin(m: &mut Machine) { m.set_maintenance(true).unwrap(); }
fn pub_normal(m: &mut Machine) { m.set_maintenance(false).unwrap(); }
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
// Checks R29-R36, R39-R47.
#[test]
fn public_catalog_log_and_service() {
    let mut m = pub_new(6, [20,10,10,10]);
    pub_admin(&mut m);
    m.set_price("tea",8).unwrap(); m.restock("tea",2).unwrap();
    let products = m.products();
    assert_eq!(products.iter().map(|p|p.id.as_str()).collect::<Vec<_>>(),vec!["juice","tea"]);
    assert_eq!(products[1].price,8); assert_eq!(products[1].stock,7);
    assert!(m.events().iter().any(|e| e.kind == EventKind::PriceChanged { id:"tea".into(),old:6,new:8 }));
    assert_eq!(m.audit().cash,20+30+40+100);
    m.reconcile([20,10,10,10],[21,10,10,10]).unwrap();
    assert_eq!(m.audit().adjustment,1);
    pub_normal(&mut m); assert!(!m.state().maintenance);

}
