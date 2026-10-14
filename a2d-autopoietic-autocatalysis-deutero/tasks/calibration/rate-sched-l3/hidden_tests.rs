#[test]
fn hidden_window_boundary() {
let mut w=SlidingWindow::new(9,12,100,0).unwrap();
w.try_acquire("edge",9,2).unwrap();
assert_eq!(w.remaining("edge",13),Ok(0));
assert_eq!(w.try_acquire("edge",9,14),Ok(true));
}

#[test]
fn hidden_window_partial_expiry() {
let mut w=SlidingWindow::new(10,10,100,0).unwrap();
w.try_acquire("x",3,0).unwrap();
w.try_acquire("x",4,4).unwrap();
w.try_acquire("x",3,9).unwrap();
assert_eq!(w.remaining("x",10),Ok(3));
assert_eq!(w.remaining("x",14),Ok(7));
assert_eq!(w.remaining("x",19),Ok(10));
}

#[test]
fn hidden_window_denial_touch() {
let mut w=SlidingWindow::new(1,100,10,0).unwrap();
w.try_acquire("busy",1,0).unwrap();
assert_eq!(w.try_acquire("busy",2,9),Ok(false));
assert_eq!(w.evict_idle(10),Ok(vec![]));
assert_eq!(w.evict_idle(19),Ok(vec![String::from("busy")]));
}

#[test]
fn hidden_window_denial_creates() {
let mut w=SlidingWindow::new(3,30,5,0).unwrap();
assert_eq!(w.try_acquire("denied",u64::MAX,2),Ok(false));
assert_eq!(w.tracked_keys(),1);
assert_eq!(w.remaining("denied",3),Ok(3));
assert_eq!(w.evict_idle(7),Ok(vec![String::from("denied")]));
}

#[test]
fn hidden_window_zero_missing() {
let mut w=SlidingWindow::new(4,7,9,0).unwrap();
assert_eq!(w.try_acquire("ghost",0,6),Ok(true));
assert_eq!(w.tracked_keys(),0);
assert_eq!(w.remaining("ghost",5),Err(WindowError::TimeWentBackwards));
}

#[test]
fn hidden_window_zero_no_touch() {
let mut w=SlidingWindow::new(4,5,10,0).unwrap();
w.try_acquire("x",4,0).unwrap();
assert_eq!(w.try_acquire("x",0,8),Ok(true));
assert_eq!(w.remaining("x",8),Ok(4));
assert_eq!(w.evict_idle(10),Ok(vec![String::from("x")]));
}

#[test]
fn hidden_window_query_no_touch() {
let mut w=SlidingWindow::new(5,100,8,0).unwrap();
w.try_acquire("x",1,0).unwrap();
assert_eq!(w.remaining("x",7),Ok(4));
assert_eq!(w.remaining("absent",7),Ok(5));
assert_eq!(w.tracked_keys(),1);
assert_eq!(w.evict_idle(8),Ok(vec![String::from("x")]));
}

#[test]
fn hidden_window_sorted_eviction() {
let mut w=SlidingWindow::new(8,100,3,0).unwrap();
for k in ["z","é","A","a",""] { w.try_acquire(k,1,0).unwrap(); }
assert_eq!(w.evict_idle(3),Ok(vec!["","A","a","z","é"].into_iter().map(String::from).collect()));
assert_eq!(w.tracked_keys(),0);
}

#[test]
fn hidden_window_expiry_retains_record() {
let mut w=SlidingWindow::new(3,2,100,0).unwrap();
w.try_acquire("retained",3,0).unwrap();
assert_eq!(w.remaining("retained",50),Ok(3));
assert_eq!(w.tracked_keys(),1);
assert_eq!(w.evict_idle(99),Ok(vec![]));
}

#[test]
fn hidden_window_explicit_eviction_only() {
let mut w=SlidingWindow::new(3,50,2,0).unwrap();
w.try_acquire("live",3,0).unwrap();
assert_eq!(w.try_acquire("live",1,10),Ok(false));
assert_eq!(w.remaining("live",11),Ok(0));
assert_eq!(w.tracked_keys(),1);
}

#[test]
fn hidden_window_evict_live_then_reset() {
let mut w=SlidingWindow::new(6,100,4,0).unwrap();
w.try_acquire("reset",6,0).unwrap();
assert_eq!(w.evict_idle(4),Ok(vec![String::from("reset")]));
assert_eq!(w.try_acquire("reset",6,4),Ok(true));
}

#[test]
fn hidden_window_backward_atomic() {
let mut w=SlidingWindow::new(5,10,20,50).unwrap();
w.try_acquire("a",5,50).unwrap();
assert_eq!(w.remaining("missing",60),Ok(5));
assert_eq!(w.try_acquire("new",3,59),Err(WindowError::TimeWentBackwards));
assert_eq!(w.evict_idle(58),Err(WindowError::TimeWentBackwards));
assert_eq!(w.try_acquire("a",0,59),Err(WindowError::TimeWentBackwards));
assert_eq!(w.tracked_keys(),1);
assert_eq!(w.evict_idle(70),Ok(vec![String::from("a")]));
}

#[test]
fn hidden_window_global_keys() {
let mut w=SlidingWindow::new(5,10,20,0).unwrap();
w.try_acquire("a",1,7).unwrap();
assert_eq!(w.try_acquire("b",1,6),Err(WindowError::TimeWentBackwards));
assert_eq!(w.remaining("a",6),Err(WindowError::TimeWentBackwards));
assert_eq!(w.tracked_keys(),1);
}

#[test]
fn hidden_window_same_time_weights() {
let mut w=SlidingWindow::new(12,9,100,0).unwrap();
for _ in 0..4 { assert_eq!(w.try_acquire("k",3,4),Ok(true)); }
assert_eq!(w.try_acquire("k",1,4),Ok(false));
assert_eq!(w.remaining("k",12),Ok(0));
assert_eq!(w.remaining("k",13),Ok(12));
}

#[test]
fn hidden_window_max_arithmetic() {
let m=u64::MAX;
let mut w=SlidingWindow::new(m,m,m,0).unwrap();
w.try_acquire("large",m-1,0).unwrap();
assert_eq!(w.try_acquire("large",m,1),Ok(false));
assert_eq!(w.try_acquire("large",1,1),Ok(true));
assert_eq!(w.remaining("large",m-1),Ok(0));
assert_eq!(w.remaining("large",m),Ok(m-1));
}

#[test]
fn hidden_window_near_max_time() {
let m=u64::MAX;
let mut w=SlidingWindow::new(2,3,4,m-5).unwrap();
w.try_acquire("late",2,m-4).unwrap();
assert_eq!(w.remaining("late",m-2),Ok(0));
assert_eq!(w.remaining("late",m-1),Ok(2));
assert_eq!(w.evict_idle(m),Ok(vec![String::from("late")]));
}

#[test]
fn hidden_window_zero_limit_precedence() {
assert!(matches!(SlidingWindow::new(0,0,0,0),Err(WindowError::ZeroWindow)));
let mut w=SlidingWindow::new(0,1,1,5).unwrap();
assert_eq!(w.try_acquire("a",0,5),Ok(true));
assert_eq!(w.try_acquire("a",1,5),Ok(false));
assert_eq!(w.tracked_keys(),1);
assert_eq!(w.remaining("a",5),Ok(0));
assert_eq!(w.evict_idle(6),Ok(vec![String::from("a")]));
}

#[test]
fn hidden_window_many_keys() {
let mut w=SlidingWindow::new(10,20,30,0).unwrap();
for i in 0..150 { let k=format!("key-{i:03}"); w.try_acquire(&k,(i%10+1) as u64,0).unwrap(); }
assert_eq!(w.tracked_keys(),150);
for i in 0..150 { let k=format!("key-{i:03}"); assert_eq!(w.remaining(&k,20),Ok(10)); }
let expected:Vec<String>=(0..150).map(|i|format!("key-{i:03}")).collect();
assert_eq!(w.evict_idle(30),Ok(expected));
}
