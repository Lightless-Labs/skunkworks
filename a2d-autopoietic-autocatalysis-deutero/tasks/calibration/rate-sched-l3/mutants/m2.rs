use std::collections::{BTreeMap, VecDeque};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowError { ZeroWindow, ZeroIdleTtl, TimeWentBackwards }
struct KeyState { events: VecDeque<(u64, u64)>, used: u64, last_activity: u64 }
impl KeyState {
    fn purge(&mut self, now: u64, window: u64) {
        while let Some(&(time, weight)) = self.events.front() {
            if now - time >= window {
                self.events.pop_front();
                self.used -= weight;
            } else { break; }
        }
    }
}
pub struct SlidingWindow {
    limit: u64, window: u64, idle_ttl: u64, last: u64,
    keys: BTreeMap<String, KeyState>,
}
impl SlidingWindow {
    pub fn new(limit: u64, window_ms: u64, idle_ttl_ms: u64, start_ms: u64) -> Result<Self, WindowError> {
        if window_ms == 0 { return Err(WindowError::ZeroWindow); }
        if idle_ttl_ms == 0 { return Err(WindowError::ZeroIdleTtl); }
        Ok(Self { limit, window: window_ms, idle_ttl: idle_ttl_ms, last: start_ms, keys: BTreeMap::new() })
    }
    fn advance(&mut self, now: u64) -> Result<(), WindowError> {
        if now < self.last { return Err(WindowError::TimeWentBackwards); }
        self.last = now;
        Ok(())
    }
    pub fn try_acquire(&mut self, key: &str, n: u64, now: u64) -> Result<bool, WindowError> {
        self.advance(now)?;
        if n == 0 {
            if let Some(state) = self.keys.get_mut(key) { state.purge(now, self.window); }
            return Ok(true);
        }
        let state = self.keys.entry(key.to_owned()).or_insert_with(|| KeyState {
            events: VecDeque::new(), used: 0, last_activity: now,
        });
        state.purge(now, self.window);
        if n > self.limit - state.used { return Ok(false); }
        state.last_activity = now;
        state.used += n;
        state.events.push_back((now, n));
        Ok(true)
    }
    pub fn remaining(&mut self, key: &str, now: u64) -> Result<u64, WindowError> {
        self.advance(now)?;
        if let Some(state) = self.keys.get_mut(key) {
            state.purge(now, self.window);
            Ok(self.limit - state.used)
        } else { Ok(self.limit) }
    }
    pub fn evict_idle(&mut self, now: u64) -> Result<Vec<String>, WindowError> {
        self.advance(now)?;
        let removed: Vec<String> = self.keys.iter().filter(|(_, state)| now - state.last_activity >= self.idle_ttl)
            .map(|(key, _)| key.clone()).collect();
        for key in &removed { self.keys.remove(key); }
        Ok(removed)
    }
    pub fn tracked_keys(&self) -> usize { self.keys.len() }
}
