// R20-R24, R34-R38
#[test]
fn cpu_only_demotion() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![6], level: 0 }, Job { id: 2, arrival: 0, bursts: vec![2], level: 0 }], &[2, 4], 0), Ok(Schedule {
        runs: vec![Run { id: 1, start: 0, end: 2, finished: false, level: 0 }, Run { id: 2, start: 2, end: 4, finished: true, level: 0 }, Run { id: 1, start: 4, end: 8, finished: true, level: 1 }],
        idle: vec![],
        stats: vec![Stat { id: 1, completion: 8, turnaround: 8, waiting: 2 }, Stat { id: 2, completion: 4, turnaround: 4, waiting: 2 }],
        mean_waiting_x100: 200, mean_turnaround_x100: 600,
        end: 8
    }));
}

// R25-R26, R31-R36
#[test]
fn io_idle_and_promotion() {
    assert_eq!(schedule(&[Job { id: 5, arrival: 2, bursts: vec![1, 3, 2], level: 1 }], &[3, 4], 0), Ok(Schedule {
        runs: vec![Run { id: 5, start: 2, end: 3, finished: false, level: 1 }, Run { id: 5, start: 6, end: 8, finished: true, level: 0 }],
        idle: vec![(0, 2), (3, 6)],
        stats: vec![Stat { id: 5, completion: 8, turnaround: 6, waiting: 0 }],
        mean_waiting_x100: 0, mean_turnaround_x100: 600,
        end: 8
    }));
}

// R33, R38
#[test]
fn empty_statistics() {
    assert_eq!(schedule(&[], &[1], 0), Ok(Schedule {
        runs: vec![],
        idle: vec![],
        stats: vec![],
        mean_waiting_x100: 0, mean_turnaround_x100: 0,
        end: 0
    }));
}

// R11
#[test]
fn invalid_cpu_burst() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![0], level: 0 }], &[2], 0), Err(Error::Burst { index: 0, position: 0 }));
}
