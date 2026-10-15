// R2-R5
#[test]
fn quantum_boundaries_and_precedence() {
    assert_eq!(schedule(&[], 1001, 1001), Err(Error::Quantum));
    assert_eq!(schedule(&[Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }], 0, 1001), Err(Error::Quantum));
}

// R2, R4
#[test]
fn invalid_dispatch_cost() {
    assert_eq!(schedule(&[], 1, 1001), Err(Error::SwitchCost));
}

// R5-R7
#[test]
fn count_precedes_rows() {
    assert_eq!(schedule(&[Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }, Job { id: 1, ticks: 1001 }], 1, 0), Err(Error::TooManyJobs));
}

// R6
#[test]
fn first_duplicate_index() {
    assert_eq!(schedule(&[Job { id: 7, ticks: 1 }, Job { id: 8, ticks: 1 }, Job { id: 7, ticks: 1 }, Job { id: 8, ticks: 1 }], 1, 0), Err(Error::DuplicateId { index: 2 }));
}

// R6-R7
#[test]
fn duplicate_beats_ticks_on_same_row() {
    assert_eq!(schedule(&[Job { id: 3, ticks: 1 }, Job { id: 3, ticks: 1001 }], 1, 0), Err(Error::DuplicateId { index: 1 }));
}

// R2, R7
#[test]
fn early_ticks_beats_later_duplicate() {
    assert_eq!(schedule(&[Job { id: 3, ticks: 1001 }, Job { id: 3, ticks: 1 }], 1, 0), Err(Error::Ticks { index: 0 }));
}

// R7
#[test]
fn nonzero_ticks_error_index() {
    assert_eq!(schedule(&[Job { id: 0, ticks: 1 }, Job { id: 2, ticks: 0 }, Job { id: 9, ticks: 4294967295 }], 1, 0), Err(Error::Ticks { index: 2 }));
}

// R3-R5, R20-R21
#[test]
fn no_jobs_max_parameters() {
    assert_eq!(schedule(&[], 1000, 1000), Ok(Schedule {
        runs: vec![],
        completion: vec![],
        end: 0
    }));
}

// R11, R18-R21
#[test]
fn only_zero_work() {
    assert_eq!(schedule(&[Job { id: 88, ticks: 0 }, Job { id: 0, ticks: 0 }, Job { id: 4294967295, ticks: 0 }], 1, 1000), Ok(Schedule {
        runs: vec![],
        completion: vec![(88, 0), (0, 0), (4294967295, 0)],
        end: 0
    }));
}

// R8, R11, R18
#[test]
fn zero_finishes_at_head_time() {
    assert_eq!(schedule(&[Job { id: 4, ticks: 3 }, Job { id: 5, ticks: 0 }, Job { id: 6, ticks: 2 }], 2, 1), Ok(Schedule {
        runs: vec![Run { id: 4, start: 0, end: 2, finished: false }, Run { id: 6, start: 3, end: 5, finished: true }, Run { id: 4, start: 6, end: 7, finished: true }],
        completion: vec![(4, 7), (5, 2), (6, 5)],
        end: 7
    }));
}

// R11-R14
#[test]
fn zero_between_same_id_dispatches() {
    assert_eq!(schedule(&[Job { id: 92, ticks: 5 }, Job { id: 13, ticks: 0 }], 2, 7), Ok(Schedule {
        runs: vec![Run { id: 92, start: 0, end: 2, finished: false }, Run { id: 92, start: 2, end: 4, finished: false }, Run { id: 92, start: 4, end: 5, finished: true }],
        completion: vec![(92, 5), (13, 2)],
        end: 5
    }));
}

// R11-R13
#[test]
fn zero_before_first_run_is_free() {
    assert_eq!(schedule(&[Job { id: 8, ticks: 0 }, Job { id: 7, ticks: 2 }], 2, 999), Ok(Schedule {
        runs: vec![Run { id: 7, start: 0, end: 2, finished: true }],
        completion: vec![(8, 0), (7, 2)],
        end: 2
    }));
}

// R11, R15, R18-R20
#[test]
fn zero_after_last_positive() {
    assert_eq!(schedule(&[Job { id: 6, ticks: 1 }, Job { id: 9, ticks: 0 }], 5, 3), Ok(Schedule {
        runs: vec![Run { id: 6, start: 0, end: 1, finished: true }],
        completion: vec![(6, 1), (9, 1)],
        end: 1
    }));
}

// R9, R14, R16-R17
#[test]
fn single_multiple_runs_not_merged() {
    assert_eq!(schedule(&[Job { id: 41, ticks: 7 }], 3, 8), Ok(Schedule {
        runs: vec![Run { id: 41, start: 0, end: 3, finished: false }, Run { id: 41, start: 3, end: 6, finished: false }, Run { id: 41, start: 6, end: 7, finished: true }],
        completion: vec![(41, 7)],
        end: 7
    }));
}

// R9-R10, R17
#[test]
fn exact_quantum_finished_flag() {
    assert_eq!(schedule(&[Job { id: 77, ticks: 6 }], 3, 1000), Ok(Schedule {
        runs: vec![Run { id: 77, start: 0, end: 3, finished: false }, Run { id: 77, start: 3, end: 6, finished: true }],
        completion: vec![(77, 6)],
        end: 6
    }));
}

// R10, R13-R17
#[test]
fn uneven_tail_has_no_trailing_cost() {
    assert_eq!(schedule(&[Job { id: 99, ticks: 1 }, Job { id: 12, ticks: 7 }], 2, 4), Ok(Schedule {
        runs: vec![Run { id: 99, start: 0, end: 1, finished: true }, Run { id: 12, start: 5, end: 7, finished: false }, Run { id: 12, start: 7, end: 9, finished: false }, Run { id: 12, start: 9, end: 11, finished: false }, Run { id: 12, start: 11, end: 12, finished: true }],
        completion: vec![(99, 1), (12, 12)],
        end: 12
    }));
}

// R18-R19
#[test]
fn completion_is_input_order() {
    assert_eq!(schedule(&[Job { id: 81, ticks: 7 }, Job { id: 18, ticks: 1 }, Job { id: 72, ticks: 3 }], 2, 1), Ok(Schedule {
        runs: vec![Run { id: 81, start: 0, end: 2, finished: false }, Run { id: 18, start: 3, end: 4, finished: true }, Run { id: 72, start: 5, end: 7, finished: false }, Run { id: 81, start: 8, end: 10, finished: false }, Run { id: 72, start: 11, end: 12, finished: true }, Run { id: 81, start: 13, end: 15, finished: false }, Run { id: 81, start: 15, end: 16, finished: true }],
        completion: vec![(81, 16), (18, 4), (72, 12)],
        end: 16
    }));
}

// R1, R8-R10
#[test]
fn ids_do_not_sort_queue() {
    assert_eq!(schedule(&[Job { id: 4294967295, ticks: 2 }, Job { id: 0, ticks: 3 }, Job { id: 1, ticks: 1 }], 1, 2), Ok(Schedule {
        runs: vec![Run { id: 4294967295, start: 0, end: 1, finished: false }, Run { id: 0, start: 3, end: 4, finished: false }, Run { id: 1, start: 6, end: 7, finished: true }, Run { id: 4294967295, start: 9, end: 10, finished: true }, Run { id: 0, start: 12, end: 13, finished: false }, Run { id: 0, start: 13, end: 14, finished: true }],
        completion: vec![(4294967295, 10), (0, 14), (1, 7)],
        end: 14
    }));
}

// R3-R4, R7, R13, R21
#[test]
fn maximum_tick_quantum_and_cost() {
    assert_eq!(schedule(&[Job { id: 19, ticks: 1000 }, Job { id: 28, ticks: 1000 }], 1000, 1000), Ok(Schedule {
        runs: vec![Run { id: 19, start: 0, end: 1000, finished: true }, Run { id: 28, start: 2000, end: 3000, finished: true }],
        completion: vec![(19, 1000), (28, 3000)],
        end: 3000
    }));
}

// R9-R10, R16-R19
#[test]
fn unit_quantum_many_turns() {
    assert_eq!(schedule(&[Job { id: 31, ticks: 4 }, Job { id: 32, ticks: 6 }, Job { id: 33, ticks: 5 }], 1, 2), Ok(Schedule {
        runs: vec![Run { id: 31, start: 0, end: 1, finished: false }, Run { id: 32, start: 3, end: 4, finished: false }, Run { id: 33, start: 6, end: 7, finished: false }, Run { id: 31, start: 9, end: 10, finished: false }, Run { id: 32, start: 12, end: 13, finished: false }, Run { id: 33, start: 15, end: 16, finished: false }, Run { id: 31, start: 18, end: 19, finished: false }, Run { id: 32, start: 21, end: 22, finished: false }, Run { id: 33, start: 24, end: 25, finished: false }, Run { id: 31, start: 27, end: 28, finished: true }, Run { id: 32, start: 30, end: 31, finished: false }, Run { id: 33, start: 33, end: 34, finished: false }, Run { id: 32, start: 36, end: 37, finished: false }, Run { id: 33, start: 39, end: 40, finished: true }, Run { id: 32, start: 42, end: 43, finished: true }],
        completion: vec![(31, 28), (32, 43), (33, 40)],
        end: 43
    }));
}

// R12-R15
#[test]
fn threeway_switch_cost() {
    assert_eq!(schedule(&[Job { id: 7, ticks: 5 }, Job { id: 1, ticks: 4 }, Job { id: 9, ticks: 6 }], 3, 5), Ok(Schedule {
        runs: vec![Run { id: 7, start: 0, end: 3, finished: false }, Run { id: 1, start: 8, end: 11, finished: false }, Run { id: 9, start: 16, end: 19, finished: false }, Run { id: 7, start: 24, end: 26, finished: true }, Run { id: 1, start: 31, end: 32, finished: true }, Run { id: 9, start: 37, end: 40, finished: true }],
        completion: vec![(7, 26), (1, 32), (9, 40)],
        end: 40
    }));
}

// R8-R12
#[test]
fn fourway_fifo_with_zero_marker() {
    assert_eq!(schedule(&[Job { id: 70, ticks: 4 }, Job { id: 30, ticks: 1 }, Job { id: 20, ticks: 0 }, Job { id: 60, ticks: 3 }], 2, 2), Ok(Schedule {
        runs: vec![Run { id: 70, start: 0, end: 2, finished: false }, Run { id: 30, start: 4, end: 5, finished: true }, Run { id: 60, start: 7, end: 9, finished: false }, Run { id: 70, start: 11, end: 13, finished: true }, Run { id: 60, start: 15, end: 16, finished: true }],
        completion: vec![(70, 13), (30, 5), (20, 5), (60, 16)],
        end: 16
    }));
}

// R9, R13-R18
#[test]
fn large_quantum_short_jobs() {
    assert_eq!(schedule(&[Job { id: 45, ticks: 1 }, Job { id: 46, ticks: 2 }, Job { id: 47, ticks: 3 }], 1000, 9), Ok(Schedule {
        runs: vec![Run { id: 45, start: 0, end: 1, finished: true }, Run { id: 46, start: 10, end: 12, finished: true }, Run { id: 47, start: 21, end: 24, finished: true }],
        completion: vec![(45, 1), (46, 12), (47, 24)],
        end: 24
    }));
}

// R5, R8-R10, R19, R21
#[test]
fn sixtyfour_jobs_are_valid() {
    assert_eq!(schedule(&[Job { id: 0, ticks: 1 }, Job { id: 1, ticks: 1 }, Job { id: 2, ticks: 1 }, Job { id: 3, ticks: 1 }, Job { id: 4, ticks: 1 }, Job { id: 5, ticks: 1 }, Job { id: 6, ticks: 1 }, Job { id: 7, ticks: 1 }, Job { id: 8, ticks: 1 }, Job { id: 9, ticks: 1 }, Job { id: 10, ticks: 1 }, Job { id: 11, ticks: 1 }, Job { id: 12, ticks: 1 }, Job { id: 13, ticks: 1 }, Job { id: 14, ticks: 1 }, Job { id: 15, ticks: 1 }, Job { id: 16, ticks: 1 }, Job { id: 17, ticks: 1 }, Job { id: 18, ticks: 1 }, Job { id: 19, ticks: 1 }, Job { id: 20, ticks: 1 }, Job { id: 21, ticks: 1 }, Job { id: 22, ticks: 1 }, Job { id: 23, ticks: 1 }, Job { id: 24, ticks: 1 }, Job { id: 25, ticks: 1 }, Job { id: 26, ticks: 1 }, Job { id: 27, ticks: 1 }, Job { id: 28, ticks: 1 }, Job { id: 29, ticks: 1 }, Job { id: 30, ticks: 1 }, Job { id: 31, ticks: 1 }, Job { id: 32, ticks: 1 }, Job { id: 33, ticks: 1 }, Job { id: 34, ticks: 1 }, Job { id: 35, ticks: 1 }, Job { id: 36, ticks: 1 }, Job { id: 37, ticks: 1 }, Job { id: 38, ticks: 1 }, Job { id: 39, ticks: 1 }, Job { id: 40, ticks: 1 }, Job { id: 41, ticks: 1 }, Job { id: 42, ticks: 1 }, Job { id: 43, ticks: 1 }, Job { id: 44, ticks: 1 }, Job { id: 45, ticks: 1 }, Job { id: 46, ticks: 1 }, Job { id: 47, ticks: 1 }, Job { id: 48, ticks: 1 }, Job { id: 49, ticks: 1 }, Job { id: 50, ticks: 1 }, Job { id: 51, ticks: 1 }, Job { id: 52, ticks: 1 }, Job { id: 53, ticks: 1 }, Job { id: 54, ticks: 1 }, Job { id: 55, ticks: 1 }, Job { id: 56, ticks: 1 }, Job { id: 57, ticks: 1 }, Job { id: 58, ticks: 1 }, Job { id: 59, ticks: 1 }, Job { id: 60, ticks: 1 }, Job { id: 61, ticks: 1 }, Job { id: 62, ticks: 1 }, Job { id: 63, ticks: 1 }], 1, 1), Ok(Schedule {
        runs: vec![Run { id: 0, start: 0, end: 1, finished: true }, Run { id: 1, start: 2, end: 3, finished: true }, Run { id: 2, start: 4, end: 5, finished: true }, Run { id: 3, start: 6, end: 7, finished: true }, Run { id: 4, start: 8, end: 9, finished: true }, Run { id: 5, start: 10, end: 11, finished: true }, Run { id: 6, start: 12, end: 13, finished: true }, Run { id: 7, start: 14, end: 15, finished: true }, Run { id: 8, start: 16, end: 17, finished: true }, Run { id: 9, start: 18, end: 19, finished: true }, Run { id: 10, start: 20, end: 21, finished: true }, Run { id: 11, start: 22, end: 23, finished: true }, Run { id: 12, start: 24, end: 25, finished: true }, Run { id: 13, start: 26, end: 27, finished: true }, Run { id: 14, start: 28, end: 29, finished: true }, Run { id: 15, start: 30, end: 31, finished: true }, Run { id: 16, start: 32, end: 33, finished: true }, Run { id: 17, start: 34, end: 35, finished: true }, Run { id: 18, start: 36, end: 37, finished: true }, Run { id: 19, start: 38, end: 39, finished: true }, Run { id: 20, start: 40, end: 41, finished: true }, Run { id: 21, start: 42, end: 43, finished: true }, Run { id: 22, start: 44, end: 45, finished: true }, Run { id: 23, start: 46, end: 47, finished: true }, Run { id: 24, start: 48, end: 49, finished: true }, Run { id: 25, start: 50, end: 51, finished: true }, Run { id: 26, start: 52, end: 53, finished: true }, Run { id: 27, start: 54, end: 55, finished: true }, Run { id: 28, start: 56, end: 57, finished: true }, Run { id: 29, start: 58, end: 59, finished: true }, Run { id: 30, start: 60, end: 61, finished: true }, Run { id: 31, start: 62, end: 63, finished: true }, Run { id: 32, start: 64, end: 65, finished: true }, Run { id: 33, start: 66, end: 67, finished: true }, Run { id: 34, start: 68, end: 69, finished: true }, Run { id: 35, start: 70, end: 71, finished: true }, Run { id: 36, start: 72, end: 73, finished: true }, Run { id: 37, start: 74, end: 75, finished: true }, Run { id: 38, start: 76, end: 77, finished: true }, Run { id: 39, start: 78, end: 79, finished: true }, Run { id: 40, start: 80, end: 81, finished: true }, Run { id: 41, start: 82, end: 83, finished: true }, Run { id: 42, start: 84, end: 85, finished: true }, Run { id: 43, start: 86, end: 87, finished: true }, Run { id: 44, start: 88, end: 89, finished: true }, Run { id: 45, start: 90, end: 91, finished: true }, Run { id: 46, start: 92, end: 93, finished: true }, Run { id: 47, start: 94, end: 95, finished: true }, Run { id: 48, start: 96, end: 97, finished: true }, Run { id: 49, start: 98, end: 99, finished: true }, Run { id: 50, start: 100, end: 101, finished: true }, Run { id: 51, start: 102, end: 103, finished: true }, Run { id: 52, start: 104, end: 105, finished: true }, Run { id: 53, start: 106, end: 107, finished: true }, Run { id: 54, start: 108, end: 109, finished: true }, Run { id: 55, start: 110, end: 111, finished: true }, Run { id: 56, start: 112, end: 113, finished: true }, Run { id: 57, start: 114, end: 115, finished: true }, Run { id: 58, start: 116, end: 117, finished: true }, Run { id: 59, start: 118, end: 119, finished: true }, Run { id: 60, start: 120, end: 121, finished: true }, Run { id: 61, start: 122, end: 123, finished: true }, Run { id: 62, start: 124, end: 125, finished: true }, Run { id: 63, start: 126, end: 127, finished: true }],
        completion: vec![(0, 1), (1, 3), (2, 5), (3, 7), (4, 9), (5, 11), (6, 13), (7, 15), (8, 17), (9, 19), (10, 21), (11, 23), (12, 25), (13, 27), (14, 29), (15, 31), (16, 33), (17, 35), (18, 37), (19, 39), (20, 41), (21, 43), (22, 45), (23, 47), (24, 49), (25, 51), (26, 53), (27, 55), (28, 57), (29, 59), (30, 61), (31, 63), (32, 65), (33, 67), (34, 69), (35, 71), (36, 73), (37, 75), (38, 77), (39, 79), (40, 81), (41, 83), (42, 85), (43, 87), (44, 89), (45, 91), (46, 93), (47, 95), (48, 97), (49, 99), (50, 101), (51, 103), (52, 105), (53, 107), (54, 109), (55, 111), (56, 113), (57, 115), (58, 117), (59, 119), (60, 121), (61, 123), (62, 125), (63, 127)],
        end: 127
    }));
}

// R11-R13, R20
#[test]
fn all_markers_except_tail() {
    assert_eq!(schedule(&[Job { id: 4, ticks: 0 }, Job { id: 3, ticks: 0 }, Job { id: 2, ticks: 0 }, Job { id: 1, ticks: 1 }], 1, 1000), Ok(Schedule {
        runs: vec![Run { id: 1, start: 0, end: 1, finished: true }],
        completion: vec![(4, 0), (3, 0), (2, 0), (1, 1)],
        end: 1
    }));
}

// R13, R16
#[test]
fn zero_cost_still_separate_runs() {
    assert_eq!(schedule(&[Job { id: 111, ticks: 5 }, Job { id: 222, ticks: 1 }], 2, 0), Ok(Schedule {
        runs: vec![Run { id: 111, start: 0, end: 2, finished: false }, Run { id: 222, start: 2, end: 3, finished: true }, Run { id: 111, start: 3, end: 5, finished: false }, Run { id: 111, start: 5, end: 6, finished: true }],
        completion: vec![(111, 6), (222, 3)],
        end: 6
    }));
}

// R1; repeated calls use only their arguments.
#[test]
fn independent_calls_preserve_input() {
    let jobs = vec![Job { id: 27, ticks: 5 }, Job { id: 8, ticks: 2 }];
    let original = jobs.clone();
    let first = schedule(&jobs, 3, 2).unwrap();
    let _ = schedule(&[], 3, 1).unwrap();
    assert_eq!(schedule(&jobs, 3, 2).unwrap(), first);
    assert_eq!(jobs, original);
}
