#[test]
fn hidden_scheduler_ready_boundary() {
let mut s=Scheduler::new(20,0).unwrap();
s.enqueue(Job { id: 41, priority: 3, ready_at: 7 },0).unwrap();
assert_eq!(s.next_ready(6),Ok(None));
let d=s.next_ready(7).unwrap().unwrap();
assert_eq!(d.started_at,7);
assert_eq!(d.job.id,41);
assert_eq!(s.len(),0);
}

#[test]
fn hidden_scheduler_future_priority_ignored() {
let mut s=Scheduler::new(1,0).unwrap();
s.enqueue(Job { id: 9, priority: u32::MAX, ready_at: 100 },0).unwrap();
s.enqueue(Job { id: 8, priority: 0, ready_at: 0 },0).unwrap();
assert_eq!(s.next_ready(10).unwrap().unwrap().job.id,8);
assert_eq!(s.next_ready(11),Ok(None));
assert_eq!(s.len(),1);
}

#[test]
fn hidden_scheduler_fifo_not_id() {
let mut s=Scheduler::new(100,0).unwrap();
for id in [90,3,80,0] { s.enqueue(Job { id, priority: 4, ready_at: 0 },0).unwrap(); }
for id in [90,3,80,0] { assert_eq!(s.next_ready(0).unwrap().unwrap().job.id,id); }
}

#[test]
fn hidden_scheduler_fifo_not_ready() {
let mut s=Scheduler::new(1000,0).unwrap();
s.enqueue(Job { id: 70, priority: 5, ready_at: 20 },0).unwrap();
s.enqueue(Job { id: 2, priority: 5, ready_at: 1 },1).unwrap();
assert_eq!(s.next_ready(20).unwrap().unwrap().job.id,70);
}

#[test]
fn hidden_scheduler_aging_boundary() {
for now in [9,10] {
let mut s=Scheduler::new(10,0).unwrap();
s.enqueue(Job { id: 50, priority: 1, ready_at: 0 },0).unwrap();
s.enqueue(Job { id: 2, priority: 2, ready_at: 5 },5).unwrap();
let d=s.next_ready(now).unwrap().unwrap();
assert_eq!(d.job.id,if now==9 {2} else {50});
assert_eq!(d.effective_priority,2);
}
}

#[test]
fn hidden_scheduler_age_during_delay() {
let mut s=Scheduler::new(5,0).unwrap();
s.enqueue(Job { id: 60, priority: 0, ready_at: 20 },0).unwrap();
s.enqueue(Job { id: 1, priority: 3, ready_at: 15 },15).unwrap();
let d=s.next_ready(20).unwrap().unwrap();
assert_eq!(d.job.id,60);
assert_eq!(d.effective_priority,4);
assert_eq!(d.job.priority,0);
}

#[test]
fn hidden_scheduler_age_not_before_enqueue() {
let mut s=Scheduler::new(2,100).unwrap();
s.enqueue(Job { id: 20, priority: 1, ready_at: 0 },100).unwrap();
s.enqueue(Job { id: 30, priority: 3, ready_at: 100 },100).unwrap();
assert_eq!(s.next_ready(102).unwrap().unwrap().job.id,30);
let d=s.next_ready(102).unwrap().unwrap();
assert_eq!(d.effective_priority,2);
assert_eq!(d.enqueued_at,100);
}

#[test]
fn hidden_scheduler_saturated_fifo() {
let mut s=Scheduler::new(1,0).unwrap();
s.enqueue(Job { id: 100, priority: u32::MAX-2, ready_at: 0 },0).unwrap();
s.enqueue(Job { id: 1, priority: u32::MAX, ready_at: 0 },0).unwrap();
let d=s.next_ready(3).unwrap().unwrap();
assert_eq!(d.job.id,100);
assert_eq!(d.effective_priority,u32::MAX);
}

#[test]
fn hidden_scheduler_huge_age() {
let mut s=Scheduler::new(1,0).unwrap();
s.enqueue(Job { id: u64::MAX, priority: 0, ready_at: u64::MAX },0).unwrap();
let d=s.next_ready(u64::MAX).unwrap().unwrap();
assert_eq!(d.effective_priority,u32::MAX);
assert_eq!(d.started_at,u64::MAX);
}

#[test]
fn hidden_scheduler_duplicate_atomic() {
let mut s=Scheduler::new(10,0).unwrap();
let j=Job { id: 7, priority: 1, ready_at: 0 };
s.enqueue(j.clone(),0).unwrap();
assert_eq!(s.enqueue(Job { id: 7, priority: 99, ready_at: 100 },50),Err(SchedulerError::DuplicateId(7)));
assert_eq!(s.len(),1);
assert_eq!(s.next_ready(1),Ok(Some(Dispatch { job: j, enqueued_at: 0, started_at: 1, effective_priority: 1 })));
}

#[test]
fn hidden_scheduler_error_precedence() {
let mut s=Scheduler::new(2,10).unwrap();
s.enqueue(Job { id: 5, priority: 0, ready_at: 10 },10).unwrap();
assert_eq!(s.enqueue(Job { id: 5, priority: 1, ready_at: 9 },9),Err(SchedulerError::TimeWentBackwards));
assert_eq!(s.enqueue(Job { id: 6, priority: 1, ready_at: 9 },9),Err(SchedulerError::TimeWentBackwards));
assert_eq!(s.cancel(5,9),Err(SchedulerError::TimeWentBackwards));
assert_eq!(s.next_ready(9),Err(SchedulerError::TimeWentBackwards));
assert_eq!(s.len(),1);
assert_eq!(s.next_ready(10).unwrap().unwrap().job.id,5);
}

#[test]
fn hidden_scheduler_empty_advances_clock() {
let mut s=Scheduler::new(3,0).unwrap();
assert_eq!(s.next_ready(8),Ok(None));
assert_eq!(s.enqueue(Job { id: 1, priority: 0, ready_at: 0 },7),Err(SchedulerError::TimeWentBackwards));
assert_eq!(s.cancel(999,12),Ok(None));
assert_eq!(s.next_ready(11),Err(SchedulerError::TimeWentBackwards));
}

#[test]
fn hidden_scheduler_reuse_dispatched() {
let mut s=Scheduler::new(10,0).unwrap();
s.enqueue(Job { id: 90, priority: 2, ready_at: 0 },0).unwrap();
assert_eq!(s.next_ready(0).unwrap().unwrap().job.id,90);
s.enqueue(Job { id: 20, priority: 2, ready_at: 1 },1).unwrap();
s.enqueue(Job { id: 90, priority: 2, ready_at: 1 },1).unwrap();
assert_eq!(s.next_ready(1).unwrap().unwrap().job.id,20);
assert_eq!(s.next_ready(1).unwrap().unwrap().enqueued_at,1);
}

#[test]
fn hidden_scheduler_cancel_preserves_fifo() {
let mut s=Scheduler::new(100,0).unwrap();
for id in [9,8,7,6] { s.enqueue(Job { id, priority: 1, ready_at: 0 },0).unwrap(); }
assert_eq!(s.cancel(8,1).unwrap().unwrap().id,8);
assert_eq!(s.cancel(6,1).unwrap().unwrap().id,6);
s.enqueue(Job { id: 8, priority: 1, ready_at: 0 },1).unwrap();
for id in [9,7,8] { assert_eq!(s.next_ready(1).unwrap().unwrap().job.id,id); }
}

#[test]
fn hidden_scheduler_cancel_future_unchanged() {
let mut s=Scheduler::new(1,0).unwrap();
let j=Job { id: 0, priority: u32::MAX, ready_at: u64::MAX };
s.enqueue(j.clone(),0).unwrap();
assert_eq!(s.cancel(0,100),Ok(Some(j)));
assert_eq!(s.next_ready(100),Ok(None));
assert_eq!(s.len(),0);
}

#[test]
fn hidden_scheduler_recompute_later() {
let mut s=Scheduler::new(10,0).unwrap();
s.enqueue(Job { id: 90, priority: 1, ready_at: 0 },0).unwrap();
s.enqueue(Job { id: 3, priority: 2, ready_at: 5 },5).unwrap();
s.enqueue(Job { id: 4, priority: 9, ready_at: 5 },5).unwrap();
assert_eq!(s.next_ready(5).unwrap().unwrap().job.id,4);
let d=s.next_ready(10).unwrap().unwrap();
assert_eq!(d.job.id,90);
assert_eq!(d.effective_priority,2);
}

#[test]
fn hidden_scheduler_near_max_clock() {
let m=u64::MAX;
let mut s=Scheduler::new(2,m-4).unwrap();
s.enqueue(Job { id: m, priority: 7, ready_at: m },m-4).unwrap();
assert_eq!(s.next_ready(m-1),Ok(None));
let d=s.next_ready(m).unwrap().unwrap();
assert_eq!(d.effective_priority,9);
assert_eq!(d.enqueued_at,m-4);
assert_eq!(s.cancel(m,m-1),Err(SchedulerError::TimeWentBackwards));
}

#[test]
fn hidden_scheduler_large_stable_order() {
let mut s=Scheduler::new(u64::MAX,0).unwrap();
for i in (0..200u64).rev() { s.enqueue(Job { id: i, priority: (i%5) as u32, ready_at: 2 },0).unwrap(); }
assert_eq!(s.len(),200);
for p in (0..5u32).rev() { for id in (0..200u64).rev().filter(|i| (*i%5) as u32==p) { let d=s.next_ready(2).unwrap().unwrap(); assert_eq!(d.job.id,id); assert_eq!(d.effective_priority,p); } }
assert_eq!(s.len(),0);
}
