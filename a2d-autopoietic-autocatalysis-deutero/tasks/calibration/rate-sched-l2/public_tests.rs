#[test]
fn public_bucket_acquire() {
let mut b=TokenBucket::new(5,2,10,0).unwrap();
assert_eq!(b.try_acquire(3,0),Ok(true));
assert_eq!(b.available(0),Ok(2));
assert_eq!(b.try_acquire(3,0),Ok(false));
assert_eq!(b.try_acquire(4,10),Ok(true));
}

#[test]
fn public_bucket_capacity() {
let mut b=TokenBucket::new(7,3,4,2).unwrap();
assert_eq!(b.available(100),Ok(7));
assert_eq!(b.try_acquire(7,100),Ok(true));
assert_eq!(b.available(104),Ok(3));
}

#[test]
fn public_bucket_zero_interval() {
assert!(matches!(TokenBucket::new(2,1,0,0),Err(BucketError::ZeroInterval)));
}

#[test]
fn public_bucket_zero_request() {
let mut b=TokenBucket::new(4,0,1,0).unwrap();
assert_eq!(b.try_acquire(0,0),Ok(true));
assert_eq!(b.available(0),Ok(4));
}
