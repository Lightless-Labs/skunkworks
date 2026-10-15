// R8-R10, R16-R20
#[test]
fn basic_rotation() {
    assert_eq!(schedule(&[Job { id: 1, ticks: 3 }, Job { id: 2, ticks: 5 }], 2, 0), Ok(Schedule {
        runs: vec![Run { id: 1, start: 0, end: 2, finished: false }, Run { id: 2, start: 2, end: 4, finished: false }, Run { id: 1, start: 4, end: 5, finished: true }, Run { id: 2, start: 5, end: 7, finished: false }, Run { id: 2, start: 7, end: 8, finished: true }],
        completion: vec![(1, 5), (2, 8)],
        end: 8
    }));
}

// R13, R15, R17
#[test]
fn paid_switches() {
    assert_eq!(schedule(&[Job { id: 10, ticks: 1 }, Job { id: 20, ticks: 1 }], 4, 2), Ok(Schedule {
        runs: vec![Run { id: 10, start: 0, end: 1, finished: true }, Run { id: 20, start: 3, end: 4, finished: true }],
        completion: vec![(10, 1), (20, 4)],
        end: 4
    }));
}

// R20-R21
#[test]
fn empty_success() {
    assert_eq!(schedule(&[], 3, 0), Ok(Schedule {
        runs: vec![],
        completion: vec![],
        end: 0
    }));
}

// R3
#[test]
fn bad_quantum() {
    assert_eq!(schedule(&[], 0, 0), Err(Error::Quantum));
}
