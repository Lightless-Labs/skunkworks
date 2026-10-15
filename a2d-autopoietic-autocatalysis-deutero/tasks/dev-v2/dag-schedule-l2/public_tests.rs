fn task(duration: u64, deps: &[usize]) -> Task { Task { duration, deps: deps.to_vec() } }
// R1-R3, R8-R18, R23
#[test]
fn public_diamond() {
let a=analyze(&[task(2,&[]),task(4,&[0]),task(1,&[0]),task(3,&[1,2])],9).unwrap(); assert_eq!(a.makespan,9); assert_eq!(a.slack,vec![0,0,3,0]); assert_eq!(a.critical,vec![0,1,3]); assert_eq!(a.path,vec![0,1,3]); assert_eq!(a.latest[2],Timing{start:5,finish:6});
}

// R14, R17-R18
#[test]
fn public_loose_target() {
let a=analyze(&[task(3,&[]),task(2,&[0])],8).unwrap(); assert_eq!(a.slack,vec![3,3]); assert_eq!(a.critical,vec![0,1]); assert_eq!(a.latest[0],Timing{start:3,finish:6});
}

// R13-R14
#[test]
fn public_target_error() {
assert_eq!(analyze(&[task(6,&[])],5),Err(Error::TargetTooSmall{minimum:6}));
}
