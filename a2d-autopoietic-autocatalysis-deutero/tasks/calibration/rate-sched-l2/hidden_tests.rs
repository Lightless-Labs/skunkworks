#[test]
fn hidden_fraction_accumulates() {
let mut b=TokenBucket::new(9,1,7,0).unwrap();
assert_eq!(b.try_acquire(9,0),Ok(true));
for t in 1..7 { assert_eq!(b.available(t),Ok(0)); }
assert_eq!(b.available(7),Ok(1));
assert_eq!(b.available(14),Ok(2));
}

#[test]
fn hidden_nondivisible_rate() {
let mut b=TokenBucket::new(20,3,8,0).unwrap();
b.try_acquire(20,0).unwrap();
for t in 1..25 { assert_eq!(b.available(t),Ok(t*3/8)); }
}

#[test]
fn hidden_denial_preserves_tokens() {
let mut b=TokenBucket::new(8,1,5,0).unwrap();
b.try_acquire(6,0).unwrap();
assert_eq!(b.try_acquire(3,0),Ok(false));
assert_eq!(b.available(0),Ok(2));
assert_eq!(b.try_acquire(2,0),Ok(true));
}

#[test]
fn hidden_denial_retains_fraction() {
let mut b=TokenBucket::new(4,2,9,0).unwrap();
b.try_acquire(4,0).unwrap();
assert_eq!(b.try_acquire(5,4),Ok(false));
assert_eq!(b.try_acquire(1,5),Ok(true));
assert_eq!(b.available(9),Ok(1));
}

#[test]
fn hidden_exact_saturation_drops_fraction() {
let mut b=TokenBucket::new(2,3,5,0).unwrap();
b.try_acquire(2,0).unwrap();
assert_eq!(b.available(4),Ok(2));
b.try_acquire(2,4).unwrap();
assert_eq!(b.available(5),Ok(0));
assert_eq!(b.available(6),Ok(1));
}

#[test]
fn hidden_overfill_drops_fraction() {
let mut b=TokenBucket::new(3,4,7,0).unwrap();
b.try_acquire(3,0).unwrap();
assert_eq!(b.available(10),Ok(3));
b.try_acquire(3,10).unwrap();
assert_eq!(b.available(11),Ok(0));
assert_eq!(b.available(12),Ok(1));
}

#[test]
fn hidden_no_bank_at_full() {
let mut b=TokenBucket::new(2,1,10,0).unwrap();
assert_eq!(b.available(9),Ok(2));
b.try_acquire(2,9).unwrap();
assert_eq!(b.available(10),Ok(0));
assert_eq!(b.available(19),Ok(1));
}

#[test]
fn hidden_backward_state() {
let mut b=TokenBucket::new(10,1,10,50).unwrap();
b.try_acquire(10,50).unwrap();
assert_eq!(b.available(55),Ok(0));
assert_eq!(b.available(51),Err(BucketError::TimeWentBackwards));
assert_eq!(b.try_acquire(0,54),Err(BucketError::TimeWentBackwards));
assert_eq!(b.available(60),Ok(1));
}

#[test]
fn hidden_before_start() {
let mut b=TokenBucket::new(6,2,3,100).unwrap();
assert_eq!(b.try_acquire(1,99),Err(BucketError::TimeWentBackwards));
assert_eq!(b.available(100),Ok(6));
}

#[test]
fn hidden_zero_capacity() {
let mut b=TokenBucket::new(0,u64::MAX,1,0).unwrap();
assert_eq!(b.try_acquire(1,0),Ok(false));
assert_eq!(b.try_acquire(0,u64::MAX),Ok(true));
assert_eq!(b.available(u64::MAX),Ok(0));
}

#[test]
fn hidden_no_refill() {
let mut b=TokenBucket::new(11,0,u64::MAX,0).unwrap();
b.try_acquire(7,0).unwrap();
assert_eq!(b.available(u64::MAX),Ok(4));
assert_eq!(b.try_acquire(5,u64::MAX),Ok(false));
assert_eq!(b.available(u64::MAX),Ok(4));
}

#[test]
fn hidden_wide_product() {
let mut b=TokenBucket::new(u64::MAX,u64::MAX,1,0).unwrap();
b.try_acquire(u64::MAX,0).unwrap();
assert_eq!(b.available(u64::MAX),Ok(u64::MAX));
}

#[test]
fn hidden_wide_fraction() {
let m=u64::MAX;
let mut b=TokenBucket::new(m,m-1,m,0).unwrap();
b.try_acquire(m,0).unwrap();
assert_eq!(b.available(m-1),Ok(m-2));
assert_eq!(b.available(m),Ok(m-1));
}

#[test]
fn hidden_equal_time() {
let mut b=TokenBucket::new(7,5,11,12).unwrap();
b.try_acquire(7,12).unwrap();
assert_eq!(b.available(14),Ok(0));
for _ in 0..30 { assert_eq!(b.try_acquire(0,14),Ok(true)); }
assert_eq!(b.available(15),Ok(1));
}

#[test]
fn hidden_zero_request_advances() {
let mut b=TokenBucket::new(8,1,6,0).unwrap();
b.try_acquire(8,0).unwrap();
assert_eq!(b.try_acquire(0,5),Ok(true));
assert_eq!(b.available(4),Err(BucketError::TimeWentBackwards));
assert_eq!(b.try_acquire(1,6),Ok(true));
}

#[test]
fn hidden_oversized_advances() {
let mut b=TokenBucket::new(5,1,3,0).unwrap();
b.try_acquire(5,0).unwrap();
assert_eq!(b.try_acquire(u64::MAX,8),Ok(false));
assert_eq!(b.available(7),Err(BucketError::TimeWentBackwards));
assert_eq!(b.available(9),Ok(3));
}

#[test]
fn hidden_dense_sequence() {
let mut b=TokenBucket::new(1000,7,13,0).unwrap();
b.try_acquire(1000,0).unwrap();
let mut removed=0;
for t in 1..=400 { let expected=t*7/13-removed; assert_eq!(b.available(t),Ok(expected)); if expected>=2 { assert_eq!(b.try_acquire(2,t),Ok(true)); removed+=2; } }
}
