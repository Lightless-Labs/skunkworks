#[test]
fn public_window_weight() {
let mut w=SlidingWindow::new(5,10,50,0).unwrap();
assert_eq!(w.try_acquire("alpha",3,0),Ok(true));
assert_eq!(w.remaining("alpha",1),Ok(2));
assert_eq!(w.try_acquire("alpha",3,1),Ok(false));
assert_eq!(w.try_acquire("alpha",5,11),Ok(true));
}

#[test]
fn public_window_keys() {
let mut w=SlidingWindow::new(2,20,50,0).unwrap();
assert_eq!(w.try_acquire("a",2,0),Ok(true));
assert_eq!(w.try_acquire("b",2,0),Ok(true));
assert_eq!(w.remaining("a",0),Ok(0));
assert_eq!(w.remaining("c",0),Ok(2));
assert_eq!(w.tracked_keys(),2);
}

#[test]
fn public_window_evict() {
let mut w=SlidingWindow::new(4,10,20,0).unwrap();
w.try_acquire("old",1,0).unwrap();
w.try_acquire("new",1,15).unwrap();
assert_eq!(w.evict_idle(21),Ok(vec![String::from("old")]));
assert_eq!(w.tracked_keys(),1);
}

#[test]
fn public_window_config() {
assert!(matches!(SlidingWindow::new(1,0,1,0),Err(WindowError::ZeroWindow)));
assert!(matches!(SlidingWindow::new(1,1,0,0),Err(WindowError::ZeroIdleTtl)));
}
