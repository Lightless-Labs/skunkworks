#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BucketError { ZeroInterval, TimeWentBackwards }
pub struct TokenBucket {
    capacity: u64, refill: u64, interval: u64,
    tokens: u64, remainder: u128, last: u64,
}
impl TokenBucket {
    pub fn new(capacity: u64, refill: u64, interval_ms: u64, start_ms: u64) -> Result<Self, BucketError> {
        if interval_ms == 0 { return Err(BucketError::ZeroInterval); }
        Ok(Self { capacity, refill, interval: interval_ms, tokens: capacity, remainder: 0, last: start_ms })
    }
    fn advance(&mut self, now: u64) -> Result<(), BucketError> {
        if now < self.last { return Err(BucketError::TimeWentBackwards); }
        let numerator = (now - self.last) as u128 * self.refill as u128 + self.remainder;
        let whole = numerator / self.interval as u128;
        let remainder = numerator % self.interval as u128;
        let total = self.tokens as u128 + whole;
        if total >= self.capacity as u128 {
            self.tokens = self.capacity;
            self.remainder = remainder;
        } else {
            self.tokens = total as u64;
            self.remainder = remainder;
        }
        self.last = now;
        Ok(())
    }
    pub fn available(&mut self, now: u64) -> Result<u64, BucketError> {
        self.advance(now)?;
        Ok(self.tokens)
    }
    pub fn try_acquire(&mut self, n: u64, now: u64) -> Result<bool, BucketError> {
        self.advance(now)?;
        if n <= self.tokens {
            self.tokens -= n;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
