#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interval { pub start: u64, pub end: u64 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeError { Reversed { index: usize } }
pub fn normalize(intervals: &[Interval], merge_touching: bool) -> Result<Vec<Interval>, MergeError> {
    for (index, interval) in intervals.iter().enumerate() {
        if interval.start > interval.end { return Err(MergeError::Reversed { index }); }
    }
    let mut sorted: Vec<Interval> = intervals.iter().filter(|i| i.start != i.end).cloned().collect();
    sorted.sort_by_key(|i| (i.start, i.end));
    let mut result: Vec<Interval> = Vec::new();
    for interval in sorted {
        if let Some(last) = result.last_mut() {
            if interval.start < last.end || (merge_touching && interval.start == last.end) {
                last.end = interval.end;
                continue;
            }
        }
        result.push(interval);
    }
    Ok(result)
}
