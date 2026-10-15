// R2-R3
#[test]
fn level_count_is_first() {
    assert_eq!(schedule(&[], &[], 1001), Err(Error::LevelCount));
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }], &[1, 1, 1, 1, 1, 1, 1, 1, 1], 0), Err(Error::LevelCount));
}

// R2, R4-R5
#[test]
fn quantum_error_index_and_precedence() {
    assert_eq!(schedule(&[], &[1, 0, 1001], 1001), Err(Error::Quantum { level: 1 }));
    assert_eq!(schedule(&[], &[1001], 0), Err(Error::Quantum { level: 0 }));
}

// R5-R6
#[test]
fn cost_and_job_count_precedence() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }], &[1], 1001), Err(Error::SwitchCost));
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }], &[1], 0), Err(Error::TooManyJobs));
}

// R7-R11
#[test]
fn duplicate_beats_all_row_fields() {
    assert_eq!(schedule(&[Job { id: 8, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 9, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 8, arrival: 1001, bursts: vec![0, 1], level: 8 }], &[1], 0), Err(Error::DuplicateId { index: 2 }));
}

// R8-R9
#[test]
fn arrival_before_level() {
    assert_eq!(schedule(&[Job { id: 8, arrival: 1001, bursts: vec![0], level: 8 }], &[1], 0), Err(Error::Arrival { index: 0 }));
    assert_eq!(schedule(&[Job { id: 8, arrival: 4294967295, bursts: vec![1], level: 0 }], &[1], 0), Err(Error::Arrival { index: 0 }));
}

// R9-R10
#[test]
fn initial_level_before_burst_shape() {
    assert_eq!(schedule(&[Job { id: 7, arrival: 0, bursts: vec![1, 0], level: 1 }], &[1], 0), Err(Error::InitialLevel { index: 0 }));
}

// R10-R11
#[test]
fn invalid_burst_shapes() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![], level: 0 }], &[1], 0), Err(Error::BurstCount { index: 0 }));
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![0, 0], level: 0 }], &[1], 0), Err(Error::BurstCount { index: 0 }));
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], level: 0 }], &[1], 0), Err(Error::BurstCount { index: 0 }));
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], level: 0 }], &[1], 0), Err(Error::BurstCount { index: 0 }));
}

// R11-R12
#[test]
fn first_bad_burst_position() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![1, 1001, 0], level: 0 }], &[1], 0), Err(Error::Burst { index: 0, position: 1 }));
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 2, arrival: 0, bursts: vec![1, 0, 1001], level: 0 }], &[1], 0), Err(Error::Burst { index: 1, position: 2 }));
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, bursts: vec![4294967295], level: 0 }], &[1], 0), Err(Error::Burst { index: 0, position: 0 }));
}

// R2, R7, R10-R11
#[test]
fn earlier_row_error_precedes_later_duplicate() {
    assert_eq!(schedule(&[Job { id: 9, arrival: 0, bursts: vec![0], level: 0 }, Job { id: 9, arrival: 0, bursts: vec![1], level: 0 }], &[1], 0), Err(Error::Burst { index: 0, position: 0 }));
    assert_eq!(schedule(&[Job { id: 9, arrival: 0, bursts: vec![1, 1], level: 0 }, Job { id: 9, arrival: 1001, bursts: vec![1], level: 0 }], &[1], 0), Err(Error::BurstCount { index: 0 }));
}

// R3-R6, R33, R38-R39
#[test]
fn empty_eight_level_means() {
    assert_eq!(schedule(&[], &[1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000], 1000), Ok(Schedule {
        runs: vec![],
        idle: vec![],
        stats: vec![],
        mean_waiting_x100: 0, mean_turnaround_x100: 0,
        end: 0
    }));
}

// R20-R21, R24, R34-R38
#[test]
fn one_exact_cpu_run() {
    assert_eq!(schedule(&[Job { id: 98, arrival: 3, bursts: vec![4], level: 1 }], &[2, 4], 7), Ok(Schedule {
        runs: vec![Run { id: 98, start: 3, end: 7, finished: true, level: 1 }],
        idle: vec![(0, 3)],
        stats: vec![Stat { id: 98, completion: 7, turnaround: 4, waiting: 0 }],
        mean_waiting_x100: 0, mean_turnaround_x100: 400,
        end: 7
    }));
}

// R18, R20-R24, R30
#[test]
fn same_id_after_demotion_no_cost() {
    assert_eq!(schedule(&[Job { id: 12, arrival: 0, bursts: vec![10], level: 0 }], &[2, 3], 1000), Ok(Schedule {
        runs: vec![Run { id: 12, start: 0, end: 2, finished: false, level: 0 }, Run { id: 12, start: 2, end: 5, finished: false, level: 1 }, Run { id: 12, start: 5, end: 8, finished: false, level: 1 }, Run { id: 12, start: 8, end: 10, finished: true, level: 1 }],
        idle: vec![],
        stats: vec![Stat { id: 12, completion: 10, turnaround: 10, waiting: 0 }],
        mean_waiting_x100: 0, mean_turnaround_x100: 1000,
        end: 10
    }));
}

// R25-R26, R31-R32, R35-R36
#[test]
fn idle_clears_history_and_excludes_blocked() {
    assert_eq!(schedule(&[Job { id: 31, arrival: 0, bursts: vec![1, 4, 1], level: 0 }, Job { id: 32, arrival: 12, bursts: vec![1], level: 0 }], &[3, 5], 9), Ok(Schedule {
        runs: vec![Run { id: 31, start: 0, end: 1, finished: false, level: 0 }, Run { id: 31, start: 5, end: 6, finished: true, level: 0 }, Run { id: 32, start: 12, end: 13, finished: true, level: 0 }],
        idle: vec![(1, 5), (6, 12)],
        stats: vec![Stat { id: 31, completion: 6, turnaround: 6, waiting: 0 }, Stat { id: 32, completion: 13, turnaround: 1, waiting: 0 }],
        mean_waiting_x100: 0, mean_turnaround_x100: 350,
        end: 13
    }));
}

// R14, R18-R21, R34-R37
#[test]
fn fifo_paid_switch_stats() {
    assert_eq!(schedule(&[Job { id: 45, arrival: 0, bursts: vec![3], level: 0 }, Job { id: 15, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 25, arrival: 0, bursts: vec![2], level: 0 }], &[2], 2), Ok(Schedule {
        runs: vec![Run { id: 45, start: 0, end: 2, finished: false, level: 0 }, Run { id: 15, start: 4, end: 5, finished: true, level: 0 }, Run { id: 25, start: 7, end: 9, finished: true, level: 0 }, Run { id: 45, start: 11, end: 12, finished: true, level: 0 }],
        idle: vec![],
        stats: vec![Stat { id: 45, completion: 12, turnaround: 12, waiting: 9 }, Stat { id: 15, completion: 5, turnaround: 5, waiting: 4 }, Stat { id: 25, completion: 9, turnaround: 9, waiting: 7 }],
        mean_waiting_x100: 667, mean_turnaround_x100: 867,
        end: 12
    }));
}

// R20, R23, R30
#[test]
fn demotion_multiple_levels() {
    assert_eq!(schedule(&[Job { id: 75, arrival: 0, bursts: vec![15], level: 0 }], &[1, 3, 2], 3), Ok(Schedule {
        runs: vec![Run { id: 75, start: 0, end: 1, finished: false, level: 0 }, Run { id: 75, start: 1, end: 4, finished: false, level: 1 }, Run { id: 75, start: 4, end: 6, finished: false, level: 2 }, Run { id: 75, start: 6, end: 8, finished: false, level: 2 }, Run { id: 75, start: 8, end: 10, finished: false, level: 2 }, Run { id: 75, start: 10, end: 12, finished: false, level: 2 }, Run { id: 75, start: 12, end: 14, finished: false, level: 2 }, Run { id: 75, start: 14, end: 15, finished: true, level: 2 }],
        idle: vec![],
        stats: vec![Stat { id: 75, completion: 15, turnaround: 15, waiting: 0 }],
        mean_waiting_x100: 0, mean_turnaround_x100: 1500,
        end: 15
    }));
}

// R14-R15, R23, R29
#[test]
fn bottom_level_fifo_persists() {
    assert_eq!(schedule(&[Job { id: 6, arrival: 0, bursts: vec![7], level: 1 }, Job { id: 5, arrival: 0, bursts: vec![5], level: 1 }], &[1, 2], 1), Ok(Schedule {
        runs: vec![Run { id: 6, start: 0, end: 2, finished: false, level: 1 }, Run { id: 5, start: 3, end: 5, finished: false, level: 1 }, Run { id: 6, start: 6, end: 8, finished: false, level: 1 }, Run { id: 5, start: 9, end: 11, finished: false, level: 1 }, Run { id: 6, start: 12, end: 14, finished: false, level: 1 }, Run { id: 5, start: 15, end: 16, finished: true, level: 1 }, Run { id: 6, start: 17, end: 18, finished: true, level: 1 }],
        idle: vec![],
        stats: vec![Stat { id: 6, completion: 18, turnaround: 18, waiting: 11 }, Stat { id: 5, completion: 16, turnaround: 16, waiting: 11 }],
        mean_waiting_x100: 1100, mean_turnaround_x100: 1700,
        end: 18
    }));
}

// R13-R16, R19
#[test]
fn higher_arrival_never_preempts() {
    assert_eq!(schedule(&[Job { id: 19, arrival: 0, bursts: vec![6], level: 1 }, Job { id: 18, arrival: 2, bursts: vec![1], level: 0 }], &[1, 5], 1), Ok(Schedule {
        runs: vec![Run { id: 19, start: 0, end: 5, finished: false, level: 1 }, Run { id: 18, start: 6, end: 7, finished: true, level: 0 }, Run { id: 19, start: 8, end: 9, finished: true, level: 1 }],
        idle: vec![],
        stats: vec![Stat { id: 19, completion: 9, turnaround: 9, waiting: 3 }, Stat { id: 18, completion: 7, turnaround: 5, waiting: 4 }],
        mean_waiting_x100: 350, mean_turnaround_x100: 700,
        end: 9
    }));
}

// R16-R20
#[test]
fn cost_selection_locked_against_priority_arrival() {
    assert_eq!(schedule(&[Job { id: 43, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 44, arrival: 0, bursts: vec![4], level: 1 }, Job { id: 45, arrival: 3, bursts: vec![1], level: 0 }], &[2, 4], 5), Ok(Schedule {
        runs: vec![Run { id: 43, start: 0, end: 1, finished: true, level: 0 }, Run { id: 44, start: 6, end: 10, finished: true, level: 1 }, Run { id: 45, start: 15, end: 16, finished: true, level: 0 }],
        idle: vec![],
        stats: vec![Stat { id: 43, completion: 1, turnaround: 1, waiting: 0 }, Stat { id: 44, completion: 10, turnaround: 10, waiting: 6 }, Stat { id: 45, completion: 16, turnaround: 13, waiting: 12 }],
        mean_waiting_x100: 600, mean_turnaround_x100: 800,
        end: 16
    }));
}

// R12, R25-R27, R30
#[test]
fn zero_io_promotes_immediately() {
    assert_eq!(schedule(&[Job { id: 21, arrival: 0, bursts: vec![1, 0, 1, 0, 1], level: 2 }], &[3, 4, 5], 9), Ok(Schedule {
        runs: vec![Run { id: 21, start: 0, end: 1, finished: false, level: 2 }, Run { id: 21, start: 1, end: 2, finished: false, level: 1 }, Run { id: 21, start: 2, end: 3, finished: true, level: 0 }],
        idle: vec![],
        stats: vec![Stat { id: 21, completion: 3, turnaround: 3, waiting: 0 }],
        mean_waiting_x100: 0, mean_turnaround_x100: 300,
        end: 3
    }));
}

// R20, R23-R27
#[test]
fn exact_burst_beats_demotion() {
    assert_eq!(schedule(&[Job { id: 7, arrival: 0, bursts: vec![3, 0, 1], level: 1 }], &[2, 3, 5], 4), Ok(Schedule {
        runs: vec![Run { id: 7, start: 0, end: 3, finished: false, level: 1 }, Run { id: 7, start: 3, end: 4, finished: true, level: 0 }],
        idle: vec![],
        stats: vec![Stat { id: 7, completion: 4, turnaround: 4, waiting: 0 }],
        mean_waiting_x100: 0, mean_turnaround_x100: 400,
        end: 4
    }));
}

// R22, R27-R29
#[test]
fn external_arrival_before_zero_wake() {
    assert_eq!(schedule(&[Job { id: 50, arrival: 0, bursts: vec![2, 0, 1], level: 0 }, Job { id: 40, arrival: 2, bursts: vec![1], level: 0 }], &[2], 1), Ok(Schedule {
        runs: vec![Run { id: 50, start: 0, end: 2, finished: false, level: 0 }, Run { id: 40, start: 3, end: 4, finished: true, level: 0 }, Run { id: 50, start: 5, end: 6, finished: true, level: 0 }],
        idle: vec![],
        stats: vec![Stat { id: 50, completion: 6, turnaround: 6, waiting: 3 }, Stat { id: 40, completion: 4, turnaround: 2, waiting: 1 }],
        mean_waiting_x100: 200, mean_turnaround_x100: 400,
        end: 6
    }));
}

// R25-R29
#[test]
fn simultaneous_wakes_use_input_not_block_order() {
    assert_eq!(schedule(&[Job { id: 90, arrival: 0, bursts: vec![1, 3, 1], level: 1 }, Job { id: 10, arrival: 0, bursts: vec![1, 4, 1], level: 0 }], &[2, 2], 0), Ok(Schedule {
        runs: vec![Run { id: 10, start: 0, end: 1, finished: false, level: 0 }, Run { id: 90, start: 1, end: 2, finished: false, level: 1 }, Run { id: 90, start: 5, end: 6, finished: true, level: 0 }, Run { id: 10, start: 6, end: 7, finished: true, level: 0 }],
        idle: vec![(2, 5)],
        stats: vec![Stat { id: 90, completion: 6, turnaround: 6, waiting: 1 }, Stat { id: 10, completion: 7, turnaround: 7, waiting: 1 }],
        mean_waiting_x100: 100, mean_turnaround_x100: 650,
        end: 7
    }));
}

// R15, R19, R22, R25-R26
#[test]
fn wake_during_cpu_nonpreemptive() {
    assert_eq!(schedule(&[Job { id: 14, arrival: 0, bursts: vec![1, 2, 1], level: 0 }, Job { id: 15, arrival: 0, bursts: vec![6], level: 1 }], &[2, 5], 0), Ok(Schedule {
        runs: vec![Run { id: 14, start: 0, end: 1, finished: false, level: 0 }, Run { id: 15, start: 1, end: 6, finished: false, level: 1 }, Run { id: 14, start: 6, end: 7, finished: true, level: 0 }, Run { id: 15, start: 7, end: 8, finished: true, level: 1 }],
        idle: vec![],
        stats: vec![Stat { id: 14, completion: 7, turnaround: 7, waiting: 3 }, Stat { id: 15, completion: 8, turnaround: 8, waiting: 2 }],
        mean_waiting_x100: 250, mean_turnaround_x100: 750,
        end: 8
    }));
}

// R17-R19, R25-R29
#[test]
fn wake_during_locked_cost() {
    assert_eq!(schedule(&[Job { id: 64, arrival: 0, bursts: vec![1, 2, 1], level: 0 }, Job { id: 65, arrival: 0, bursts: vec![2], level: 1 }], &[2, 4], 5), Ok(Schedule {
        runs: vec![Run { id: 64, start: 0, end: 1, finished: false, level: 0 }, Run { id: 65, start: 6, end: 8, finished: true, level: 1 }, Run { id: 64, start: 13, end: 14, finished: true, level: 0 }],
        idle: vec![],
        stats: vec![Stat { id: 64, completion: 14, turnaround: 14, waiting: 10 }, Stat { id: 65, completion: 8, turnaround: 8, waiting: 6 }],
        mean_waiting_x100: 800, mean_turnaround_x100: 1100,
        end: 14
    }));
}

// R20, R23-R27, R30
#[test]
fn new_quantum_after_short_burst() {
    assert_eq!(schedule(&[Job { id: 38, arrival: 0, bursts: vec![1, 0, 5], level: 1 }], &[3, 4], 2), Ok(Schedule {
        runs: vec![Run { id: 38, start: 0, end: 1, finished: false, level: 1 }, Run { id: 38, start: 1, end: 4, finished: false, level: 0 }, Run { id: 38, start: 4, end: 6, finished: true, level: 1 }],
        idle: vec![],
        stats: vec![Stat { id: 38, completion: 6, turnaround: 6, waiting: 0 }],
        mean_waiting_x100: 0, mean_turnaround_x100: 600,
        end: 6
    }));
}

// R25, R31-R32, R34-R36
#[test]
fn blocked_time_excluded_from_wait() {
    assert_eq!(schedule(&[Job { id: 87, arrival: 4, bursts: vec![1, 8, 2], level: 0 }, Job { id: 86, arrival: 5, bursts: vec![4], level: 0 }], &[2, 3], 1), Ok(Schedule {
        runs: vec![Run { id: 87, start: 4, end: 5, finished: false, level: 0 }, Run { id: 86, start: 6, end: 8, finished: false, level: 0 }, Run { id: 86, start: 8, end: 10, finished: true, level: 1 }, Run { id: 87, start: 13, end: 15, finished: true, level: 0 }],
        idle: vec![(0, 4), (10, 13)],
        stats: vec![Stat { id: 87, completion: 15, turnaround: 11, waiting: 0 }, Stat { id: 86, completion: 10, turnaround: 5, waiting: 1 }],
        mean_waiting_x100: 50, mean_turnaround_x100: 800,
        end: 15
    }));
}

// R34-R38
#[test]
fn round_exact_half_up_eight_jobs() {
    assert_eq!(schedule(&[Job { id: 100, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 101, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 102, arrival: 4, bursts: vec![1], level: 0 }, Job { id: 103, arrival: 7, bursts: vec![1], level: 0 }, Job { id: 104, arrival: 10, bursts: vec![1], level: 0 }, Job { id: 105, arrival: 13, bursts: vec![1], level: 0 }, Job { id: 106, arrival: 16, bursts: vec![1], level: 0 }, Job { id: 107, arrival: 19, bursts: vec![1], level: 0 }], &[3], 0), Ok(Schedule {
        runs: vec![Run { id: 100, start: 0, end: 1, finished: true, level: 0 }, Run { id: 101, start: 1, end: 2, finished: true, level: 0 }, Run { id: 102, start: 4, end: 5, finished: true, level: 0 }, Run { id: 103, start: 7, end: 8, finished: true, level: 0 }, Run { id: 104, start: 10, end: 11, finished: true, level: 0 }, Run { id: 105, start: 13, end: 14, finished: true, level: 0 }, Run { id: 106, start: 16, end: 17, finished: true, level: 0 }, Run { id: 107, start: 19, end: 20, finished: true, level: 0 }],
        idle: vec![(2, 4), (5, 7), (8, 10), (11, 13), (14, 16), (17, 19)],
        stats: vec![Stat { id: 100, completion: 1, turnaround: 1, waiting: 0 }, Stat { id: 101, completion: 2, turnaround: 2, waiting: 1 }, Stat { id: 102, completion: 5, turnaround: 1, waiting: 0 }, Stat { id: 103, completion: 8, turnaround: 1, waiting: 0 }, Stat { id: 104, completion: 11, turnaround: 1, waiting: 0 }, Stat { id: 105, completion: 14, turnaround: 1, waiting: 0 }, Stat { id: 106, completion: 17, turnaround: 1, waiting: 0 }, Stat { id: 107, completion: 20, turnaround: 1, waiting: 0 }],
        mean_waiting_x100: 13, mean_turnaround_x100: 113,
        end: 20
    }));
}

// R34-R38
#[test]
fn round_thirds_down() {
    assert_eq!(schedule(&[Job { id: 71, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 72, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 73, arrival: 5, bursts: vec![1], level: 0 }], &[2], 0), Ok(Schedule {
        runs: vec![Run { id: 71, start: 0, end: 1, finished: true, level: 0 }, Run { id: 72, start: 1, end: 2, finished: true, level: 0 }, Run { id: 73, start: 5, end: 6, finished: true, level: 0 }],
        idle: vec![(2, 5)],
        stats: vec![Stat { id: 71, completion: 1, turnaround: 1, waiting: 0 }, Stat { id: 72, completion: 2, turnaround: 2, waiting: 1 }, Stat { id: 73, completion: 6, turnaround: 1, waiting: 0 }],
        mean_waiting_x100: 33, mean_turnaround_x100: 133,
        end: 6
    }));
}

// R20-R30, R34-R39
#[test]
fn many_bursts_and_demotions() {
    assert_eq!(schedule(&[Job { id: 13, arrival: 0, bursts: vec![5, 0, 2, 3, 7], level: 1 }, Job { id: 12, arrival: 1, bursts: vec![3, 1, 2], level: 0 }, Job { id: 11, arrival: 4, bursts: vec![4, 0, 1], level: 2 }], &[2, 3, 4], 2), Ok(Schedule {
        runs: vec![Run { id: 13, start: 0, end: 3, finished: false, level: 1 }, Run { id: 12, start: 5, end: 7, finished: false, level: 0 }, Run { id: 12, start: 7, end: 8, finished: false, level: 1 }, Run { id: 13, start: 10, end: 12, finished: false, level: 2 }, Run { id: 12, start: 14, end: 16, finished: true, level: 0 }, Run { id: 13, start: 18, end: 20, finished: false, level: 1 }, Run { id: 11, start: 22, end: 26, finished: false, level: 2 }, Run { id: 13, start: 28, end: 30, finished: false, level: 0 }, Run { id: 11, start: 32, end: 33, finished: true, level: 1 }, Run { id: 13, start: 35, end: 38, finished: false, level: 1 }, Run { id: 13, start: 38, end: 40, finished: true, level: 2 }],
        idle: vec![],
        stats: vec![Stat { id: 13, completion: 40, turnaround: 40, waiting: 23 }, Stat { id: 12, completion: 16, turnaround: 15, waiting: 9 }, Stat { id: 11, completion: 33, turnaround: 29, waiting: 24 }],
        mean_waiting_x100: 1867, mean_turnaround_x100: 2800,
        end: 40
    }));
}

// R1, R13-R16, R28, R34
#[test]
fn extreme_ids_unsorted_events_and_completion() {
    assert_eq!(schedule(&[Job { id: 0, arrival: 7, bursts: vec![1], level: 1 }, Job { id: 4294967295, arrival: 0, bursts: vec![4, 2, 1], level: 0 }, Job { id: 99, arrival: 2, bursts: vec![1], level: 0 }], &[2, 3], 1), Ok(Schedule {
        runs: vec![Run { id: 4294967295, start: 0, end: 2, finished: false, level: 0 }, Run { id: 99, start: 3, end: 4, finished: true, level: 0 }, Run { id: 4294967295, start: 5, end: 7, finished: false, level: 1 }, Run { id: 0, start: 8, end: 9, finished: true, level: 1 }, Run { id: 4294967295, start: 10, end: 11, finished: true, level: 0 }],
        idle: vec![],
        stats: vec![Stat { id: 0, completion: 9, turnaround: 2, waiting: 1 }, Stat { id: 4294967295, completion: 11, turnaround: 11, waiting: 4 }, Stat { id: 99, completion: 4, turnaround: 2, waiting: 1 }],
        mean_waiting_x100: 200, mean_turnaround_x100: 500,
        end: 11
    }));
}

// R3-R6, R8-R12, R20, R25, R39
#[test]
fn all_bounds_including_long_burst_vector() {
    assert_eq!(schedule(&[Job { id: 3, arrival: 1000, bursts: vec![1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000], level: 7 }, Job { id: 4, arrival: 1000, bursts: vec![1], level: 7 }], &[1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000], 1000), Ok(Schedule {
        runs: vec![Run { id: 3, start: 1000, end: 2000, finished: false, level: 7 }, Run { id: 4, start: 3000, end: 3001, finished: true, level: 7 }, Run { id: 3, start: 4001, end: 5001, finished: false, level: 6 }, Run { id: 3, start: 6001, end: 7001, finished: false, level: 5 }, Run { id: 3, start: 8001, end: 9001, finished: false, level: 4 }, Run { id: 3, start: 10001, end: 11001, finished: false, level: 3 }, Run { id: 3, start: 12001, end: 13001, finished: false, level: 2 }, Run { id: 3, start: 14001, end: 15001, finished: false, level: 1 }, Run { id: 3, start: 16001, end: 17001, finished: false, level: 0 }, Run { id: 3, start: 18001, end: 19001, finished: false, level: 0 }, Run { id: 3, start: 20001, end: 21001, finished: false, level: 0 }, Run { id: 3, start: 22001, end: 23001, finished: false, level: 0 }, Run { id: 3, start: 24001, end: 25001, finished: false, level: 0 }, Run { id: 3, start: 26001, end: 27001, finished: false, level: 0 }, Run { id: 3, start: 28001, end: 29001, finished: false, level: 0 }, Run { id: 3, start: 30001, end: 31001, finished: false, level: 0 }, Run { id: 3, start: 32001, end: 33001, finished: true, level: 0 }],
        idle: vec![(0, 1000), (5001, 6001), (7001, 8001), (9001, 10001), (11001, 12001), (13001, 14001), (15001, 16001), (17001, 18001), (19001, 20001), (21001, 22001), (23001, 24001), (25001, 26001), (27001, 28001), (29001, 30001), (31001, 32001)],
        stats: vec![Stat { id: 3, completion: 33001, turnaround: 32001, waiting: 1001 }, Stat { id: 4, completion: 3001, turnaround: 2001, waiting: 2000 }],
        mean_waiting_x100: 150050, mean_turnaround_x100: 1700100,
        end: 33001
    }));
}

// R6, R13-R15, R34, R38-R39
#[test]
fn full_capacity_initial_levels() {
    assert_eq!(schedule(&[Job { id: 100, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 101, arrival: 1, bursts: vec![1], level: 1 }, Job { id: 102, arrival: 2, bursts: vec![1], level: 2 }, Job { id: 103, arrival: 3, bursts: vec![1], level: 3 }, Job { id: 104, arrival: 0, bursts: vec![1], level: 4 }, Job { id: 105, arrival: 1, bursts: vec![1], level: 5 }, Job { id: 106, arrival: 2, bursts: vec![1], level: 6 }, Job { id: 107, arrival: 3, bursts: vec![1], level: 7 }, Job { id: 108, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 109, arrival: 1, bursts: vec![1], level: 1 }, Job { id: 110, arrival: 2, bursts: vec![1], level: 2 }, Job { id: 111, arrival: 3, bursts: vec![1], level: 3 }, Job { id: 112, arrival: 0, bursts: vec![1], level: 4 }, Job { id: 113, arrival: 1, bursts: vec![1], level: 5 }, Job { id: 114, arrival: 2, bursts: vec![1], level: 6 }, Job { id: 115, arrival: 3, bursts: vec![1], level: 7 }, Job { id: 116, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 117, arrival: 1, bursts: vec![1], level: 1 }, Job { id: 118, arrival: 2, bursts: vec![1], level: 2 }, Job { id: 119, arrival: 3, bursts: vec![1], level: 3 }, Job { id: 120, arrival: 0, bursts: vec![1], level: 4 }, Job { id: 121, arrival: 1, bursts: vec![1], level: 5 }, Job { id: 122, arrival: 2, bursts: vec![1], level: 6 }, Job { id: 123, arrival: 3, bursts: vec![1], level: 7 }, Job { id: 124, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 125, arrival: 1, bursts: vec![1], level: 1 }, Job { id: 126, arrival: 2, bursts: vec![1], level: 2 }, Job { id: 127, arrival: 3, bursts: vec![1], level: 3 }, Job { id: 128, arrival: 0, bursts: vec![1], level: 4 }, Job { id: 129, arrival: 1, bursts: vec![1], level: 5 }, Job { id: 130, arrival: 2, bursts: vec![1], level: 6 }, Job { id: 131, arrival: 3, bursts: vec![1], level: 7 }, Job { id: 132, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 133, arrival: 1, bursts: vec![1], level: 1 }, Job { id: 134, arrival: 2, bursts: vec![1], level: 2 }, Job { id: 135, arrival: 3, bursts: vec![1], level: 3 }, Job { id: 136, arrival: 0, bursts: vec![1], level: 4 }, Job { id: 137, arrival: 1, bursts: vec![1], level: 5 }, Job { id: 138, arrival: 2, bursts: vec![1], level: 6 }, Job { id: 139, arrival: 3, bursts: vec![1], level: 7 }, Job { id: 140, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 141, arrival: 1, bursts: vec![1], level: 1 }, Job { id: 142, arrival: 2, bursts: vec![1], level: 2 }, Job { id: 143, arrival: 3, bursts: vec![1], level: 3 }, Job { id: 144, arrival: 0, bursts: vec![1], level: 4 }, Job { id: 145, arrival: 1, bursts: vec![1], level: 5 }, Job { id: 146, arrival: 2, bursts: vec![1], level: 6 }, Job { id: 147, arrival: 3, bursts: vec![1], level: 7 }, Job { id: 148, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 149, arrival: 1, bursts: vec![1], level: 1 }, Job { id: 150, arrival: 2, bursts: vec![1], level: 2 }, Job { id: 151, arrival: 3, bursts: vec![1], level: 3 }, Job { id: 152, arrival: 0, bursts: vec![1], level: 4 }, Job { id: 153, arrival: 1, bursts: vec![1], level: 5 }, Job { id: 154, arrival: 2, bursts: vec![1], level: 6 }, Job { id: 155, arrival: 3, bursts: vec![1], level: 7 }, Job { id: 156, arrival: 0, bursts: vec![1], level: 0 }, Job { id: 157, arrival: 1, bursts: vec![1], level: 1 }, Job { id: 158, arrival: 2, bursts: vec![1], level: 2 }, Job { id: 159, arrival: 3, bursts: vec![1], level: 3 }, Job { id: 160, arrival: 0, bursts: vec![1], level: 4 }, Job { id: 161, arrival: 1, bursts: vec![1], level: 5 }, Job { id: 162, arrival: 2, bursts: vec![1], level: 6 }, Job { id: 163, arrival: 3, bursts: vec![1], level: 7 }], &[2, 2, 2, 2, 2, 2, 2, 2], 0), Ok(Schedule {
        runs: vec![Run { id: 100, start: 0, end: 1, finished: true, level: 0 }, Run { id: 108, start: 1, end: 2, finished: true, level: 0 }, Run { id: 116, start: 2, end: 3, finished: true, level: 0 }, Run { id: 124, start: 3, end: 4, finished: true, level: 0 }, Run { id: 132, start: 4, end: 5, finished: true, level: 0 }, Run { id: 140, start: 5, end: 6, finished: true, level: 0 }, Run { id: 148, start: 6, end: 7, finished: true, level: 0 }, Run { id: 156, start: 7, end: 8, finished: true, level: 0 }, Run { id: 101, start: 8, end: 9, finished: true, level: 1 }, Run { id: 109, start: 9, end: 10, finished: true, level: 1 }, Run { id: 117, start: 10, end: 11, finished: true, level: 1 }, Run { id: 125, start: 11, end: 12, finished: true, level: 1 }, Run { id: 133, start: 12, end: 13, finished: true, level: 1 }, Run { id: 141, start: 13, end: 14, finished: true, level: 1 }, Run { id: 149, start: 14, end: 15, finished: true, level: 1 }, Run { id: 157, start: 15, end: 16, finished: true, level: 1 }, Run { id: 102, start: 16, end: 17, finished: true, level: 2 }, Run { id: 110, start: 17, end: 18, finished: true, level: 2 }, Run { id: 118, start: 18, end: 19, finished: true, level: 2 }, Run { id: 126, start: 19, end: 20, finished: true, level: 2 }, Run { id: 134, start: 20, end: 21, finished: true, level: 2 }, Run { id: 142, start: 21, end: 22, finished: true, level: 2 }, Run { id: 150, start: 22, end: 23, finished: true, level: 2 }, Run { id: 158, start: 23, end: 24, finished: true, level: 2 }, Run { id: 103, start: 24, end: 25, finished: true, level: 3 }, Run { id: 111, start: 25, end: 26, finished: true, level: 3 }, Run { id: 119, start: 26, end: 27, finished: true, level: 3 }, Run { id: 127, start: 27, end: 28, finished: true, level: 3 }, Run { id: 135, start: 28, end: 29, finished: true, level: 3 }, Run { id: 143, start: 29, end: 30, finished: true, level: 3 }, Run { id: 151, start: 30, end: 31, finished: true, level: 3 }, Run { id: 159, start: 31, end: 32, finished: true, level: 3 }, Run { id: 104, start: 32, end: 33, finished: true, level: 4 }, Run { id: 112, start: 33, end: 34, finished: true, level: 4 }, Run { id: 120, start: 34, end: 35, finished: true, level: 4 }, Run { id: 128, start: 35, end: 36, finished: true, level: 4 }, Run { id: 136, start: 36, end: 37, finished: true, level: 4 }, Run { id: 144, start: 37, end: 38, finished: true, level: 4 }, Run { id: 152, start: 38, end: 39, finished: true, level: 4 }, Run { id: 160, start: 39, end: 40, finished: true, level: 4 }, Run { id: 105, start: 40, end: 41, finished: true, level: 5 }, Run { id: 113, start: 41, end: 42, finished: true, level: 5 }, Run { id: 121, start: 42, end: 43, finished: true, level: 5 }, Run { id: 129, start: 43, end: 44, finished: true, level: 5 }, Run { id: 137, start: 44, end: 45, finished: true, level: 5 }, Run { id: 145, start: 45, end: 46, finished: true, level: 5 }, Run { id: 153, start: 46, end: 47, finished: true, level: 5 }, Run { id: 161, start: 47, end: 48, finished: true, level: 5 }, Run { id: 106, start: 48, end: 49, finished: true, level: 6 }, Run { id: 114, start: 49, end: 50, finished: true, level: 6 }, Run { id: 122, start: 50, end: 51, finished: true, level: 6 }, Run { id: 130, start: 51, end: 52, finished: true, level: 6 }, Run { id: 138, start: 52, end: 53, finished: true, level: 6 }, Run { id: 146, start: 53, end: 54, finished: true, level: 6 }, Run { id: 154, start: 54, end: 55, finished: true, level: 6 }, Run { id: 162, start: 55, end: 56, finished: true, level: 6 }, Run { id: 107, start: 56, end: 57, finished: true, level: 7 }, Run { id: 115, start: 57, end: 58, finished: true, level: 7 }, Run { id: 123, start: 58, end: 59, finished: true, level: 7 }, Run { id: 131, start: 59, end: 60, finished: true, level: 7 }, Run { id: 139, start: 60, end: 61, finished: true, level: 7 }, Run { id: 147, start: 61, end: 62, finished: true, level: 7 }, Run { id: 155, start: 62, end: 63, finished: true, level: 7 }, Run { id: 163, start: 63, end: 64, finished: true, level: 7 }],
        idle: vec![],
        stats: vec![Stat { id: 100, completion: 1, turnaround: 1, waiting: 0 }, Stat { id: 101, completion: 9, turnaround: 8, waiting: 7 }, Stat { id: 102, completion: 17, turnaround: 15, waiting: 14 }, Stat { id: 103, completion: 25, turnaround: 22, waiting: 21 }, Stat { id: 104, completion: 33, turnaround: 33, waiting: 32 }, Stat { id: 105, completion: 41, turnaround: 40, waiting: 39 }, Stat { id: 106, completion: 49, turnaround: 47, waiting: 46 }, Stat { id: 107, completion: 57, turnaround: 54, waiting: 53 }, Stat { id: 108, completion: 2, turnaround: 2, waiting: 1 }, Stat { id: 109, completion: 10, turnaround: 9, waiting: 8 }, Stat { id: 110, completion: 18, turnaround: 16, waiting: 15 }, Stat { id: 111, completion: 26, turnaround: 23, waiting: 22 }, Stat { id: 112, completion: 34, turnaround: 34, waiting: 33 }, Stat { id: 113, completion: 42, turnaround: 41, waiting: 40 }, Stat { id: 114, completion: 50, turnaround: 48, waiting: 47 }, Stat { id: 115, completion: 58, turnaround: 55, waiting: 54 }, Stat { id: 116, completion: 3, turnaround: 3, waiting: 2 }, Stat { id: 117, completion: 11, turnaround: 10, waiting: 9 }, Stat { id: 118, completion: 19, turnaround: 17, waiting: 16 }, Stat { id: 119, completion: 27, turnaround: 24, waiting: 23 }, Stat { id: 120, completion: 35, turnaround: 35, waiting: 34 }, Stat { id: 121, completion: 43, turnaround: 42, waiting: 41 }, Stat { id: 122, completion: 51, turnaround: 49, waiting: 48 }, Stat { id: 123, completion: 59, turnaround: 56, waiting: 55 }, Stat { id: 124, completion: 4, turnaround: 4, waiting: 3 }, Stat { id: 125, completion: 12, turnaround: 11, waiting: 10 }, Stat { id: 126, completion: 20, turnaround: 18, waiting: 17 }, Stat { id: 127, completion: 28, turnaround: 25, waiting: 24 }, Stat { id: 128, completion: 36, turnaround: 36, waiting: 35 }, Stat { id: 129, completion: 44, turnaround: 43, waiting: 42 }, Stat { id: 130, completion: 52, turnaround: 50, waiting: 49 }, Stat { id: 131, completion: 60, turnaround: 57, waiting: 56 }, Stat { id: 132, completion: 5, turnaround: 5, waiting: 4 }, Stat { id: 133, completion: 13, turnaround: 12, waiting: 11 }, Stat { id: 134, completion: 21, turnaround: 19, waiting: 18 }, Stat { id: 135, completion: 29, turnaround: 26, waiting: 25 }, Stat { id: 136, completion: 37, turnaround: 37, waiting: 36 }, Stat { id: 137, completion: 45, turnaround: 44, waiting: 43 }, Stat { id: 138, completion: 53, turnaround: 51, waiting: 50 }, Stat { id: 139, completion: 61, turnaround: 58, waiting: 57 }, Stat { id: 140, completion: 6, turnaround: 6, waiting: 5 }, Stat { id: 141, completion: 14, turnaround: 13, waiting: 12 }, Stat { id: 142, completion: 22, turnaround: 20, waiting: 19 }, Stat { id: 143, completion: 30, turnaround: 27, waiting: 26 }, Stat { id: 144, completion: 38, turnaround: 38, waiting: 37 }, Stat { id: 145, completion: 46, turnaround: 45, waiting: 44 }, Stat { id: 146, completion: 54, turnaround: 52, waiting: 51 }, Stat { id: 147, completion: 62, turnaround: 59, waiting: 58 }, Stat { id: 148, completion: 7, turnaround: 7, waiting: 6 }, Stat { id: 149, completion: 15, turnaround: 14, waiting: 13 }, Stat { id: 150, completion: 23, turnaround: 21, waiting: 20 }, Stat { id: 151, completion: 31, turnaround: 28, waiting: 27 }, Stat { id: 152, completion: 39, turnaround: 39, waiting: 38 }, Stat { id: 153, completion: 47, turnaround: 46, waiting: 45 }, Stat { id: 154, completion: 55, turnaround: 53, waiting: 52 }, Stat { id: 155, completion: 63, turnaround: 60, waiting: 59 }, Stat { id: 156, completion: 8, turnaround: 8, waiting: 7 }, Stat { id: 157, completion: 16, turnaround: 15, waiting: 14 }, Stat { id: 158, completion: 24, turnaround: 22, waiting: 21 }, Stat { id: 159, completion: 32, turnaround: 29, waiting: 28 }, Stat { id: 160, completion: 40, turnaround: 40, waiting: 39 }, Stat { id: 161, completion: 48, turnaround: 47, waiting: 46 }, Stat { id: 162, completion: 56, turnaround: 54, waiting: 53 }, Stat { id: 163, completion: 64, turnaround: 61, waiting: 60 }],
        mean_waiting_x100: 3000, mean_turnaround_x100: 3100,
        end: 64
    }));
}

// R1; repeated calls use only their arguments.
#[test]
fn independent_calls_preserve_input() {
    let jobs = vec![Job { id: 27, arrival: 2, bursts: vec![2, 0, 3], level: 0 }, Job { id: 8, arrival: 0, bursts: vec![1, 3, 1], level: 0 }];
    let original = jobs.clone();
    let first = schedule(&jobs, &[2, 4], 2).unwrap();
    let _ = schedule(&[], &[2, 4], 1).unwrap();
    assert_eq!(schedule(&jobs, &[2, 4], 2).unwrap(), first);
    assert_eq!(jobs, original);
}
