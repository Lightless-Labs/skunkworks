fn task(duration: u64, deps: &[usize]) -> Task { Task { duration, deps: deps.to_vec() } }
// R1-R3, R8-R13
#[test]
fn public_chain() {
let p = earliest(&[task(3,&[]),task(5,&[0]),task(2,&[1])]).unwrap();
assert_eq!(p, Plan { order: vec![0,1,2], times: vec![Timing{start:0,finish:3},Timing{start:3,finish:8},Timing{start:8,finish:10}], makespan:10 });
}

// R10-R12, R17
#[test]
fn public_parallel() {
let p = earliest(&[task(4,&[]),task(7,&[]),task(1,&[0,1])]).unwrap();
assert_eq!(p.times[2], Timing{start:7,finish:8}); assert_eq!(p.makespan,8);
}

// R6
#[test]
fn public_cycle() {
assert_eq!(earliest(&[task(1,&[1]),task(2,&[0])]), Err(Error::Cycle{blocked:vec![0,1]}));
}
