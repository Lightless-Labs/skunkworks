fn job(duration: u64, deps: &[usize], release: u64, priority: i32, needs: &[u32]) -> Task { Task { duration, deps: deps.to_vec(), release, priority, needs: needs.to_vec() } }
// R1-R2, R8, R21-R22, R30, R34-R35
#[test]
fn empty_header_and_workers() {
assert_eq!(schedule(&[],&[8,0,3],&[0,9]),Ok(Schedule{runs:vec![],makespan:0,trace:"workers=3;resources=2
makespan=0
".to_string()})); assert_eq!(schedule(&[],&[],&[]),Err(Error::Workers{count:0})); assert_eq!(schedule(&[],&[0;65],&[]),Err(Error::Workers{count:65})); assert!(schedule(&[],&[u64::MAX;64],&[]).is_ok());
}

// R3-R4, R8, R38, R44
#[test]
fn dependency_errors_first() {
assert_eq!(schedule(&[job(1,&[7],0,0,&[])],&[],&[1]),Err(Error::BadDependency{task:0,position:0,value:7})); assert_eq!(schedule(&[job(0,&[],0,0,&[]),job(0,&[0,0,9],0,0,&[])],&[],&[]),Err(Error::DuplicateDependency{task:1,position:1,value:0})); assert_eq!(schedule(&[job(0,&[0],0,0,&[])],&[0],&[]),Err(Error::SelfDependency{task:0,position:0}));
}

// R8, R23, R38
#[test]
fn worker_before_shape() {
assert_eq!(schedule(&[job(0,&[],0,0,&[])],&[],&[0]),Err(Error::Workers{count:0}));
}

// R23, R25, R44
#[test]
fn shape_payloads() {
assert_eq!(schedule(&[job(0,&[],0,0,&[]),job(0,&[],0,0,&[0])],&[0],&[0]),Err(Error::ResourceShape{task:0,expected:1,actual:0})); assert_eq!(schedule(&[job(0,&[],0,0,&[0])],&[0],&[]),Err(Error::ResourceShape{task:0,expected:0,actual:1}));
}

// R23-R24, R38, R44
#[test]
fn row_resource_precedence() {
assert_eq!(schedule(&[job(1,&[],0,0,&[1,6]),job(1,&[],0,0,&[])],&[0],&[1,5]),Err(Error::ResourceExceeded{task:0,resource:1,need:6,capacity:5})); assert_eq!(schedule(&[job(1,&[],0,0,&[4,6])],&[0],&[1,5]),Err(Error::ResourceExceeded{task:0,resource:0,need:4,capacity:1}));
}

// R24-R25, R38
#[test]
fn resource_before_cycle() {
assert_eq!(schedule(&[job(0,&[1],0,0,&[0]),job(0,&[0],0,0,&[1])],&[0],&[0]),Err(Error::ResourceExceeded{task:1,resource:0,need:1,capacity:0}));
}

// R6, R9, R38
#[test]
fn cycle_blocked_and_precedence() {
assert_eq!(schedule(&[job(0,&[1],0,0,&[]),job(0,&[0],0,0,&[]),job(0,&[0],0,0,&[]),job(u64::MAX,&[],0,0,&[]),job(1,&[3],0,0,&[])],&[0],&[]),Err(Error::Cycle{blocked:vec![0,1,2]}));
}

// R5, R7, R9, R38
#[test]
fn reverse_kahn_tail_overflow() {
assert_eq!(schedule(&[job(u64::MAX,&[],0,0,&[]),job(u64::MAX,&[],u64::MAX,0,&[]),job(1,&[0],0,0,&[]),job(1,&[1],0,0,&[])],&[1],&[]),Err(Error::Overflow{task:1,stage:Stage::Tail}));
}

// R9-R12, R39
#[test]
fn tail_and_priority_ties() {
let s=schedule(&[job(4,&[],0,i32::MAX,&[]),job(2,&[],0,i32::MIN,&[]),job(5,&[1],0,0,&[])],&[0],&[]).unwrap(); assert_eq!(s.runs[1].start,0); assert_eq!(s.runs[2].start,2); let x=schedule(&[job(1,&[],0,i32::MIN,&[]),job(1,&[],0,i32::MAX,&[]),job(1,&[],0,i32::MAX,&[])],&[0],&[]).unwrap(); assert_eq!(x.runs.iter().map(|r|r.start).collect::<Vec<_>>(),vec![2,0,1]);
}

// R12, R17, R26-R27, R39, R41
#[test]
fn bypass_nonfitting_head() {
let s=schedule(&[job(6,&[],0,0,&[2]),job(5,&[],0,0,&[2]),job(2,&[],0,0,&[1])],&[0,0],&[3]).unwrap(); assert_eq!(s.runs[0].start,0); assert_eq!(s.runs[2].start,0); assert_eq!(s.runs[1].start,6); assert!(s.trace.contains("0|start|2|1|3
"));
}

// R22, R26-R27, R41
#[test]
fn resources_not_interchangeable() {
let s=schedule(&[job(4,&[],0,0,&[2,0]),job(3,&[],0,0,&[1,0]),job(2,&[],0,0,&[0,2])],&[0,0],&[2,9]).unwrap(); assert_eq!(s.runs[2].start,0); assert_eq!(s.runs[1].start,4);
}

// R25-R27, R33, R41
#[test]
fn zero_demand_runs_during_full_usage() {
let s=schedule(&[job(5,&[],0,0,&[1]),job(2,&[],0,0,&[0])],&[0,0],&[1]).unwrap(); assert_eq!(s.runs[1].start,0); assert_eq!(s.trace,"workers=2;resources=1
0|start|0|0|1
0|start|1|1|1
2|end|1|1|1
5|end|0|0|0
makespan=5
");
}

// R18, R28, R37
#[test]
fn cyclic_worker_preference() {
let s=schedule(&[job(1,&[],4,0,&[]),job(2,&[],0,0,&[])],&[0,0,0],&[]).unwrap(); assert_eq!(s.runs[1].worker,1); assert_eq!(s.runs[0].worker,0); assert_eq!(s.trace,"workers=3;resources=0
0|start|1|1|-
2|end|1|1|-
4|start|0|0|-
5|end|0|0|-
makespan=5
");
}

// R21, R28
#[test]
fn worker_scan_wraps() {
let s=schedule(&[job(0,&[],10,0,&[]),job(0,&[],10,0,&[]),job(2,&[],1,0,&[]),job(0,&[],10,0,&[]),job(0,&[],10,0,&[]),job(5,&[],0,0,&[])],&[0,0,0],&[]).unwrap(); assert_eq!(s.runs[5].worker,2); assert_eq!(s.runs[2],Run{start:1,finish:3,worker:0});
}

// R15, R18, R21, R26, R37
#[test]
fn per_worker_cooldown_not_held_resources() {
let s=schedule(&[job(3,&[],0,0,&[1]),job(2,&[0],0,0,&[1])],&[10,0],&[1]).unwrap(); assert_eq!(s.runs[1],Run{start:3,finish:5,worker:1}); assert_eq!(s.makespan,5); assert!(s.trace.contains("3|end|0|0|0
3|start|1|1|1
"));
}

// R15, R17, R21, R28
#[test]
fn cooldown_changes_next_admission() {
let s=schedule(&[job(2,&[],0,0,&[]),job(1,&[0],0,0,&[]),job(1,&[1],0,0,&[])],&[6,2],&[]).unwrap(); assert_eq!(s.runs,vec![Run{start:0,finish:2,worker:0},Run{start:2,finish:3,worker:1},Run{start:5,finish:6,worker:1}]); assert_eq!(s.makespan,6);
}

// R13, R26, R31-R33, R37
#[test]
fn simultaneous_end_snapshots() {
let s=schedule(&[job(3,&[],0,0,&[1,1]),job(3,&[],0,9,&[2,0]),job(2,&[0,1],0,0,&[3,1])],&[0,0],&[3,1]).unwrap(); assert_eq!(s.trace,"workers=2;resources=2
0|start|1|1|2,0
0|start|0|0|3,1
3|end|0|0|2,0
3|end|1|1|0,0
3|start|2|0|3,1
5|end|2|0|0,0
makespan=5
");
}

// R7, R12-R13, R16, R25-R26, R31-R36
#[test]
fn zero_start_end_reselection() {
let s=schedule(&[job(0,&[],0,5,&[1]),job(0,&[],0,0,&[1]),job(0,&[0],0,9,&[1])],&[0],&[1]).unwrap(); assert_eq!(s.trace,"workers=1;resources=1
0|start|0|0|1
0|end|0|0|0
0|start|2|0|1
0|end|2|0|0
0|start|1|0|1
0|end|1|0|0
makespan=0
");
}

// R15-R16, R21, R25, R36
#[test]
fn zero_still_incurs_cooldown() {
let s=schedule(&[job(0,&[],0,1,&[1]),job(0,&[],0,0,&[1])],&[4],&[1]).unwrap(); assert_eq!(s.runs[1].start,4); assert_eq!(s.makespan,4); assert_eq!(s.trace,"workers=1;resources=1
0|start|0|0|1
0|end|0|0|0
4|start|1|0|1
4|end|1|0|0
makespan=4
");
}

// R10-R12, R17, R39, R43
#[test]
fn release_no_reservation_or_preemption() {
let s=schedule(&[job(8,&[],2,100,&[1]),job(3,&[],0,0,&[1])],&[0,0],&[1]).unwrap(); assert_eq!(s.runs[1].start,0); assert_eq!(s.runs[0].start,3); assert_eq!(s.makespan,11);
}

// R7, R14, R19, R38
#[test]
fn finish_error_precedes_cooldown() {
assert_eq!(schedule(&[job(2,&[],u64::MAX,0,&[])],&[u64::MAX],&[]),Err(Error::Overflow{task:0,stage:Stage::Finish}));
}

// R14, R19, R21, R28, R42
#[test]
fn chosen_worker_overflow_no_fallback() {
assert_eq!(schedule(&[job(0,&[],0,0,&[]),job(2,&[],u64::MAX-2,0,&[])],&[0,1],&[]),Err(Error::Overflow{task:1,stage:Stage::Turnaround}));
}

// R7, R14, R16, R19, R34
#[test]
fn exact_max_and_zero_release() {
let s=schedule(&[job(0,&[],u64::MAX,0,&[])],&[0],&[]).unwrap(); assert_eq!(s.runs[0],Run{start:u64::MAX,finish:u64::MAX,worker:0}); assert_eq!(s.makespan,u64::MAX); assert_eq!(s.trace,format!("workers=1;resources=0
{}|start|0|0|-
{}|end|0|0|-
makespan={}
",u64::MAX,u64::MAX,u64::MAX)); let s=schedule(&[job(5,&[],u64::MAX-5,0,&[])],&[0],&[]).unwrap(); assert_eq!(s.makespan,u64::MAX);
}

// R22, R26, R31-R33, R40
#[test]
fn full_width_resources() {
let s=schedule(&[job(3,&[],0,0,&[u32::MAX]),job(2,&[],0,0,&[1])],&[0,0],&[u32::MAX]).unwrap(); assert_eq!(s.runs[1].start,3); assert!(s.trace.contains("0|start|0|0|4294967295
")); let x=schedule(&[job(3,&[],0,0,&[u32::MAX-1]),job(2,&[],0,0,&[1])],&[0,0],&[u32::MAX]).unwrap(); assert!(x.trace.contains("0|start|1|1|4294967295
"));
}

// R22, R25, R33
#[test]
fn all_zero_capacities() {
let s=schedule(&[job(2,&[],0,0,&[0,0,0]),job(1,&[],0,0,&[0,0,0])],&[0,0],&[0,0,0]).unwrap(); assert_eq!(s.runs[1].start,0); assert!(s.trace.contains("0|start|1|1|0,0,0
"));
}

// R14, R18-R19, R42
#[test]
fn last_task_cooldown_error() {
assert_eq!(schedule(&[job(u64::MAX,&[],0,0,&[0])],&[1],&[0]),Err(Error::Overflow{task:0,stage:Stage::Turnaround}));
}

// R9, R38, R43
#[test]
fn releases_ignored_by_tail_overflow() {
assert_eq!(schedule(&[job(u64::MAX,&[],u64::MAX,0,&[]),job(1,&[0],u64::MAX,0,&[])],&[0],&[]),Err(Error::Overflow{task:0,stage:Stage::Tail}));
}

// R9, R12
#[test]
fn tail_max_not_branch_sum() {
let s=schedule(&[job(1,&[],0,0,&[]),job(3,&[0],0,0,&[]),job(3,&[0],0,0,&[]),job(5,&[],0,0,&[])],&[0],&[]).unwrap(); assert_eq!(s.runs[3].start,0);
}

// R3, R5, R11, R18, R20, R29
#[test]
fn input_order_reverse_dependencies() {
let ts=vec![job(2,&[2,1],0,0,&[1]),job(3,&[],0,0,&[1]),job(3,&[],0,0,&[1])]; let mut reversed=ts.clone(); reversed[0].deps.reverse(); let s=schedule(&ts,&[0,0],&[2]).unwrap(); assert_eq!(s.runs[0].start,3); assert_eq!(s.makespan,5); assert_eq!(s,schedule(&reversed,&[0,0],&[2]).unwrap()); assert_eq!(ts[0].deps,vec![2,1]);
}

// R17-R18, R26-R28, R30-R34, R40
#[test]
fn larger_resource_pool() {
let ts:Vec<_>=(0..90).map(|_|job(1,&[],0,0,&[1])).collect(); let s=schedule(&ts,&[0;8],&[3]).unwrap(); assert_eq!(s.makespan,30); for i in 0..90{assert_eq!(s.runs[i].start,(i/3) as u64);} assert_eq!(s.trace.lines().count(),182); assert!(s.trace.ends_with("makespan=30
"));
}

// R2, R20, R29, R34
#[test]
fn owned_output_and_no_state() {
let ts=vec![job(4,&[],0,0,&[1])]; let copy=ts.clone(); let mut s=schedule(&ts,&[0],&[1]).unwrap(); s.trace.clear(); s.runs.clear(); assert_eq!(schedule(&ts,&[0],&[1]).unwrap().runs.len(),1); assert_eq!(schedule(&[],&[0],&[1]).unwrap().trace,"workers=1;resources=1
makespan=0
"); assert_eq!(ts,copy);
}

// R12, R14, R19, R42
#[test]
fn simulation_error_ranking() {
assert_eq!(schedule(&[job(0,&[],u64::MAX,0,&[]),job(0,&[],u64::MAX,9,&[])],&[1,2],&[]),Err(Error::Overflow{task:1,stage:Stage::Turnaround}));
}

// R13, R26-R27, R31-R32, R37
#[test]
fn completion_batch_before_independent_admission() {
let s=schedule(&[job(3,&[],0,0,&[1]),job(3,&[],0,5,&[2]),job(2,&[],0,0,&[1])],&[0,0],&[3]).unwrap(); assert_eq!(s.trace,"workers=2;resources=1
0|start|1|1|2
0|start|0|0|3
3|end|0|0|2
3|end|1|1|0
3|start|2|0|1
5|end|2|0|0
makespan=5
");
}
