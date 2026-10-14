#[test]
fn public_scheduler_priority() {
let mut s=Scheduler::new(10,0).unwrap();
s.enqueue(Job { id: 1, priority: 2, ready_at: 0 },0).unwrap();
s.enqueue(Job { id: 2, priority: 5, ready_at: 0 },0).unwrap();
assert_eq!(s.next_ready(0).unwrap().unwrap().job.id,2);
assert_eq!(s.next_ready(0).unwrap().unwrap().job.id,1);
assert_eq!(s.next_ready(0),Ok(None));
}

#[test]
fn public_scheduler_fifo_future() {
let mut s=Scheduler::new(100,0).unwrap();
for id in [10,20] { s.enqueue(Job { id, priority: 3, ready_at: 5 },0).unwrap(); }
assert_eq!(s.next_ready(3),Ok(None));
assert_eq!(s.len(),2);
assert_eq!(s.next_ready(6).unwrap().unwrap().job.id,10);
assert_eq!(s.next_ready(6).unwrap().unwrap().job.id,20);
}

#[test]
fn public_scheduler_aging() {
let mut s=Scheduler::new(10,0).unwrap();
s.enqueue(Job { id: 10, priority: 1, ready_at: 0 },0).unwrap();
s.enqueue(Job { id: 20, priority: 2, ready_at: 10 },10).unwrap();
let d=s.next_ready(30).unwrap().unwrap();
assert_eq!(d,Dispatch { job: Job { id: 10, priority: 1, ready_at: 0 }, enqueued_at: 0, started_at: 30, effective_priority: 4 });
}

#[test]
fn public_scheduler_cancel_config() {
assert!(matches!(Scheduler::new(0,0),Err(SchedulerError::ZeroAgingInterval)));
let mut s=Scheduler::new(10,0).unwrap();
let j=Job { id: 8, priority: 7, ready_at: 100 };
s.enqueue(j.clone(),0).unwrap();
assert_eq!(s.cancel(8,1),Ok(Some(j)));
assert_eq!(s.cancel(8,1),Ok(None));
assert_eq!(s.len(),0);
}
