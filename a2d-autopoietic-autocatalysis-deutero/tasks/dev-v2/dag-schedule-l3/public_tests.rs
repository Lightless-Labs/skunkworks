fn job(duration: u64, deps: &[usize], release: u64, priority: i32) -> Task { Task { duration, deps: deps.to_vec(), release, priority } }
// R1-R3, R11, R14, R18, R21-R23
#[test]
fn public_serial() {
let s=schedule(&[job(2,&[],0,0),job(3,&[0],0,0)],1,0).unwrap(); assert_eq!(s.runs,vec![Run{start:0,finish:2,worker:0},Run{start:2,finish:5,worker:0}]); assert_eq!(s.makespan,5); assert_eq!(s.events,vec![Event{time:0,task:0,worker:0,kind:EventKind::Start},Event{time:2,task:0,worker:0,kind:EventKind::Finish},Event{time:2,task:1,worker:0,kind:EventKind::Start},Event{time:5,task:1,worker:0,kind:EventKind::Finish}]);
}

// R9, R12-R13, R22
#[test]
fn public_parallel_join() {
let s=schedule(&[job(4,&[],0,0),job(2,&[],0,0),job(1,&[0,1],0,0)],2,0).unwrap(); assert_eq!(s.runs,vec![Run{start:0,finish:4,worker:0},Run{start:0,finish:2,worker:1},Run{start:4,finish:5,worker:0}]); assert_eq!(s.makespan,5);
}

// R10-R12, R17
#[test]
fn public_release() {
let s=schedule(&[job(1,&[],3,9),job(2,&[],0,-1)],1,0).unwrap(); assert_eq!(s.runs[1].start,0); assert_eq!(s.runs[0].start,3); assert_eq!(s.makespan,4);
}
