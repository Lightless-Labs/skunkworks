// R2-R3
#[test]
fn level_count_before_all_other_checks() {
    assert_eq!(schedule(&[], &[], 1001), Err(Error::LevelCount));
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }], &[1, 1, 1, 1, 1, 1, 1, 1, 1], 0), Err(Error::LevelCount));
}

// R2, R4-R5
#[test]
fn first_bad_quantum_before_cost() {
    assert_eq!(schedule(&[], &[2, 1001, 0], 1001), Err(Error::Quantum { level: 1 }));
    assert_eq!(schedule(&[], &[0], 0), Err(Error::Quantum { level: 0 }));
}

// R5-R6
#[test]
fn cost_before_count() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }], &[1], 1001), Err(Error::SwitchCost));
}

// R6-R7
#[test]
fn count_before_rows() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }, Job { id: 1, arrival: 0, ticks: 1, level: 0 }], &[1], 0), Err(Error::TooManyJobs));
}

// R7-R10
#[test]
fn duplicate_before_other_row_fields() {
    assert_eq!(schedule(&[Job { id: 9, arrival: 0, ticks: 1, level: 0 }, Job { id: 8, arrival: 0, ticks: 1, level: 0 }, Job { id: 9, arrival: 1001, ticks: 1001, level: 99 }], &[1], 0), Err(Error::DuplicateId { index: 2 }));
}

// R8-R10
#[test]
fn row_field_precedence() {
    assert_eq!(schedule(&[Job { id: 6, arrival: 1001, ticks: 1001, level: 9 }], &[1], 0), Err(Error::Arrival { index: 0 }));
    assert_eq!(schedule(&[Job { id: 6, arrival: 0, ticks: 1001, level: 1 }], &[1], 0), Err(Error::InitialLevel { index: 0 }));
}

// R2, R7, R10
#[test]
fn early_ticks_before_later_duplicate() {
    assert_eq!(schedule(&[Job { id: 7, arrival: 0, ticks: 1001, level: 0 }, Job { id: 7, arrival: 0, ticks: 1, level: 0 }], &[1], 0), Err(Error::Ticks { index: 0 }));
    assert_eq!(schedule(&[Job { id: 6, arrival: 0, ticks: 1, level: 0 }, Job { id: 7, arrival: 0, ticks: 4294967295, level: 0 }], &[1], 0), Err(Error::Ticks { index: 1 }));
}

// R3-R6, R27, R29
#[test]
fn empty_eight_levels() {
    assert_eq!(schedule(&[], &[1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000], 1000), Ok(Schedule {
        runs: vec![],
        idle: vec![],
        completion: vec![],
        end: 0
    }));
}

// R11-R12, R25-R28
#[test]
fn zero_jobs_complete_at_arrival() {
    assert_eq!(schedule(&[Job { id: 8, arrival: 2, ticks: 0, level: 1 }, Job { id: 7, arrival: 5, ticks: 0, level: 0 }], &[2, 3], 8), Ok(Schedule {
        runs: vec![],
        idle: vec![(0, 5)],
        completion: vec![(8, 2), (7, 5)],
        end: 5
    }));
}

// R12, R16-R18
#[test]
fn zero_during_run_and_cost() {
    assert_eq!(schedule(&[Job { id: 10, arrival: 0, ticks: 3, level: 0 }, Job { id: 11, arrival: 0, ticks: 1, level: 0 }, Job { id: 12, arrival: 3, ticks: 0, level: 1 }, Job { id: 13, arrival: 6, ticks: 0, level: 0 }], &[4, 8], 5), Ok(Schedule {
        runs: vec![Run { id: 10, start: 0, end: 3, finished: true, level: 0 }, Run { id: 11, start: 8, end: 9, finished: true, level: 0 }],
        idle: vec![],
        completion: vec![(10, 3), (11, 9), (12, 3), (13, 6)],
        end: 9
    }));
}

// R4, R19, R22
#[test]
fn nonmonotone_quanta_are_allowed() {
    assert_eq!(schedule(&[Job { id: 34, arrival: 0, ticks: 10, level: 0 }], &[4, 1, 2], 3), Ok(Schedule {
        runs: vec![Run { id: 34, start: 0, end: 4, finished: false, level: 0 }, Run { id: 34, start: 4, end: 5, finished: false, level: 1 }, Run { id: 34, start: 5, end: 7, finished: false, level: 2 }, Run { id: 34, start: 7, end: 9, finished: false, level: 2 }, Run { id: 34, start: 9, end: 10, finished: true, level: 2 }],
        idle: vec![],
        completion: vec![(34, 10)],
        end: 10
    }));
}

// R19-R20, R22-R24
#[test]
fn demote_through_all_levels() {
    assert_eq!(schedule(&[Job { id: 31, arrival: 0, ticks: 14, level: 0 }], &[1, 2, 3, 4], 1000), Ok(Schedule {
        runs: vec![Run { id: 31, start: 0, end: 1, finished: false, level: 0 }, Run { id: 31, start: 1, end: 3, finished: false, level: 1 }, Run { id: 31, start: 3, end: 6, finished: false, level: 2 }, Run { id: 31, start: 6, end: 10, finished: false, level: 3 }, Run { id: 31, start: 10, end: 14, finished: true, level: 3 }],
        idle: vec![],
        completion: vec![(31, 14)],
        end: 14
    }));
}

// R13-R14, R22
#[test]
fn bottom_level_round_robin() {
    assert_eq!(schedule(&[Job { id: 82, arrival: 0, ticks: 5, level: 1 }, Job { id: 18, arrival: 0, ticks: 6, level: 1 }], &[1, 2], 1), Ok(Schedule {
        runs: vec![Run { id: 82, start: 0, end: 2, finished: false, level: 1 }, Run { id: 18, start: 3, end: 5, finished: false, level: 1 }, Run { id: 82, start: 6, end: 8, finished: false, level: 1 }, Run { id: 18, start: 9, end: 11, finished: false, level: 1 }, Run { id: 82, start: 12, end: 13, finished: true, level: 1 }, Run { id: 18, start: 14, end: 16, finished: true, level: 1 }],
        idle: vec![],
        completion: vec![(82, 13), (18, 16)],
        end: 16
    }));
}

// R9, R13-R14
#[test]
fn highest_priority_wins() {
    assert_eq!(schedule(&[Job { id: 7, arrival: 0, ticks: 3, level: 2 }, Job { id: 6, arrival: 0, ticks: 2, level: 1 }, Job { id: 9, arrival: 0, ticks: 1, level: 0 }], &[1, 2, 3], 0), Ok(Schedule {
        runs: vec![Run { id: 9, start: 0, end: 1, finished: true, level: 0 }, Run { id: 6, start: 1, end: 3, finished: true, level: 1 }, Run { id: 7, start: 3, end: 6, finished: true, level: 2 }],
        idle: vec![],
        completion: vec![(7, 6), (6, 3), (9, 1)],
        end: 6
    }));
}

// R14, R18-R19
#[test]
fn higher_arrival_does_not_preempt() {
    assert_eq!(schedule(&[Job { id: 64, arrival: 0, ticks: 7, level: 1 }, Job { id: 65, arrival: 2, ticks: 1, level: 0 }], &[1, 5], 1), Ok(Schedule {
        runs: vec![Run { id: 64, start: 0, end: 5, finished: false, level: 1 }, Run { id: 65, start: 6, end: 7, finished: true, level: 0 }, Run { id: 64, start: 8, end: 10, finished: true, level: 1 }],
        idle: vec![],
        completion: vec![(64, 10), (65, 7)],
        end: 10
    }));
}

// R21-R22
#[test]
fn arrival_at_end_before_demotion() {
    assert_eq!(schedule(&[Job { id: 40, arrival: 0, ticks: 6, level: 0 }, Job { id: 41, arrival: 2, ticks: 1, level: 1 }], &[2, 4], 0), Ok(Schedule {
        runs: vec![Run { id: 40, start: 0, end: 2, finished: false, level: 0 }, Run { id: 41, start: 2, end: 3, finished: true, level: 1 }, Run { id: 40, start: 3, end: 7, finished: true, level: 1 }],
        idle: vec![],
        completion: vec![(40, 7), (41, 3)],
        end: 7
    }));
}

// R13, R21-R22
#[test]
fn existing_tail_before_demoted_job() {
    assert_eq!(schedule(&[Job { id: 40, arrival: 0, ticks: 6, level: 0 }, Job { id: 41, arrival: 0, ticks: 1, level: 1 }, Job { id: 42, arrival: 2, ticks: 1, level: 1 }], &[2, 4], 0), Ok(Schedule {
        runs: vec![Run { id: 40, start: 0, end: 2, finished: false, level: 0 }, Run { id: 41, start: 2, end: 3, finished: true, level: 1 }, Run { id: 42, start: 3, end: 4, finished: true, level: 1 }, Run { id: 40, start: 4, end: 8, finished: true, level: 1 }],
        idle: vec![],
        completion: vec![(40, 8), (41, 3), (42, 4)],
        end: 8
    }));
}

// R14, R16-R19
#[test]
fn locked_choice_during_cost() {
    assert_eq!(schedule(&[Job { id: 70, arrival: 0, ticks: 1, level: 0 }, Job { id: 71, arrival: 0, ticks: 4, level: 1 }, Job { id: 72, arrival: 3, ticks: 1, level: 0 }], &[2, 4], 5), Ok(Schedule {
        runs: vec![Run { id: 70, start: 0, end: 1, finished: true, level: 0 }, Run { id: 71, start: 6, end: 10, finished: true, level: 1 }, Run { id: 72, start: 15, end: 16, finished: true, level: 0 }],
        idle: vec![],
        completion: vec![(70, 1), (71, 10), (72, 16)],
        end: 16
    }));
}

// R17, R25-R26
#[test]
fn idle_resets_paid_switch() {
    assert_eq!(schedule(&[Job { id: 4, arrival: 0, ticks: 1, level: 1 }, Job { id: 5, arrival: 10, ticks: 1, level: 0 }], &[2, 3], 7), Ok(Schedule {
        runs: vec![Run { id: 4, start: 0, end: 1, finished: true, level: 1 }, Run { id: 5, start: 10, end: 11, finished: true, level: 0 }],
        idle: vec![(1, 10)],
        completion: vec![(4, 1), (5, 11)],
        end: 11
    }));
}

// R12, R25-R27
#[test]
fn idle_merge_zero_markers() {
    assert_eq!(schedule(&[Job { id: 51, arrival: 2, ticks: 0, level: 0 }, Job { id: 52, arrival: 4, ticks: 0, level: 1 }, Job { id: 53, arrival: 9, ticks: 1, level: 1 }], &[2, 3], 6), Ok(Schedule {
        runs: vec![Run { id: 53, start: 9, end: 10, finished: true, level: 1 }],
        idle: vec![(0, 9)],
        completion: vec![(51, 2), (52, 4), (53, 10)],
        end: 10
    }));
}

// R12, R24
#[test]
fn zero_marker_keeps_dispatch_id() {
    assert_eq!(schedule(&[Job { id: 21, arrival: 0, ticks: 9, level: 0 }, Job { id: 22, arrival: 2, ticks: 0, level: 1 }], &[2, 3], 9), Ok(Schedule {
        runs: vec![Run { id: 21, start: 0, end: 2, finished: false, level: 0 }, Run { id: 21, start: 2, end: 5, finished: false, level: 1 }, Run { id: 21, start: 5, end: 8, finished: false, level: 1 }, Run { id: 21, start: 8, end: 9, finished: true, level: 1 }],
        idle: vec![],
        completion: vec![(21, 9), (22, 2)],
        end: 9
    }));
}

// R20, R23
#[test]
fn finish_exactly_at_level_quantum() {
    assert_eq!(schedule(&[Job { id: 81, arrival: 0, ticks: 6, level: 0 }], &[2, 4], 6), Ok(Schedule {
        runs: vec![Run { id: 81, start: 0, end: 2, finished: false, level: 0 }, Run { id: 81, start: 2, end: 6, finished: true, level: 1 }],
        idle: vec![],
        completion: vec![(81, 6)],
        end: 6
    }));
}

// R1, R11, R28
#[test]
fn input_order_results_with_extreme_ids() {
    assert_eq!(schedule(&[Job { id: 4294967295, arrival: 5, ticks: 1, level: 1 }, Job { id: 0, arrival: 0, ticks: 6, level: 0 }, Job { id: 3, arrival: 0, ticks: 1, level: 1 }], &[2, 4], 1), Ok(Schedule {
        runs: vec![Run { id: 0, start: 0, end: 2, finished: false, level: 0 }, Run { id: 3, start: 3, end: 4, finished: true, level: 1 }, Run { id: 0, start: 5, end: 9, finished: true, level: 1 }, Run { id: 4294967295, start: 10, end: 11, finished: true, level: 1 }],
        idle: vec![],
        completion: vec![(4294967295, 11), (0, 9), (3, 4)],
        end: 11
    }));
}

// R9, R19-R20
#[test]
fn initial_level_uses_own_quantum() {
    assert_eq!(schedule(&[Job { id: 17, arrival: 0, ticks: 7, level: 2 }], &[1, 2, 5], 0), Ok(Schedule {
        runs: vec![Run { id: 17, start: 0, end: 5, finished: false, level: 2 }, Run { id: 17, start: 5, end: 7, finished: true, level: 2 }],
        idle: vec![],
        completion: vec![(17, 7)],
        end: 7
    }));
}

// R3, R13, R22
#[test]
fn single_level_degenerates_to_fifo() {
    assert_eq!(schedule(&[Job { id: 11, arrival: 0, ticks: 4, level: 0 }, Job { id: 12, arrival: 0, ticks: 3, level: 0 }, Job { id: 13, arrival: 1, ticks: 1, level: 0 }], &[2], 1), Ok(Schedule {
        runs: vec![Run { id: 11, start: 0, end: 2, finished: false, level: 0 }, Run { id: 12, start: 3, end: 5, finished: false, level: 0 }, Run { id: 13, start: 6, end: 7, finished: true, level: 0 }, Run { id: 11, start: 8, end: 10, finished: true, level: 0 }, Run { id: 12, start: 11, end: 12, finished: true, level: 0 }],
        idle: vec![],
        completion: vec![(11, 10), (12, 12), (13, 7)],
        end: 12
    }));
}

// R4-R5, R8-R10, R29
#[test]
fn max_arrival_and_service() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 1000, ticks: 1000, level: 7 }, Job { id: 2, arrival: 1000, ticks: 1000, level: 7 }], &[1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000], 1000), Ok(Schedule {
        runs: vec![Run { id: 1, start: 1000, end: 2000, finished: true, level: 7 }, Run { id: 2, start: 3000, end: 4000, finished: true, level: 7 }],
        idle: vec![(0, 1000)],
        completion: vec![(1, 2000), (2, 4000)],
        end: 4000
    }));
}

// R6, R11, R13-R14, R28
#[test]
fn sixtyfour_initial_levels() {
    assert_eq!(schedule(&[Job { id: 100, arrival: 0, ticks: 1, level: 0 }, Job { id: 101, arrival: 0, ticks: 1, level: 1 }, Job { id: 102, arrival: 0, ticks: 1, level: 2 }, Job { id: 103, arrival: 0, ticks: 1, level: 3 }, Job { id: 104, arrival: 0, ticks: 1, level: 4 }, Job { id: 105, arrival: 0, ticks: 1, level: 5 }, Job { id: 106, arrival: 0, ticks: 1, level: 6 }, Job { id: 107, arrival: 0, ticks: 1, level: 7 }, Job { id: 108, arrival: 0, ticks: 1, level: 0 }, Job { id: 109, arrival: 0, ticks: 1, level: 1 }, Job { id: 110, arrival: 0, ticks: 1, level: 2 }, Job { id: 111, arrival: 0, ticks: 1, level: 3 }, Job { id: 112, arrival: 0, ticks: 1, level: 4 }, Job { id: 113, arrival: 0, ticks: 1, level: 5 }, Job { id: 114, arrival: 0, ticks: 1, level: 6 }, Job { id: 115, arrival: 0, ticks: 1, level: 7 }, Job { id: 116, arrival: 0, ticks: 1, level: 0 }, Job { id: 117, arrival: 0, ticks: 1, level: 1 }, Job { id: 118, arrival: 0, ticks: 1, level: 2 }, Job { id: 119, arrival: 0, ticks: 1, level: 3 }, Job { id: 120, arrival: 0, ticks: 1, level: 4 }, Job { id: 121, arrival: 0, ticks: 1, level: 5 }, Job { id: 122, arrival: 0, ticks: 1, level: 6 }, Job { id: 123, arrival: 0, ticks: 1, level: 7 }, Job { id: 124, arrival: 0, ticks: 1, level: 0 }, Job { id: 125, arrival: 0, ticks: 1, level: 1 }, Job { id: 126, arrival: 0, ticks: 1, level: 2 }, Job { id: 127, arrival: 0, ticks: 1, level: 3 }, Job { id: 128, arrival: 0, ticks: 1, level: 4 }, Job { id: 129, arrival: 0, ticks: 1, level: 5 }, Job { id: 130, arrival: 0, ticks: 1, level: 6 }, Job { id: 131, arrival: 0, ticks: 1, level: 7 }, Job { id: 132, arrival: 0, ticks: 1, level: 0 }, Job { id: 133, arrival: 0, ticks: 1, level: 1 }, Job { id: 134, arrival: 0, ticks: 1, level: 2 }, Job { id: 135, arrival: 0, ticks: 1, level: 3 }, Job { id: 136, arrival: 0, ticks: 1, level: 4 }, Job { id: 137, arrival: 0, ticks: 1, level: 5 }, Job { id: 138, arrival: 0, ticks: 1, level: 6 }, Job { id: 139, arrival: 0, ticks: 1, level: 7 }, Job { id: 140, arrival: 0, ticks: 1, level: 0 }, Job { id: 141, arrival: 0, ticks: 1, level: 1 }, Job { id: 142, arrival: 0, ticks: 1, level: 2 }, Job { id: 143, arrival: 0, ticks: 1, level: 3 }, Job { id: 144, arrival: 0, ticks: 1, level: 4 }, Job { id: 145, arrival: 0, ticks: 1, level: 5 }, Job { id: 146, arrival: 0, ticks: 1, level: 6 }, Job { id: 147, arrival: 0, ticks: 1, level: 7 }, Job { id: 148, arrival: 0, ticks: 1, level: 0 }, Job { id: 149, arrival: 0, ticks: 1, level: 1 }, Job { id: 150, arrival: 0, ticks: 1, level: 2 }, Job { id: 151, arrival: 0, ticks: 1, level: 3 }, Job { id: 152, arrival: 0, ticks: 1, level: 4 }, Job { id: 153, arrival: 0, ticks: 1, level: 5 }, Job { id: 154, arrival: 0, ticks: 1, level: 6 }, Job { id: 155, arrival: 0, ticks: 1, level: 7 }, Job { id: 156, arrival: 0, ticks: 1, level: 0 }, Job { id: 157, arrival: 0, ticks: 1, level: 1 }, Job { id: 158, arrival: 0, ticks: 1, level: 2 }, Job { id: 159, arrival: 0, ticks: 1, level: 3 }, Job { id: 160, arrival: 0, ticks: 1, level: 4 }, Job { id: 161, arrival: 0, ticks: 1, level: 5 }, Job { id: 162, arrival: 0, ticks: 1, level: 6 }, Job { id: 163, arrival: 0, ticks: 1, level: 7 }], &[2, 2, 2, 2, 2, 2, 2, 2], 0), Ok(Schedule {
        runs: vec![Run { id: 100, start: 0, end: 1, finished: true, level: 0 }, Run { id: 108, start: 1, end: 2, finished: true, level: 0 }, Run { id: 116, start: 2, end: 3, finished: true, level: 0 }, Run { id: 124, start: 3, end: 4, finished: true, level: 0 }, Run { id: 132, start: 4, end: 5, finished: true, level: 0 }, Run { id: 140, start: 5, end: 6, finished: true, level: 0 }, Run { id: 148, start: 6, end: 7, finished: true, level: 0 }, Run { id: 156, start: 7, end: 8, finished: true, level: 0 }, Run { id: 101, start: 8, end: 9, finished: true, level: 1 }, Run { id: 109, start: 9, end: 10, finished: true, level: 1 }, Run { id: 117, start: 10, end: 11, finished: true, level: 1 }, Run { id: 125, start: 11, end: 12, finished: true, level: 1 }, Run { id: 133, start: 12, end: 13, finished: true, level: 1 }, Run { id: 141, start: 13, end: 14, finished: true, level: 1 }, Run { id: 149, start: 14, end: 15, finished: true, level: 1 }, Run { id: 157, start: 15, end: 16, finished: true, level: 1 }, Run { id: 102, start: 16, end: 17, finished: true, level: 2 }, Run { id: 110, start: 17, end: 18, finished: true, level: 2 }, Run { id: 118, start: 18, end: 19, finished: true, level: 2 }, Run { id: 126, start: 19, end: 20, finished: true, level: 2 }, Run { id: 134, start: 20, end: 21, finished: true, level: 2 }, Run { id: 142, start: 21, end: 22, finished: true, level: 2 }, Run { id: 150, start: 22, end: 23, finished: true, level: 2 }, Run { id: 158, start: 23, end: 24, finished: true, level: 2 }, Run { id: 103, start: 24, end: 25, finished: true, level: 3 }, Run { id: 111, start: 25, end: 26, finished: true, level: 3 }, Run { id: 119, start: 26, end: 27, finished: true, level: 3 }, Run { id: 127, start: 27, end: 28, finished: true, level: 3 }, Run { id: 135, start: 28, end: 29, finished: true, level: 3 }, Run { id: 143, start: 29, end: 30, finished: true, level: 3 }, Run { id: 151, start: 30, end: 31, finished: true, level: 3 }, Run { id: 159, start: 31, end: 32, finished: true, level: 3 }, Run { id: 104, start: 32, end: 33, finished: true, level: 4 }, Run { id: 112, start: 33, end: 34, finished: true, level: 4 }, Run { id: 120, start: 34, end: 35, finished: true, level: 4 }, Run { id: 128, start: 35, end: 36, finished: true, level: 4 }, Run { id: 136, start: 36, end: 37, finished: true, level: 4 }, Run { id: 144, start: 37, end: 38, finished: true, level: 4 }, Run { id: 152, start: 38, end: 39, finished: true, level: 4 }, Run { id: 160, start: 39, end: 40, finished: true, level: 4 }, Run { id: 105, start: 40, end: 41, finished: true, level: 5 }, Run { id: 113, start: 41, end: 42, finished: true, level: 5 }, Run { id: 121, start: 42, end: 43, finished: true, level: 5 }, Run { id: 129, start: 43, end: 44, finished: true, level: 5 }, Run { id: 137, start: 44, end: 45, finished: true, level: 5 }, Run { id: 145, start: 45, end: 46, finished: true, level: 5 }, Run { id: 153, start: 46, end: 47, finished: true, level: 5 }, Run { id: 161, start: 47, end: 48, finished: true, level: 5 }, Run { id: 106, start: 48, end: 49, finished: true, level: 6 }, Run { id: 114, start: 49, end: 50, finished: true, level: 6 }, Run { id: 122, start: 50, end: 51, finished: true, level: 6 }, Run { id: 130, start: 51, end: 52, finished: true, level: 6 }, Run { id: 138, start: 52, end: 53, finished: true, level: 6 }, Run { id: 146, start: 53, end: 54, finished: true, level: 6 }, Run { id: 154, start: 54, end: 55, finished: true, level: 6 }, Run { id: 162, start: 55, end: 56, finished: true, level: 6 }, Run { id: 107, start: 56, end: 57, finished: true, level: 7 }, Run { id: 115, start: 57, end: 58, finished: true, level: 7 }, Run { id: 123, start: 58, end: 59, finished: true, level: 7 }, Run { id: 131, start: 59, end: 60, finished: true, level: 7 }, Run { id: 139, start: 60, end: 61, finished: true, level: 7 }, Run { id: 147, start: 61, end: 62, finished: true, level: 7 }, Run { id: 155, start: 62, end: 63, finished: true, level: 7 }, Run { id: 163, start: 63, end: 64, finished: true, level: 7 }],
        idle: vec![],
        completion: vec![(100, 1), (101, 9), (102, 17), (103, 25), (104, 33), (105, 41), (106, 49), (107, 57), (108, 2), (109, 10), (110, 18), (111, 26), (112, 34), (113, 42), (114, 50), (115, 58), (116, 3), (117, 11), (118, 19), (119, 27), (120, 35), (121, 43), (122, 51), (123, 59), (124, 4), (125, 12), (126, 20), (127, 28), (128, 36), (129, 44), (130, 52), (131, 60), (132, 5), (133, 13), (134, 21), (135, 29), (136, 37), (137, 45), (138, 53), (139, 61), (140, 6), (141, 14), (142, 22), (143, 30), (144, 38), (145, 46), (146, 54), (147, 62), (148, 7), (149, 15), (150, 23), (151, 31), (152, 39), (153, 47), (154, 55), (155, 63), (156, 8), (157, 16), (158, 24), (159, 32), (160, 40), (161, 48), (162, 56), (163, 64)],
        end: 64
    }));
}

// R11-R12, R15, R21
#[test]
fn unsorted_events_and_zero_endpoint() {
    assert_eq!(schedule(&[Job { id: 54, arrival: 4, ticks: 2, level: 0 }, Job { id: 55, arrival: 0, ticks: 6, level: 1 }, Job { id: 56, arrival: 3, ticks: 0, level: 0 }, Job { id: 57, arrival: 3, ticks: 1, level: 0 }], &[2, 3], 1), Ok(Schedule {
        runs: vec![Run { id: 55, start: 0, end: 3, finished: false, level: 1 }, Run { id: 57, start: 4, end: 5, finished: true, level: 0 }, Run { id: 54, start: 6, end: 8, finished: true, level: 0 }, Run { id: 55, start: 9, end: 12, finished: true, level: 1 }],
        idle: vec![],
        completion: vec![(54, 8), (55, 12), (56, 3), (57, 5)],
        end: 12
    }));
}

// R1; repeated calls use only their arguments.
#[test]
fn independent_calls_preserve_input() {
    let jobs = vec![Job { id: 27, arrival: 2, ticks: 5, level: 0 }, Job { id: 8, arrival: 0, ticks: 2, level: 0 }];
    let original = jobs.clone();
    let first = schedule(&jobs, &[2, 4], 2).unwrap();
    let _ = schedule(&[], &[2, 4], 1).unwrap();
    assert_eq!(schedule(&jobs, &[2, 4], 2).unwrap(), first);
    assert_eq!(jobs, original);
}
