#[test]
fn hidden_drop_empty() {
let v = vec![Interval { start: 0, end: 0 }, Interval { start: 5, end: 5 }, Interval { start: u64::MAX, end: u64::MAX }];
assert_eq!(normalize(&v, false), Ok(vec![]));
assert_eq!(normalize(&v, true), Ok(vec![]));
}

#[test]
fn hidden_empty_cannot_bridge() {
let v = vec![Interval { start: 1, end: 4 }, Interval { start: 4, end: 4 }, Interval { start: 4, end: 7 }];
assert_eq!(normalize(&v, false), Ok(vec![v[0].clone(), v[2].clone()]));
}

#[test]
fn hidden_first_error_original() {
let v = vec![Interval { start: 50, end: 60 }, Interval { start: 70, end: 5 }, Interval { start: 2, end: 1 }];
assert_eq!(normalize(&v, true), Err(MergeError::Reversed { index: 1 }));
}

#[test]
fn hidden_validate_after_empties() {
let v = vec![Interval { start: 99, end: 99 }, Interval { start: 5, end: 5 }, Interval { start: 14, end: 13 }];
assert_eq!(normalize(&v, false), Err(MergeError::Reversed { index: 2 }));
}

#[test]
fn hidden_nested_end() {
let v = vec![Interval { start: 1, end: 100 }, Interval { start: 3, end: 6 }, Interval { start: 90, end: 120 }];
assert_eq!(normalize(&v, false), Ok(vec![Interval { start: 1, end: 120 }]));
}

#[test]
fn hidden_nested_then_disjoint() {
let v = vec![Interval { start: 2, end: 40 }, Interval { start: 8, end: 9 }, Interval { start: 41, end: 42 }];
assert_eq!(normalize(&v, true), Ok(vec![v[0].clone(),v[2].clone()]));
}

#[test]
fn hidden_duplicate_intervals() {
let v = vec![Interval { start: 11, end: 18 }; 20];
assert_eq!(normalize(&v, false), Ok(vec![v[0].clone()]));
}

#[test]
fn hidden_equal_start() {
let v = vec![Interval { start: 6, end: 50 }, Interval { start: 6, end: 8 }, Interval { start: 6, end: 30 }];
assert_eq!(normalize(&v, true), Ok(vec![Interval { start: 6, end: 50 }]));
}

#[test]
fn hidden_sorted_disjoint() {
let v = vec![Interval { start: 30, end: 31 }, Interval { start: 10, end: 11 }, Interval { start: 20, end: 21 }];
assert_eq!(normalize(&v, false), Ok(vec![v[1].clone(),v[2].clone(),v[0].clone()]));
}

#[test]
fn hidden_touch_chain() {
let v = vec![Interval { start: 12, end: 15 }, Interval { start: 3, end: 6 }, Interval { start: 9, end: 12 }, Interval { start: 6, end: 9 }];
assert_eq!(normalize(&v, true), Ok(vec![Interval { start: 3, end: 15 }]));
assert_eq!(normalize(&v, false).unwrap().len(), 4);
}

#[test]
fn hidden_overlap_chain() {
let v: Vec<_> = (0..80).rev().map(|i| Interval { start: i*2, end: i*2+3 }).collect();
assert_eq!(normalize(&v, false), Ok(vec![Interval { start: 0, end: 161 }]));
}

#[test]
fn hidden_max_endpoint() {
let m = u64::MAX;
let v = vec![Interval { start: m-4, end: m }, Interval { start: m-9, end: m-4 }];
assert_eq!(normalize(&v, true), Ok(vec![Interval { start: m-9, end: m }]));
assert_eq!(normalize(&v, false).unwrap().len(), 2);
}

#[test]
fn hidden_full_domain() {
let v = vec![Interval { start: 0, end: u64::MAX }, Interval { start: 17, end: 18 }, Interval { start: u64::MAX, end: u64::MAX }];
assert_eq!(normalize(&v, false), Ok(vec![v[0].clone()]));
}

#[test]
fn hidden_input_preserved() {
let v = vec![Interval { start: 19, end: 23 }, Interval { start: 1, end: 3 }];
let copy=v.clone();
normalize(&v, true).unwrap();
assert_eq!(v, copy);
}

#[test]
fn hidden_idempotent() {
let v = vec![Interval { start: 2, end: 20 }, Interval { start: 5, end: 7 }, Interval { start: 20, end: 22 }, Interval { start: 30, end: 30 }];
for touching in [false,true] { let once=normalize(&v,touching).unwrap(); assert_eq!(normalize(&once,touching),Ok(once)); }
}

#[test]
fn hidden_small_gap() {
let v=vec![Interval { start: 0, end: 1 },Interval { start: 2, end: 3 }];
assert_eq!(normalize(&v,true),Ok(v.clone()));
assert_eq!(normalize(&v,false),Ok(v));
}
