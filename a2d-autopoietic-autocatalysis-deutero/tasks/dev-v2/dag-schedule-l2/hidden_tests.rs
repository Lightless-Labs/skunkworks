fn task(duration: u64, deps: &[usize]) -> Task { Task { duration, deps: deps.to_vec() } }
// R1-R2, R12, R22, R24
#[test]
fn empty_any_target() {
for target in [0,17,u64::MAX] {assert_eq!(analyze(&[],target),Ok(Analysis{order:vec![],earliest:vec![],latest:vec![],slack:vec![],critical:vec![],path:vec![],makespan:0}));}
}

// R3, R5, R8, R11
#[test]
fn backward_and_reinsert() {
let a=analyze(&[task(2,&[1]),task(3,&[]),task(1,&[])],7).unwrap(); assert_eq!(a.order,vec![1,0,2]); assert_eq!(a.earliest[0],Timing{start:3,finish:5}); assert_eq!(a.latest[0],Timing{start:5,finish:7});
}

// R4, R25
#[test]
fn bad_value_position() {
assert_eq!(analyze(&[task(1,&[]),task(1,&[0,4])],0),Err(Error::BadDependency{task:1,position:1,value:4}));
}

// R4, R26
#[test]
fn self_beats_target() {
assert_eq!(analyze(&[task(9,&[0])],0),Err(Error::SelfDependency{task:0,position:0}));
}

// R4, R26
#[test]
fn duplicate_beats_later_value() {
assert_eq!(analyze(&[task(1,&[]),task(1,&[0,0,9])],0),Err(Error::DuplicateDependency{task:1,position:1,value:0}));
}

// R4, R26
#[test]
fn validation_before_cycle() {
assert_eq!(analyze(&[task(1,&[1]),task(1,&[0]),task(1,&[2])],0),Err(Error::SelfDependency{task:2,position:0}));
}

// R6, R13
#[test]
fn blocked_descendants() {
assert_eq!(analyze(&[task(1,&[2]),task(0,&[]),task(0,&[0]),task(1,&[2])],0),Err(Error::Cycle{blocked:vec![0,2,3]}));
}

// R6-R7, R14
#[test]
fn cycle_precedes_overflow() {
assert_eq!(analyze(&[task(u64::MAX,&[]),task(1,&[0]),task(0,&[3]),task(0,&[2])],0),Err(Error::Cycle{blocked:vec![2,3]}));
}

// R9, R13-R14
#[test]
fn overflow_precedes_target() {
assert_eq!(analyze(&[task(u64::MAX,&[]),task(3,&[0])],0),Err(Error::Overflow{task:1}));
}

// R5, R9, R25
#[test]
fn overflow_kahn_not_id() {
assert_eq!(analyze(&[task(2,&[3]),task(3,&[2]),task(u64::MAX,&[]),task(u64::MAX,&[])],u64::MAX),Err(Error::Overflow{task:1}));
}

// R12-R14
#[test]
fn invalid_target_minimum() {
assert_eq!(analyze(&[task(11,&[]),task(3,&[]),task(5,&[1])],7),Err(Error::TargetTooSmall{minimum:11}));
}

// R15-R17
#[test]
fn latest_uses_min_child() {
let a=analyze(&[task(2,&[]),task(1,&[0]),task(7,&[0]),task(3,&[1])],12).unwrap(); assert_eq!(a.latest[0],Timing{start:3,finish:5}); assert_eq!(a.slack,vec![3,6,3,6]);
}

// R15-R16, R23
#[test]
fn all_sinks_target() {
let a=analyze(&[task(4,&[]),task(9,&[]),task(2,&[0])],15).unwrap(); assert_eq!(a.latest[1].finish,15); assert_eq!(a.latest[2].finish,15); assert_eq!(a.slack,vec![9,6,9]); assert_eq!(a.critical,vec![1]); assert_eq!(a.path,vec![1]);
}

// R17-R18
#[test]
fn positive_float_critical() {
let a=analyze(&[task(8,&[]),task(3,&[]),task(2,&[1])],20).unwrap(); assert_eq!(a.critical,vec![0]); assert_eq!(a.slack,vec![12,15,15]);
}

// R19, R23
#[test]
fn sink_tie_greatest() {
let a=analyze(&[task(5,&[]),task(2,&[]),task(3,&[1]),task(0,&[0])],9).unwrap(); assert_eq!(a.path,vec![0,3]); assert_eq!(a.critical,vec![0,1,2,3]);
}

// R20
#[test]
fn parent_tie_greatest() {
let a=analyze(&[task(6,&[]),task(6,&[]),task(1,&[1,0])],7).unwrap(); assert_eq!(a.path,vec![1,2]);
}

// R19-R20
#[test]
fn local_not_whole_path() {
let a=analyze(&[task(1,&[]),task(1,&[]),task(1,&[0]),task(1,&[1]),task(1,&[3,2])],3).unwrap(); assert_eq!(a.path,vec![1,3,4]);
}

// R10, R19, R21
#[test]
fn zero_sink_required() {
let a=analyze(&[task(10,&[]),task(0,&[0]),task(0,&[1])],12).unwrap(); assert_eq!(a.path,vec![0,1,2]); assert_eq!(a.earliest[2],Timing{start:10,finish:10});
}

// R20-R21
#[test]
fn zero_parent_tie() {
let a=analyze(&[task(0,&[]),task(0,&[0]),task(1,&[0,1])],1).unwrap(); assert_eq!(a.path,vec![0,1,2]);
}

// R7, R18-R21
#[test]
fn all_zero_graph() {
let a=analyze(&[task(0,&[]),task(0,&[]),task(0,&[0])],4).unwrap(); assert_eq!(a.critical,vec![0,1,2]); assert_eq!(a.slack,vec![4,4,4]); assert_eq!(a.path,vec![0,2]);
}

// R7, R25
#[test]
fn max_time_zero_successor() {
let a=analyze(&[task(u64::MAX,&[]),task(0,&[0])],u64::MAX).unwrap(); assert_eq!(a.path,vec![0,1]); assert_eq!(a.latest[1],Timing{start:u64::MAX,finish:u64::MAX}); assert_eq!(a.slack,vec![0,0]);
}

// R15-R18
#[test]
fn max_target_small_graph() {
let a=analyze(&[task(2,&[]),task(1,&[0])],u64::MAX).unwrap(); assert_eq!(a.slack,vec![u64::MAX-3,u64::MAX-3]); assert_eq!(a.critical,vec![0,1]);
}

// R17-R20, R24
#[test]
fn target_shift_invariance() {
let ts=vec![task(2,&[]),task(5,&[]),task(4,&[0]),task(0,&[1,2])]; let a=analyze(&ts,6).unwrap(); let b=analyze(&ts,16).unwrap(); assert_eq!(a.path,b.path); assert_eq!(a.critical,b.critical); assert_eq!(a.earliest,b.earliest); for i in 0..ts.len(){assert_eq!(a.slack[i]+10,b.slack[i]);}
}

// R3, R5, R20, R26
#[test]
fn permutation_independent() {
let ts=vec![task(4,&[]),task(4,&[]),task(2,&[0,1])]; let mut other=ts.clone(); other[2].deps.reverse(); assert_eq!(analyze(&ts,8),analyze(&other,8)); assert_eq!(ts[2].deps,vec![0,1]);
}

// R8, R12, R15-R18
#[test]
fn larger_long_short_branches() {
let mut ts=vec![task(1,&[])]; for i in 1..80 {ts.push(task(1,&[i-1]));} ts.push(task(7,&[])); let a=analyze(&ts,85).unwrap(); assert_eq!(a.makespan,80); assert_eq!(a.path,(0..80).collect::<Vec<_>>()); assert_eq!(a.critical,(0..80).collect::<Vec<_>>()); assert_eq!(a.slack[80],78);
}

// R2, R24, R26
#[test]
fn fresh_owned_analysis() {
let ts=vec![task(2,&[])]; let mut a=analyze(&ts,3).unwrap(); a.path.clear(); a.latest[0].finish=0; assert_eq!(analyze(&ts,3).unwrap().path,vec![0]); assert_eq!(analyze(&ts,3).unwrap().latest[0].finish,3);
}
