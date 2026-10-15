// R2-R5
#[test]
fn parameter_error_precedence() {
    assert_eq!(schedule(&[], 0, 1001), Err(Error::Quantum));
    assert_eq!(schedule(&[], 1001, 0), Err(Error::Quantum));
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }, Job { id: 1, arrival: 0, ticks: 1001 }], 1, 1001), Err(Error::SwitchCost));
}

// R5-R6
#[test]
fn too_many_and_duplicate() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 1, arrival: 0, ticks: 1 }], 1, 0), Err(Error::TooManyJobs));
    assert_eq!(schedule(&[Job { id: 8, arrival: 0, ticks: 1 }, Job { id: 9, arrival: 0, ticks: 1 }, Job { id: 8, arrival: 1001, ticks: 1001 }], 1, 0), Err(Error::DuplicateId { index: 2 }));
}

// R7-R8
#[test]
fn arrival_beats_ticks() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 0, ticks: 1 }, Job { id: 2, arrival: 1001, ticks: 1001 }], 1, 0), Err(Error::Arrival { index: 1 }));
    assert_eq!(schedule(&[Job { id: 1, arrival: 4294967295, ticks: 1 }], 1, 0), Err(Error::Arrival { index: 0 }));
}

// R2, R6-R8
#[test]
fn row_order_beats_error_class() {
    assert_eq!(schedule(&[Job { id: 4, arrival: 0, ticks: 1001 }, Job { id: 4, arrival: 1001, ticks: 1 }], 1, 0), Err(Error::Ticks { index: 0 }));
    assert_eq!(schedule(&[Job { id: 2, arrival: 0, ticks: 1 }, Job { id: 3, arrival: 0, ticks: 4294967295 }], 1, 0), Err(Error::Ticks { index: 1 }));
}

// R3-R5, R22, R24
#[test]
fn empty_with_max_quantum() {
    assert_eq!(schedule(&[], 1000, 1000), Ok(Schedule {
        runs: vec![],
        idle: vec![],
        completion: vec![],
        end: 0
    }));
}

// R10, R20-R23
#[test]
fn zeros_coalesce_idle() {
    assert_eq!(schedule(&[Job { id: 4, arrival: 2, ticks: 0 }, Job { id: 8, arrival: 5, ticks: 0 }, Job { id: 1, arrival: 8, ticks: 0 }], 2, 4), Ok(Schedule {
        runs: vec![],
        idle: vec![(0, 8)],
        completion: vec![(4, 2), (8, 5), (1, 8)],
        end: 8
    }));
}

// R10, R12, R23
#[test]
fn zero_during_cpu_completes_at_event() {
    assert_eq!(schedule(&[Job { id: 9, arrival: 0, ticks: 9 }, Job { id: 8, arrival: 3, ticks: 0 }, Job { id: 7, arrival: 7, ticks: 0 }], 10, 3), Ok(Schedule {
        runs: vec![Run { id: 9, start: 0, end: 9, finished: true }],
        idle: vec![],
        completion: vec![(9, 9), (8, 3), (7, 7)],
        end: 9
    }));
}

// R10, R13-R15
#[test]
fn zero_during_cost_exact_timestamp() {
    assert_eq!(schedule(&[Job { id: 19, arrival: 0, ticks: 1 }, Job { id: 18, arrival: 0, ticks: 1 }, Job { id: 17, arrival: 3, ticks: 0 }], 2, 5), Ok(Schedule {
        runs: vec![Run { id: 19, start: 0, end: 1, finished: true }, Run { id: 18, start: 6, end: 7, finished: true }],
        idle: vec![],
        completion: vec![(19, 1), (18, 7), (17, 3)],
        end: 7
    }));
}

// R9, R11, R17-R18
#[test]
fn arrival_at_quantum_end_precedes_requeue() {
    assert_eq!(schedule(&[Job { id: 41, arrival: 0, ticks: 5 }, Job { id: 42, arrival: 2, ticks: 1 }], 2, 0), Ok(Schedule {
        runs: vec![Run { id: 41, start: 0, end: 2, finished: false }, Run { id: 42, start: 2, end: 3, finished: true }, Run { id: 41, start: 3, end: 5, finished: false }, Run { id: 41, start: 5, end: 6, finished: true }],
        idle: vec![],
        completion: vec![(41, 6), (42, 3)],
        end: 6
    }));
}

// R12, R15, R17-R18
#[test]
fn arrival_inside_slice_precedes_requeue() {
    assert_eq!(schedule(&[Job { id: 61, arrival: 0, ticks: 6 }, Job { id: 62, arrival: 1, ticks: 1 }], 3, 1), Ok(Schedule {
        runs: vec![Run { id: 61, start: 0, end: 3, finished: false }, Run { id: 62, start: 4, end: 5, finished: true }, Run { id: 61, start: 6, end: 9, finished: true }],
        idle: vec![],
        completion: vec![(61, 9), (62, 5)],
        end: 9
    }));
}

// R1, R9, R23
#[test]
fn unsorted_input_admits_by_time() {
    assert_eq!(schedule(&[Job { id: 7, arrival: 5, ticks: 2 }, Job { id: 99, arrival: 0, ticks: 4 }, Job { id: 0, arrival: 2, ticks: 1 }], 2, 0), Ok(Schedule {
        runs: vec![Run { id: 99, start: 0, end: 2, finished: false }, Run { id: 0, start: 2, end: 3, finished: true }, Run { id: 99, start: 3, end: 5, finished: true }, Run { id: 7, start: 5, end: 7, finished: true }],
        idle: vec![],
        completion: vec![(7, 7), (99, 5), (0, 3)],
        end: 7
    }));
}

// R9, R11
#[test]
fn equal_arrivals_use_input_not_id() {
    assert_eq!(schedule(&[Job { id: 4294967295, arrival: 3, ticks: 2 }, Job { id: 0, arrival: 3, ticks: 2 }, Job { id: 5, arrival: 3, ticks: 1 }], 1, 1), Ok(Schedule {
        runs: vec![Run { id: 4294967295, start: 3, end: 4, finished: false }, Run { id: 0, start: 5, end: 6, finished: false }, Run { id: 5, start: 7, end: 8, finished: true }, Run { id: 4294967295, start: 9, end: 10, finished: true }, Run { id: 0, start: 11, end: 12, finished: true }],
        idle: vec![(0, 3)],
        completion: vec![(4294967295, 10), (0, 12), (5, 8)],
        end: 12
    }));
}

// R14, R20-R21
#[test]
fn idle_clears_switch_history() {
    assert_eq!(schedule(&[Job { id: 81, arrival: 0, ticks: 1 }, Job { id: 82, arrival: 9, ticks: 2 }], 2, 4), Ok(Schedule {
        runs: vec![Run { id: 81, start: 0, end: 1, finished: true }, Run { id: 82, start: 9, end: 11, finished: true }],
        idle: vec![(1, 9)],
        completion: vec![(81, 1), (82, 11)],
        end: 11
    }));
}

// R14, R20-R21
#[test]
fn no_idle_at_exact_completion_arrival() {
    assert_eq!(schedule(&[Job { id: 28, arrival: 0, ticks: 2 }, Job { id: 29, arrival: 2, ticks: 1 }], 4, 3), Ok(Schedule {
        runs: vec![Run { id: 28, start: 0, end: 2, finished: true }, Run { id: 29, start: 5, end: 6, finished: true }],
        idle: vec![],
        completion: vec![(28, 2), (29, 6)],
        end: 6
    }));
}

// R10, R20-R23
#[test]
fn zero_arrival_after_final_work() {
    assert_eq!(schedule(&[Job { id: 3, arrival: 0, ticks: 2 }, Job { id: 2, arrival: 6, ticks: 0 }], 3, 4), Ok(Schedule {
        runs: vec![Run { id: 3, start: 0, end: 2, finished: true }],
        idle: vec![(2, 6)],
        completion: vec![(3, 2), (2, 6)],
        end: 6
    }));
}

// R10, R20-R21
#[test]
fn idle_merges_across_zero_arrival() {
    assert_eq!(schedule(&[Job { id: 1, arrival: 1, ticks: 1 }, Job { id: 2, arrival: 5, ticks: 0 }, Job { id: 3, arrival: 9, ticks: 1 }], 2, 6), Ok(Schedule {
        runs: vec![Run { id: 1, start: 1, end: 2, finished: true }, Run { id: 3, start: 9, end: 10, finished: true }],
        idle: vec![(0, 1), (2, 9)],
        completion: vec![(1, 2), (2, 5), (3, 10)],
        end: 10
    }));
}

// R13-R15, R17
#[test]
fn locked_dispatch_cost_keeps_fifo() {
    assert_eq!(schedule(&[Job { id: 4, arrival: 0, ticks: 4 }, Job { id: 5, arrival: 0, ticks: 1 }, Job { id: 6, arrival: 3, ticks: 1 }], 2, 4), Ok(Schedule {
        runs: vec![Run { id: 4, start: 0, end: 2, finished: false }, Run { id: 5, start: 6, end: 7, finished: true }, Run { id: 4, start: 11, end: 13, finished: true }, Run { id: 6, start: 17, end: 18, finished: true }],
        idle: vec![],
        completion: vec![(4, 13), (5, 7), (6, 18)],
        end: 18
    }));
}

// R9, R13-R15
#[test]
fn multiple_arrivals_during_dispatch_gap() {
    assert_eq!(schedule(&[Job { id: 10, arrival: 0, ticks: 1 }, Job { id: 20, arrival: 0, ticks: 1 }, Job { id: 40, arrival: 4, ticks: 1 }, Job { id: 30, arrival: 3, ticks: 1 }], 1, 7), Ok(Schedule {
        runs: vec![Run { id: 10, start: 0, end: 1, finished: true }, Run { id: 20, start: 8, end: 9, finished: true }, Run { id: 30, start: 16, end: 17, finished: true }, Run { id: 40, start: 24, end: 25, finished: true }],
        idle: vec![],
        completion: vec![(10, 1), (20, 9), (40, 25), (30, 17)],
        end: 25
    }));
}

// R16, R18-R19
#[test]
fn single_job_cost_free_unmerged() {
    assert_eq!(schedule(&[Job { id: 52, arrival: 0, ticks: 8 }], 3, 1000), Ok(Schedule {
        runs: vec![Run { id: 52, start: 0, end: 3, finished: false }, Run { id: 52, start: 3, end: 6, finished: false }, Run { id: 52, start: 6, end: 8, finished: true }],
        idle: vec![],
        completion: vec![(52, 8)],
        end: 8
    }));
}

// R10, R19
#[test]
fn zero_arrivals_leave_last_id() {
    assert_eq!(schedule(&[Job { id: 52, arrival: 0, ticks: 8 }, Job { id: 53, arrival: 2, ticks: 0 }], 2, 7), Ok(Schedule {
        runs: vec![Run { id: 52, start: 0, end: 2, finished: false }, Run { id: 52, start: 2, end: 4, finished: false }, Run { id: 52, start: 4, end: 6, finished: false }, Run { id: 52, start: 6, end: 8, finished: true }],
        idle: vec![],
        completion: vec![(52, 8), (53, 2)],
        end: 8
    }));
}

// R16, R18
#[test]
fn exact_multiple_final_run() {
    assert_eq!(schedule(&[Job { id: 48, arrival: 4, ticks: 6 }], 3, 9), Ok(Schedule {
        runs: vec![Run { id: 48, start: 4, end: 7, finished: false }, Run { id: 48, start: 7, end: 10, finished: true }],
        idle: vec![(0, 4)],
        completion: vec![(48, 10)],
        end: 10
    }));
}

// R9-R10, R22
#[test]
fn zero_at_start_not_before_arrival() {
    assert_eq!(schedule(&[Job { id: 0, arrival: 0, ticks: 0 }, Job { id: 4294967295, arrival: 4, ticks: 1 }], 2, 4), Ok(Schedule {
        runs: vec![Run { id: 4294967295, start: 4, end: 5, finished: true }],
        idle: vec![(0, 4)],
        completion: vec![(0, 0), (4294967295, 5)],
        end: 5
    }));
}

// R23
#[test]
fn input_order_completion_not_finish_order() {
    assert_eq!(schedule(&[Job { id: 21, arrival: 0, ticks: 7 }, Job { id: 22, arrival: 1, ticks: 1 }, Job { id: 23, arrival: 2, ticks: 0 }], 2, 0), Ok(Schedule {
        runs: vec![Run { id: 21, start: 0, end: 2, finished: false }, Run { id: 22, start: 2, end: 3, finished: true }, Run { id: 21, start: 3, end: 5, finished: false }, Run { id: 21, start: 5, end: 7, finished: false }, Run { id: 21, start: 7, end: 8, finished: true }],
        idle: vec![],
        completion: vec![(21, 8), (22, 3), (23, 2)],
        end: 8
    }));
}

// R3-R4, R7-R8, R24
#[test]
fn maximum_time_bounds() {
    assert_eq!(schedule(&[Job { id: 5, arrival: 1000, ticks: 1000 }, Job { id: 6, arrival: 1000, ticks: 1000 }], 1000, 1000), Ok(Schedule {
        runs: vec![Run { id: 5, start: 1000, end: 2000, finished: true }, Run { id: 6, start: 3000, end: 4000, finished: true }],
        idle: vec![(0, 1000)],
        completion: vec![(5, 2000), (6, 4000)],
        end: 4000
    }));
}

// R5, R9, R11, R24
#[test]
fn full_capacity_tied_arrivals() {
    assert_eq!(schedule(&[Job { id: 100, arrival: 3, ticks: 1 }, Job { id: 101, arrival: 3, ticks: 1 }, Job { id: 102, arrival: 3, ticks: 1 }, Job { id: 103, arrival: 3, ticks: 1 }, Job { id: 104, arrival: 3, ticks: 1 }, Job { id: 105, arrival: 3, ticks: 1 }, Job { id: 106, arrival: 3, ticks: 1 }, Job { id: 107, arrival: 3, ticks: 1 }, Job { id: 108, arrival: 3, ticks: 1 }, Job { id: 109, arrival: 3, ticks: 1 }, Job { id: 110, arrival: 3, ticks: 1 }, Job { id: 111, arrival: 3, ticks: 1 }, Job { id: 112, arrival: 3, ticks: 1 }, Job { id: 113, arrival: 3, ticks: 1 }, Job { id: 114, arrival: 3, ticks: 1 }, Job { id: 115, arrival: 3, ticks: 1 }, Job { id: 116, arrival: 3, ticks: 1 }, Job { id: 117, arrival: 3, ticks: 1 }, Job { id: 118, arrival: 3, ticks: 1 }, Job { id: 119, arrival: 3, ticks: 1 }, Job { id: 120, arrival: 3, ticks: 1 }, Job { id: 121, arrival: 3, ticks: 1 }, Job { id: 122, arrival: 3, ticks: 1 }, Job { id: 123, arrival: 3, ticks: 1 }, Job { id: 124, arrival: 3, ticks: 1 }, Job { id: 125, arrival: 3, ticks: 1 }, Job { id: 126, arrival: 3, ticks: 1 }, Job { id: 127, arrival: 3, ticks: 1 }, Job { id: 128, arrival: 3, ticks: 1 }, Job { id: 129, arrival: 3, ticks: 1 }, Job { id: 130, arrival: 3, ticks: 1 }, Job { id: 131, arrival: 3, ticks: 1 }, Job { id: 132, arrival: 3, ticks: 1 }, Job { id: 133, arrival: 3, ticks: 1 }, Job { id: 134, arrival: 3, ticks: 1 }, Job { id: 135, arrival: 3, ticks: 1 }, Job { id: 136, arrival: 3, ticks: 1 }, Job { id: 137, arrival: 3, ticks: 1 }, Job { id: 138, arrival: 3, ticks: 1 }, Job { id: 139, arrival: 3, ticks: 1 }, Job { id: 140, arrival: 3, ticks: 1 }, Job { id: 141, arrival: 3, ticks: 1 }, Job { id: 142, arrival: 3, ticks: 1 }, Job { id: 143, arrival: 3, ticks: 1 }, Job { id: 144, arrival: 3, ticks: 1 }, Job { id: 145, arrival: 3, ticks: 1 }, Job { id: 146, arrival: 3, ticks: 1 }, Job { id: 147, arrival: 3, ticks: 1 }, Job { id: 148, arrival: 3, ticks: 1 }, Job { id: 149, arrival: 3, ticks: 1 }, Job { id: 150, arrival: 3, ticks: 1 }, Job { id: 151, arrival: 3, ticks: 1 }, Job { id: 152, arrival: 3, ticks: 1 }, Job { id: 153, arrival: 3, ticks: 1 }, Job { id: 154, arrival: 3, ticks: 1 }, Job { id: 155, arrival: 3, ticks: 1 }, Job { id: 156, arrival: 3, ticks: 1 }, Job { id: 157, arrival: 3, ticks: 1 }, Job { id: 158, arrival: 3, ticks: 1 }, Job { id: 159, arrival: 3, ticks: 1 }, Job { id: 160, arrival: 3, ticks: 1 }, Job { id: 161, arrival: 3, ticks: 1 }, Job { id: 162, arrival: 3, ticks: 1 }, Job { id: 163, arrival: 3, ticks: 1 }], 1, 0), Ok(Schedule {
        runs: vec![Run { id: 100, start: 3, end: 4, finished: true }, Run { id: 101, start: 4, end: 5, finished: true }, Run { id: 102, start: 5, end: 6, finished: true }, Run { id: 103, start: 6, end: 7, finished: true }, Run { id: 104, start: 7, end: 8, finished: true }, Run { id: 105, start: 8, end: 9, finished: true }, Run { id: 106, start: 9, end: 10, finished: true }, Run { id: 107, start: 10, end: 11, finished: true }, Run { id: 108, start: 11, end: 12, finished: true }, Run { id: 109, start: 12, end: 13, finished: true }, Run { id: 110, start: 13, end: 14, finished: true }, Run { id: 111, start: 14, end: 15, finished: true }, Run { id: 112, start: 15, end: 16, finished: true }, Run { id: 113, start: 16, end: 17, finished: true }, Run { id: 114, start: 17, end: 18, finished: true }, Run { id: 115, start: 18, end: 19, finished: true }, Run { id: 116, start: 19, end: 20, finished: true }, Run { id: 117, start: 20, end: 21, finished: true }, Run { id: 118, start: 21, end: 22, finished: true }, Run { id: 119, start: 22, end: 23, finished: true }, Run { id: 120, start: 23, end: 24, finished: true }, Run { id: 121, start: 24, end: 25, finished: true }, Run { id: 122, start: 25, end: 26, finished: true }, Run { id: 123, start: 26, end: 27, finished: true }, Run { id: 124, start: 27, end: 28, finished: true }, Run { id: 125, start: 28, end: 29, finished: true }, Run { id: 126, start: 29, end: 30, finished: true }, Run { id: 127, start: 30, end: 31, finished: true }, Run { id: 128, start: 31, end: 32, finished: true }, Run { id: 129, start: 32, end: 33, finished: true }, Run { id: 130, start: 33, end: 34, finished: true }, Run { id: 131, start: 34, end: 35, finished: true }, Run { id: 132, start: 35, end: 36, finished: true }, Run { id: 133, start: 36, end: 37, finished: true }, Run { id: 134, start: 37, end: 38, finished: true }, Run { id: 135, start: 38, end: 39, finished: true }, Run { id: 136, start: 39, end: 40, finished: true }, Run { id: 137, start: 40, end: 41, finished: true }, Run { id: 138, start: 41, end: 42, finished: true }, Run { id: 139, start: 42, end: 43, finished: true }, Run { id: 140, start: 43, end: 44, finished: true }, Run { id: 141, start: 44, end: 45, finished: true }, Run { id: 142, start: 45, end: 46, finished: true }, Run { id: 143, start: 46, end: 47, finished: true }, Run { id: 144, start: 47, end: 48, finished: true }, Run { id: 145, start: 48, end: 49, finished: true }, Run { id: 146, start: 49, end: 50, finished: true }, Run { id: 147, start: 50, end: 51, finished: true }, Run { id: 148, start: 51, end: 52, finished: true }, Run { id: 149, start: 52, end: 53, finished: true }, Run { id: 150, start: 53, end: 54, finished: true }, Run { id: 151, start: 54, end: 55, finished: true }, Run { id: 152, start: 55, end: 56, finished: true }, Run { id: 153, start: 56, end: 57, finished: true }, Run { id: 154, start: 57, end: 58, finished: true }, Run { id: 155, start: 58, end: 59, finished: true }, Run { id: 156, start: 59, end: 60, finished: true }, Run { id: 157, start: 60, end: 61, finished: true }, Run { id: 158, start: 61, end: 62, finished: true }, Run { id: 159, start: 62, end: 63, finished: true }, Run { id: 160, start: 63, end: 64, finished: true }, Run { id: 161, start: 64, end: 65, finished: true }, Run { id: 162, start: 65, end: 66, finished: true }, Run { id: 163, start: 66, end: 67, finished: true }],
        idle: vec![(0, 3)],
        completion: vec![(100, 4), (101, 5), (102, 6), (103, 7), (104, 8), (105, 9), (106, 10), (107, 11), (108, 12), (109, 13), (110, 14), (111, 15), (112, 16), (113, 17), (114, 18), (115, 19), (116, 20), (117, 21), (118, 22), (119, 23), (120, 24), (121, 25), (122, 26), (123, 27), (124, 28), (125, 29), (126, 30), (127, 31), (128, 32), (129, 33), (130, 34), (131, 35), (132, 36), (133, 37), (134, 38), (135, 39), (136, 40), (137, 41), (138, 42), (139, 43), (140, 44), (141, 45), (142, 46), (143, 47), (144, 48), (145, 49), (146, 50), (147, 51), (148, 52), (149, 53), (150, 54), (151, 55), (152, 56), (153, 57), (154, 58), (155, 59), (156, 60), (157, 61), (158, 62), (159, 63), (160, 64), (161, 65), (162, 66), (163, 67)],
        end: 67
    }));
}

// R12, R15, R17
#[test]
fn busy_through_late_arrivals() {
    assert_eq!(schedule(&[Job { id: 100, arrival: 0, ticks: 8 }, Job { id: 200, arrival: 2, ticks: 1 }, Job { id: 300, arrival: 4, ticks: 2 }, Job { id: 400, arrival: 7, ticks: 1 }], 8, 2), Ok(Schedule {
        runs: vec![Run { id: 100, start: 0, end: 8, finished: true }, Run { id: 200, start: 10, end: 11, finished: true }, Run { id: 300, start: 13, end: 15, finished: true }, Run { id: 400, start: 17, end: 18, finished: true }],
        idle: vec![],
        completion: vec![(100, 8), (200, 11), (300, 15), (400, 18)],
        end: 18
    }));
}

// R10, R17-R18
#[test]
fn zeros_at_run_endpoint() {
    assert_eq!(schedule(&[Job { id: 43, arrival: 0, ticks: 4 }, Job { id: 44, arrival: 2, ticks: 0 }, Job { id: 45, arrival: 2, ticks: 1 }], 2, 1), Ok(Schedule {
        runs: vec![Run { id: 43, start: 0, end: 2, finished: false }, Run { id: 45, start: 3, end: 4, finished: true }, Run { id: 43, start: 5, end: 7, finished: true }],
        idle: vec![],
        completion: vec![(43, 7), (44, 2), (45, 4)],
        end: 7
    }));
}

// R1; repeated calls use only their arguments.
#[test]
fn independent_calls_preserve_input() {
    let jobs = vec![Job { id: 27, arrival: 2, ticks: 5 }, Job { id: 8, arrival: 0, ticks: 2 }];
    let original = jobs.clone();
    let first = schedule(&jobs, 3, 2).unwrap();
    let _ = schedule(&[], 3, 1).unwrap();
    assert_eq!(schedule(&jobs, 3, 2).unwrap(), first);
    assert_eq!(jobs, original);
}
