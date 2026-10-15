// R13-R14, R19-R23
#[test]
fn demotion_basics() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, ticks: 7, level: 0 }, Job { id: 2, arrival: 0, ticks: 2, level: 0 }], &[2, 4], 0), Ok(Schedule {
        runs: vec![Run { id: 1, start: 0, end: 2, finished: false, level: 0 }, Run { id: 2, start: 2, end: 4, finished: true, level: 0 }, Run { id: 1, start: 4, end: 8, finished: false, level: 1 }, Run { id: 1, start: 8, end: 9, finished: true, level: 1 }],
        idle: vec![],
        completion: vec![(1, 9), (2, 4)],
        end: 9
    }));
}

// R11, R14, R25-R28
#[test]
fn priority_and_idle() {
    assert_eq!(schedule(&[Job { id: 3, arrival: 3, ticks: 1, level: 1 }, Job { id: 4, arrival: 3, ticks: 1, level: 0 }], &[2, 3], 0), Ok(Schedule {
        runs: vec![Run { id: 4, start: 3, end: 4, finished: true, level: 0 }, Run { id: 3, start: 4, end: 5, finished: true, level: 1 }],
        idle: vec![(0, 3)],
        completion: vec![(3, 5), (4, 4)],
        end: 5
    }));
}

// R27
#[test]
fn empty_priority_schedule() {
    assert_eq!(schedule(&[], &[1], 0), Ok(Schedule {
        runs: vec![],
        idle: vec![],
        completion: vec![],
        end: 0
    }));
}

// R9
#[test]
fn reject_invalid_level() {
    assert_eq!(schedule(&[Job { id: 5, arrival: 0, ticks: 1, level: 2 }], &[2, 3], 0), Err(Error::InitialLevel { index: 0 }));
}
