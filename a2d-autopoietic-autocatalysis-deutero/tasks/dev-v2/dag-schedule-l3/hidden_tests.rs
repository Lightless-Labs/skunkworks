fn job(duration: u64, deps: &[usize], release: u64, priority: i32) -> Task { Task { duration, deps: deps.to_vec(), release, priority } }
// R1-R2, R8, R21, R25
#[test]
fn empty_and_boundaries() {
assert_eq!(schedule(&[],64,u64::MAX),Ok(Schedule{runs:vec![],makespan:0,events:vec![]})); for w in [0,65,usize::MAX]{assert_eq!(schedule(&[],w,0),Err(Error::Workers{count:w}));}
}

// R3-R4, R28
#[test]
fn dependency_entry_errors() {
assert_eq!(schedule(&[job(0,&[5],0,0)],0,0),Err(Error::BadDependency{task:0,position:0,value:5})); assert_eq!(schedule(&[job(0,&[0],0,0)],1,0),Err(Error::SelfDependency{task:0,position:0})); assert_eq!(schedule(&[job(0,&[],0,0),job(0,&[0,0,8],0,0)],1,0),Err(Error::DuplicateDependency{task:1,position:1,value:0}));
}

// R4, R8, R28
#[test]
fn row_validation_before_cycle() {
assert_eq!(schedule(&[job(1,&[1],0,0),job(1,&[0],0,0),job(0,&[2],0,0)],0,0),Err(Error::SelfDependency{task:2,position:0}));
}

// R8, R28
#[test]
fn worker_before_cycle() {
assert_eq!(schedule(&[job(1,&[1],0,0),job(1,&[0],0,0)],0,0),Err(Error::Workers{count:0}));
}

// R6, R8, R28
#[test]
fn cycle_descendants_before_tail() {
assert_eq!(schedule(&[job(u64::MAX,&[],0,0),job(1,&[0],0,0),job(0,&[3],0,0),job(0,&[2],0,0),job(2,&[3],0,0)],1,0),Err(Error::Cycle{blocked:vec![2,3,4]}));
}

// R5, R7, R9, R28
#[test]
fn tail_overflow_reverse_order() {
let ts=vec![job(u64::MAX,&[],0,0),job(u64::MAX,&[],0,0),job(1,&[0],0,0),job(1,&[1],0,0)]; assert_eq!(schedule(&ts,1,0),Err(Error::Overflow{task:1,stage:Stage::Tail}));
}

// R9, R19, R28
#[test]
fn tail_before_simulation_error() {
assert_eq!(schedule(&[job(u64::MAX,&[],u64::MAX,0),job(1,&[0],0,0)],1,1),Err(Error::Overflow{task:0,stage:Stage::Tail}));
}

// R9, R12, R29
#[test]
fn bottom_level_not_duration() {
let s=schedule(&[job(4,&[],0,0),job(2,&[],0,0),job(5,&[1],0,0)],1,0).unwrap(); assert_eq!(s.runs[1].start,0); assert_eq!(s.runs[0].start,7);
}

// R10, R12
#[test]
fn priority_after_tail() {
let s=schedule(&[job(3,&[],0,i32::MAX),job(5,&[],0,i32::MIN)],1,0).unwrap(); assert_eq!(s.runs[1].start,0); assert_eq!(s.runs[0].start,5); let x=schedule(&[job(2,&[],0,i32::MIN),job(2,&[],0,i32::MAX)],1,0).unwrap(); assert_eq!(x.runs[1].start,0);
}

// R12, R22
#[test]
fn id_ties_ascending() {
let s=schedule(&[job(2,&[],0,3),job(2,&[],0,3),job(2,&[],0,3)],2,0).unwrap(); assert_eq!(s.runs,vec![Run{start:0,finish:2,worker:0},Run{start:0,finish:2,worker:1},Run{start:2,finish:4,worker:0}]);
}

// R10-R12, R17
#[test]
fn release_no_anticipatory_idle() {
let s=schedule(&[job(6,&[],2,100),job(3,&[],0,0)],1,0).unwrap(); assert_eq!(s.runs[1].start,0); assert_eq!(s.runs[0].start,3);
}

// R11, R26
#[test]
fn dependency_and_release_max() {
let s=schedule(&[job(3,&[],4,0),job(2,&[0],10,0),job(1,&[1],0,0)],1,0).unwrap(); assert_eq!(s.runs.iter().map(|r|r.start).collect::<Vec<_>>(),vec![4,10,12]);
}

// R15, R18, R21, R26
#[test]
fn turnaround_serial() {
let s=schedule(&[job(2,&[],0,0),job(1,&[0],0,0)],1,4).unwrap(); assert_eq!(s.runs[1].start,6); assert_eq!(s.makespan,7); assert_eq!(s.events[1].time,2);
}

// R15, R22, R26
#[test]
fn successor_uses_other_worker() {
let s=schedule(&[job(2,&[],0,0),job(1,&[0],0,0)],2,8).unwrap(); assert_eq!(s.runs[1],Run{start:2,finish:3,worker:1});
}

// R13, R23-R24
#[test]
fn end_batch_before_start_id_order() {
let s=schedule(&[job(3,&[],0,0),job(3,&[],0,5),job(2,&[0],0,0),job(2,&[1],0,0)],2,0).unwrap(); assert_eq!(s.runs[1].worker,0); assert_eq!(s.runs[0].worker,1); let es:Vec<_>=s.events.iter().filter(|e|e.time==3).cloned().collect(); assert_eq!(es,vec![Event{time:3,task:0,worker:1,kind:EventKind::Finish},Event{time:3,task:1,worker:0,kind:EventKind::Finish},Event{time:3,task:2,worker:0,kind:EventKind::Start},Event{time:3,task:3,worker:1,kind:EventKind::Start}]);
}

// R12-R13, R16, R27
#[test]
fn zero_reselects_unblocked_priority() {
let s=schedule(&[job(0,&[],0,5),job(0,&[],0,0),job(0,&[0],0,10)],1,0).unwrap(); let ids:Vec<_>=s.events.iter().filter(|e|e.kind==EventKind::Start).map(|e|e.task).collect(); assert_eq!(ids,vec![0,2,1]); assert_eq!(s.events.iter().map(|e|e.kind.clone()).collect::<Vec<_>>(),vec![EventKind::Start,EventKind::Finish,EventKind::Start,EventKind::Finish,EventKind::Start,EventKind::Finish]); assert_eq!(s.makespan,0);
}

// R15-R16, R21, R27
#[test]
fn zero_turnaround_still_blocks() {
let s=schedule(&[job(0,&[],0,2),job(0,&[],0,1),job(0,&[],0,0)],1,5).unwrap(); assert_eq!(s.runs.iter().map(|r|r.start).collect::<Vec<_>>(),vec![0,5,10]); assert_eq!(s.makespan,10);
}

// R15-R16, R22
#[test]
fn zero_no_hold_other_worker() {
let s=schedule(&[job(0,&[],0,9),job(0,&[0],0,10)],2,7).unwrap(); assert_eq!(s.runs[1],Run{start:0,finish:0,worker:1}); assert_eq!(s.events[1].kind,EventKind::Finish);
}

// R7, R14, R19
#[test]
fn finish_overflow_at_release() {
assert_eq!(schedule(&[job(2,&[],u64::MAX,0)],1,1),Err(Error::Overflow{task:0,stage:Stage::Finish}));
}

// R7, R14, R18, R30
#[test]
fn turnaround_overflow_even_last() {
assert_eq!(schedule(&[job(u64::MAX,&[],0,0)],1,1),Err(Error::Overflow{task:0,stage:Stage::Turnaround}));
}

// R7, R16, R19
#[test]
fn zero_max_release_succeeds() {
let s=schedule(&[job(0,&[],u64::MAX,0)],1,0).unwrap(); assert_eq!(s.runs[0].start,u64::MAX); assert_eq!(s.makespan,u64::MAX); assert_eq!(s.events.len(),2);
}

// R7, R14, R30
#[test]
fn exact_max_finish() {
let s=schedule(&[job(5,&[],u64::MAX-5,0)],1,0).unwrap(); assert_eq!(s.runs[0].finish,u64::MAX);
}

// R14-R15, R19
#[test]
fn worker_delay_causes_finish_overflow() {
assert_eq!(schedule(&[job(u64::MAX-1,&[],0,1),job(1,&[],0,0)],1,1),Err(Error::Overflow{task:1,stage:Stage::Finish}));
}

// R12, R14, R19
#[test]
fn first_admission_error_order() {
assert_eq!(schedule(&[job(0,&[],u64::MAX,0),job(0,&[],u64::MAX,5)],2,1),Err(Error::Overflow{task:1,stage:Stage::Turnaround}));
}

// R9, R29
#[test]
fn tail_uses_max_not_sum() {
let s=schedule(&[job(1,&[],0,0),job(3,&[0],0,0),job(3,&[0],0,0),job(5,&[],0,0)],1,0).unwrap(); assert_eq!(s.runs[3].start,0);
}

// R3, R5, R9, R18, R20
#[test]
fn large_reverse_chain() {
let n=110; let ts:Vec<_>=(0..n).map(|i|{let ds=if i+1<n {vec![i+1]} else {vec![]}; job(1,&ds,0,0)}).collect(); let copy=ts.clone(); let s=schedule(&ts,4,0).unwrap(); assert_eq!(s.makespan,n as u64); for i in 0..n{assert_eq!(s.runs[i].start,(n-1-i) as u64);} assert_eq!(ts,copy); assert_eq!(s,schedule(&ts,4,0).unwrap());
}

// R9-R12, R17
#[test]
fn future_release_ignores_tail_readiness() {
let s=schedule(&[job(1,&[],0,0),job(9,&[0],40,0),job(3,&[],0,0)],1,0).unwrap(); assert_eq!(s.runs[0].start,0); assert_eq!(s.runs[2].start,1); assert_eq!(s.runs[1].start,40); assert_eq!(s.makespan,49);
}

// R2, R20, R23
#[test]
fn owned_events_independent() {
let ts=vec![job(7,&[],0,0)]; let mut a=schedule(&ts,1,0).unwrap(); a.events.clear(); a.runs[0].finish=1; assert_eq!(schedule(&ts,1,0).unwrap().events.len(),2); assert_eq!(schedule(&[],1,0).unwrap().makespan,0);
}
