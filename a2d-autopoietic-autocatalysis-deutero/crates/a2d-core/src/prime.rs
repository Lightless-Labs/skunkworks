/// Determines whether an integer is prime.
///
/// Negative numbers are rejected because primality is only defined here for
/// non-negative integers. Values `0` and `1` are treated as non-prime.
///
/// # Errors
///
/// Returns an error when `n` is negative.
///
/// # Examples
///
/// ```
/// use a2d_core::is_prime;
///
/// assert_eq!(is_prime(2), Ok(true));
/// assert_eq!(is_prime(21), Ok(false));
/// assert!(is_prime(-7).is_err());
/// ```
pub fn is_prime(n: i64) -> Result<bool, String> {
    if n < 0 {
        return Err(format!(
            "{n} is negative; primality is only defined for non-negative integers"
        ));
    }

    if n < 2 {
        return Ok(false);
    }

    if n == 2 {
        return Ok(true);
    }

    if n % 2 == 0 {
        return Ok(false);
    }

    let n = n as u64;
    let mut divisor = 3;
    while divisor <= n / divisor {
        if n % divisor == 0 {
            return Ok(false);
        }
        divisor += 2;
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::is_prime;

    #[test]
    fn rejects_negative_numbers() {
        assert_eq!(
            is_prime(-3),
            Err("-3 is negative; primality is only defined for non-negative integers".to_string())
        );
    }

    #[test]
    fn rejects_numbers_below_two() {
        assert_eq!(is_prime(0), Ok(false));
        assert_eq!(is_prime(1), Ok(false));
    }

    #[test]
    fn accepts_small_primes() {
        assert_eq!(is_prime(2), Ok(true));
        assert_eq!(is_prime(3), Ok(true));
        assert_eq!(is_prime(5), Ok(true));
        assert_eq!(is_prime(97), Ok(true));
    }

    #[test]
    fn rejects_non_prime_numbers() {
        assert_eq!(is_prime(4), Ok(false));
        assert_eq!(is_prime(9), Ok(false));
        assert_eq!(is_prime(21), Ok(false));
        assert_eq!(is_prime(100), Ok(false));
    }

    #[test]
    fn handles_larger_prime_and_composite_values() {
        assert_eq!(is_prime(7919), Ok(true));
        assert_eq!(is_prime(7920), Ok(false));
    }
}
