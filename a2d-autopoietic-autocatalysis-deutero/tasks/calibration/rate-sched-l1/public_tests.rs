#[test]
fn public_overlap() {
let v = vec![Interval { start: 8, end: 13 }, Interval { start: 2, end: 10 }];
assert_eq!(normalize(&v, false), Ok(vec![Interval { start: 2, end: 13 }]));
}

#[test]
fn public_touch_policy() {
let v = vec![Interval { start: 3, end: 6 }, Interval { start: 6, end: 9 }];
assert_eq!(normalize(&v, false), Ok(v.clone()));
assert_eq!(normalize(&v, true), Ok(vec![Interval { start: 3, end: 9 }]));
}

#[test]
fn public_empty_input() {
assert_eq!(normalize(&[], true), Ok(vec![]));
}

#[test]
fn public_reversed() {
assert_eq!(normalize(&[Interval { start: 7, end: 4 }], false), Err(MergeError::Reversed { index: 0 }));
}
