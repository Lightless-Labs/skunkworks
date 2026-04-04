pub mod error;
pub mod id;
pub mod protocol;
pub mod traits;

pub fn fibonacci(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut a = 0u64;
    let mut b = 1u64;
    for _ in 1..n {
        let tmp = a + b;
        a = b;
        b = tmp;
    }
    b
}

#[test]
fn test_fibonacci() {
    assert_eq!(fibonacci(0), 0);
    assert_eq!(fibonacci(1), 1);
    assert_eq!(fibonacci(10), 55);
    assert_eq!(fibonacci(20), 6765);
}
