fn c(id: u32, arrival: u64, service: u64, priority: u8, patience: Option<u64>) -> Customer {
    Customer {
        id,
        arrival,
        service,
        priority,
        patience,
    }
}
fn cfg(waiting_capacity: usize, switch_cost: u64) -> Config {
    Config {
        switch_cost,
        waiting_capacity,
    }
}
fn e(time: u64, customer: u32, kind: Kind) -> Event {
    Event {
        time,
        customer,
        kind,
    }
}
#[test]
fn public_complete_and_stats() {
    let r = simulate(&[c(1, 2, 3, 1, None), c(2, 3, 2, 1, None)], &cfg(2, 1)).unwrap();
    assert_eq!(
        r.stats,
        Stats {
            accepted: 2,
            balked: 0,
            reneged: 0,
            completed: 2,
            started: 2,
            preemptions: 0,
            elapsed: 8,
            busy_ticks: 6,
            wait_ticks: 2,
            turnaround_ticks: 8,
            mean_wait_milli: 1000,
            mean_turnaround_milli: 4000,
            utilization_bps: 7500,
            completion_order: vec![1, 2]
        }
    );
    assert_eq!(
        r.events[4],
        e(
            5,
            2,
            Kind::Start {
                remaining: 2,
                work_at: 6,
                finish: 8
            }
        )
    );
}
#[test]
fn public_balk_and_renege() {
    let r = simulate(
        &[
            c(1, 0, 10, 0, None),
            c(2, 1, 1, 1, Some(2)),
            c(3, 2, 1, 2, None),
        ],
        &cfg(1, 0),
    )
    .unwrap();
    assert!(r.events.contains(&e(2, 3, Kind::Balk)));
    assert!(r.events.contains(&e(3, 2, Kind::Renege { waited: 2 })));
    assert_eq!(
        (
            r.stats.accepted,
            r.stats.balked,
            r.stats.reneged,
            r.stats.completed
        ),
        (2, 1, 1, 1)
    );
}
#[test]
fn public_preempt_resume() {
    let r = simulate(&[c(4, 0, 5, 3, None), c(5, 1, 1, 0, None)], &cfg(2, 0)).unwrap();
    assert!(r.events.contains(&e(
        1,
        4,
        Kind::Preempt {
            remaining: 4,
            by: 5
        }
    )));
    assert!(r.events.contains(&e(
        2,
        4,
        Kind::Resume {
            remaining: 4,
            work_at: 2,
            finish: 6
        }
    )));
    assert_eq!(r.stats.completion_order, vec![5, 4]);
    assert_eq!(
        (
            r.stats.busy_ticks,
            r.stats.wait_ticks,
            r.stats.turnaround_ticks
        ),
        (6, 1, 7)
    );
}
#[test]
fn public_round_and_empty() {
    assert_eq!(rounded_ratio(7, 2), Some(4));
    assert_eq!(rounded_ratio(6, 0), None);
    let r = simulate(&[], &cfg(0, 5)).unwrap();
    assert!(r.events.is_empty());
    assert_eq!(
        (r.stats.elapsed, r.stats.busy_ticks, r.stats.mean_wait_milli),
        (0, 0, 0)
    );
}
