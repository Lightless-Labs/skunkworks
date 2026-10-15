fn task(duration: u64, deps: &[usize]) -> Task { Task { duration, deps: deps.to_vec() } }
// R1-R2, R12-R14
#[test]
fn empty_result() {
assert_eq!(earliest(&[]),Ok(Plan{order:vec![],times:vec![],makespan:0}));
}

// R7-R10, R13
#[test]
fn one_zero() {
assert_eq!(earliest(&[task(0,&[])]).unwrap(),Plan{order:vec![0],times:vec![Timing{start:0,finish:0}],makespan:0});
}

// R3, R5, R8, R11, R17
#[test]
fn backward_edges() {
let p=earliest(&[task(2,&[2]),task(4,&[]),task(3,&[1])]).unwrap(); assert_eq!(p.order,vec![1,2,0]); assert_eq!(p.times,vec![Timing{start:7,finish:9},Timing{start:0,finish:4},Timing{start:4,finish:7}]);
}

// R5, R11
#[test]
fn fresh_small_ready() {
let p=earliest(&[task(1,&[1]),task(1,&[]),task(1,&[]),task(1,&[0])]).unwrap(); assert_eq!(p.order,vec![1,0,2,3]);
}

// R8, R16
#[test]
fn join_uses_max() {
let p=earliest(&[task(9,&[]),task(6,&[]),task(5,&[1,0])]).unwrap(); assert_eq!(p.times[2],Timing{start:9,finish:14});
}

// R7, R10
#[test]
fn zero_transitive() {
let p=earliest(&[task(6,&[]),task(0,&[0]),task(0,&[1]),task(4,&[2])]).unwrap(); assert_eq!(p.times[3],Timing{start:6,finish:10});
}

// R12, R17
#[test]
fn isolated_origin() {
let p=earliest(&[task(10,&[]),task(1,&[]),task(2,&[1]),task(15,&[])]).unwrap(); assert_eq!(p.makespan,15); assert_eq!(p.times[3].start,0); assert_eq!(p.times[2].finish,3);
}

// R3, R16, R19
#[test]
fn equal_parent_permutation() {
let a=vec![task(8,&[]),task(8,&[]),task(3,&[0,1])]; let mut b=a.clone(); b[2].deps.reverse(); assert_eq!(earliest(&a),earliest(&b)); assert_eq!(a[2].deps,vec![0,1]);
}

// R4
#[test]
fn bad_dependency_value() {
assert_eq!(earliest(&[task(1,&[4])]),Err(Error::BadDependency{task:0,position:0,value:4}));
}

// R4
#[test]
fn bad_dependency_position() {
assert_eq!(earliest(&[task(1,&[]),task(1,&[]),task(1,&[0,1,7])]),Err(Error::BadDependency{task:2,position:2,value:7}));
}

// R4, R18
#[test]
fn self_specific() {
assert_eq!(earliest(&[task(0,&[0])]),Err(Error::SelfDependency{task:0,position:0}));
}

// R4
#[test]
fn duplicate_specific() {
assert_eq!(earliest(&[task(1,&[]),task(1,&[0,0])]),Err(Error::DuplicateDependency{task:1,position:1,value:0}));
}

// R4
#[test]
fn position_precedence() {
assert_eq!(earliest(&[task(1,&[]),task(1,&[0,0,9])]),Err(Error::DuplicateDependency{task:1,position:1,value:0}));
}

// R4, R18
#[test]
fn row_precedence() {
assert_eq!(earliest(&[task(1,&[3]),task(1,&[1])]),Err(Error::BadDependency{task:0,position:0,value:3}));
}

// R4, R18
#[test]
fn later_error_before_cycle() {
assert_eq!(earliest(&[task(1,&[1]),task(1,&[0]),task(1,&[8])]),Err(Error::BadDependency{task:2,position:0,value:8}));
}

// R4, R18
#[test]
fn later_error_before_overflow() {
assert_eq!(earliest(&[task(u64::MAX,&[]),task(1,&[0]),task(0,&[2])]),Err(Error::SelfDependency{task:2,position:0}));
}

// R6
#[test]
fn cycle_includes_downstream() {
assert_eq!(earliest(&[task(2,&[1]),task(2,&[0]),task(1,&[1]),task(5,&[]),task(1,&[2,3])]),Err(Error::Cycle{blocked:vec![0,1,2,4]}));
}

// R6-R7
#[test]
fn all_zero_cycle() {
assert_eq!(earliest(&[task(0,&[2]),task(0,&[0]),task(0,&[1])]),Err(Error::Cycle{blocked:vec![0,1,2]}));
}

// R6, R13
#[test]
fn cycle_before_arithmetic() {
assert_eq!(earliest(&[task(u64::MAX,&[]),task(2,&[0]),task(1,&[3]),task(1,&[2])]),Err(Error::Cycle{blocked:vec![2,3]}));
}

// R7, R15
#[test]
fn maximum_root() {
let p=earliest(&[task(u64::MAX,&[])]).unwrap(); assert_eq!(p.times[0].finish,u64::MAX); assert_eq!(p.makespan,u64::MAX);
}

// R7, R15
#[test]
fn maximum_zero_child() {
let p=earliest(&[task(u64::MAX,&[]),task(0,&[0])]).unwrap(); assert_eq!(p.times[1],Timing{start:u64::MAX,finish:u64::MAX});
}

// R9, R15
#[test]
fn overflow_child() {
assert_eq!(earliest(&[task(u64::MAX,&[]),task(2,&[0])]),Err(Error::Overflow{task:1}));
}

// R5, R9
#[test]
fn overflow_traversal_position() {
assert_eq!(earliest(&[task(1,&[3]),task(1,&[2]),task(u64::MAX,&[]),task(u64::MAX,&[])]),Err(Error::Overflow{task:1}));
}

// R7, R9
#[test]
fn overflow_exact_boundary() {
let p=earliest(&[task(u64::MAX-9,&[]),task(9,&[0]),task(0,&[1])]).unwrap(); assert_eq!(p.makespan,u64::MAX);
}

// R4-R5, R8, R12, R16
#[test]
fn large_shared_fanout() {
let mut ts=vec![task(2,&[])]; for _ in 1..121 {ts.push(task(3,&[0]));} let deps:Vec<_>=(1..121).rev().collect(); ts.push(task(4,&deps)); let p=earliest(&ts).unwrap(); assert_eq!(p.order,(0..122).collect::<Vec<_>>()); assert_eq!(p.times[121],Timing{start:5,finish:9}); assert_eq!(p.makespan,9);
}

// R2-R3, R19
#[test]
fn independent_owned_results() {
let ts=vec![task(12,&[]),task(3,&[0])]; let copy=ts.clone(); let mut first=earliest(&ts).unwrap(); first.times[0].finish=999; assert_eq!(earliest(&ts).unwrap().makespan,15); assert_eq!(ts,copy); assert_eq!(earliest(&[]).unwrap().makespan,0);
}
