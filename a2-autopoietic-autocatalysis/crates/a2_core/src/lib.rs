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
