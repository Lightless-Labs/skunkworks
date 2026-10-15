fn job(duration: u64, deps: &[usize], release: u64, priority: i32, needs: &[u32]) -> Task { Task { duration, deps: deps.to_vec(), release, priority, needs: needs.to_vec() } }
// R1-R3, R12, R18, R26, R28-R34
#[test]
fn public_exclusive() {
let s=schedule(&[job(3,&[],0,0,&[1]),job(1,&[],0,0,&[1])],&[0,0],&[1]).unwrap(); assert_eq!(s.runs,vec![Run{start:0,finish:3,worker:0},Run{start:3,finish:4,worker:1}]); assert_eq!(s.trace,"workers=2;resources=1
0|start|0|0|1
3|end|0|0|0
3|start|1|1|1
4|end|1|1|0
makespan=4
");
}

// R11, R22, R29-R34
#[test]
fn public_two_resources() {
let s=schedule(&[job(2,&[],0,0,&[1,0]),job(1,&[0],0,0,&[0,2])],&[0],&[2,2]).unwrap(); assert_eq!(s.makespan,3); assert_eq!(s.runs[1].start,2); assert_eq!(s.trace,"workers=1;resources=2
0|start|0|0|1,0
2|end|0|0|0,0
2|start|1|0|0,2
3|end|1|0|0,0
makespan=3
");
}

// R23-R24, R44
#[test]
fn public_resource_error() {
assert_eq!(schedule(&[job(2,&[],0,0,&[4])],&[0],&[3]),Err(Error::ResourceExceeded{task:0,resource:0,need:4,capacity:3}));
}
