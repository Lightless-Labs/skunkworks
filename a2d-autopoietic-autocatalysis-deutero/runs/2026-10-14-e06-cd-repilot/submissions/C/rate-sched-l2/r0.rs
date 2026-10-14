use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronError {
    Layout(usize),
    Syntax(usize),
    Number(usize),
    Bounds(usize),
    Descending(usize),
    Step(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    minutes: [bool; 60],
    hours: [bool; 24],
}

fn parse_number(
    text: &str,
    start: usize,
    end: usize,
    limit: u32,
    error_kind: u8,
) -> Result<u32, CronError> {
    let bytes = text.as_bytes();
    if start == end {
        return Err(CronError::Syntax(start));
    }
    for i in start..end {
        if !bytes[i].is_ascii_digit() {
            return Err(CronError::Syntax(i));
        }
    }
    if end - start > 1 && bytes[start] == b'0' {
        return Err(CronError::Number(start));
    }

    let mut value = 0u32;
    for &b in &bytes[start..end] {
        value = match value
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u32))
        {
            Some(n) => n,
            None => return Err(if error_kind == 0 {
                CronError::Bounds(start)
            } else {
                CronError::Step(start)
            }),
        };
    }
    if value > limit {
        return Err(if error_kind == 0 {
            CronError::Bounds(start)
        } else {
            CronError::Step(start)
        });
    }
    Ok(value)
}

fn parse_field(
    text: &str,
    field_start: usize,
    field_end: usize,
    limit: u32,
    step_limit: u32,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let bytes = text.as_bytes();
    let mut item_start = field_start;

    loop {
        let mut item_end = item_start;
        while item_end < field_end && bytes[item_end] != b',' {
            item_end += 1;
        }
        if item_start == item_end {
            return Err(CronError::Syntax(item_start));
        }

        let mut slash = None;
        for i in item_start..item_end {
            if bytes[i] == b'/' {
                if slash.is_some() {
                    return Err(CronError::Syntax(i));
                }
                slash = Some(i);
            }
        }
        let base_end = slash.unwrap_or(item_end);

        if let Some(slash_at) = slash {
            let has_star = base_end == item_start + 1 && bytes[item_start] == b'*';
            let has_dash = bytes[item_start..base_end].contains(&b'-');
            if !has_star && !has_dash {
                return Err(CronError::Syntax(slash_at));
            }
        }

        let mut range_start = 0u32;
        let mut range_end = limit;
        let base = &bytes[item_start..base_end];

        if base.len() == 1 && base[0] == b'*' {
            // The wildcard selects the whole field.
        } else if let Some(rel_dash) = base.iter().position(|&b| b == b'-') {
            let dash = item_start + rel_dash;
            range_start = parse_number(text, item_start, dash, limit, 0)?;
            range_end = parse_number(text, dash + 1, base_end, limit, 0)?;
            if range_start > range_end {
                return Err(CronError::Descending(dash));
            }
        } else {
            let n = parse_number(text, item_start, base_end, limit, 0)?;
            range_start = n;
            range_end = n;
        }

        let step = if let Some(slash_at) = slash {
            parse_number(text, slash_at + 1, item_end, step_limit, 1)?
        } else {
            1
        };
        if step == 0 {
            return Err(CronError::Step(slash.map_or(item_start, |s| s + 1)));
        }

        let mut value = range_start;
        while value <= range_end {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        if item_end == field_end {
            break;
        }
        item_start = item_end + 1;
        if item_start > field_end {
            return Err(CronError::Syntax(field_end));
        }
    }
    Ok(())
}

impl Cron {
    /// Parses a five-field-independent minute and hour cron expression.
    pub fn parse(text: &str) -> Result<Self, CronError> {
        for (i, b) in text.bytes().enumerate() {
            if b.is_ascii_whitespace() && b != b' ' {
                return Err(CronError::Layout(i));
            }
        }
        let spaces: Vec<usize> = text
            .bytes()
            .enumerate()
            .filter_map(|(i, b)| (b == b' ').then_some(i))
            .collect();
        if spaces.is_empty() {
            return Err(CronError::Layout(text.len()));
        }
        if spaces.len() > 1 {
            return Err(CronError::Layout(spaces[1]));
        }
        let split = spaces[0];
        if split == 0 || split + 1 == text.len() {
            return Err(CronError::Layout(split));
        }

        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        parse_field(text, 0, split, 59, 60, &mut minutes)?;
        parse_field(text, split + 1, text.len(), 23, 24, &mut hours)?;
        Ok(Self { minutes, hours })
    }

    /// Tests whether an absolute minute matches this cron expression.
    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Policy {
    Skip,
    Once,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub id: String,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    TimeReversed,
    TimeRange,
    BadId,
    BadPriority,
    Duplicate,
    Cron(CronError),
}

struct Job {
    id: String,
    cron: Cron,
    priority: u8,
    policy: Policy,
    insertion: usize,
}

pub struct Calendar {
    jobs: Vec<Job>,
    ids: HashSet<String>,
    last_poll: Option<u64>,
}

impl Calendar {
    /// Creates an empty calendar.
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            ids: HashSet::new(),
            last_poll: None,
        }
    }

    /// Adds a job after validating its ID, priority, and cron expression.
    pub fn add(
        &mut self,
        id: &str,
        expression: &str,
        priority: u8,
        policy: Policy,
    ) -> Result<(), Error> {
        if id.is_empty()
            || id.len() > 16
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(Error::BadId);
        }
        if priority > 9 {
            return Err(Error::BadPriority);
        }
        if self.ids.contains(id) {
            return Err(Error::Duplicate);
        }
        let cron = Cron::parse(expression).map_err(Error::Cron)?;
        let owned_id = id.to_owned();
        self.ids.insert(owned_id.clone());
        self.jobs.push(Job {
            id: owned_id,
            cron,
            priority,
            policy,
            insertion: self.jobs.len(),
        });
        Ok(())
    }

    /// Emits scheduled occurrences in the interval ending at `now`.
    pub fn poll(&mut self, now: u64) -> Result<Vec<Occurrence>, Error> {
        if self.last_poll.is_some_and(|last| now < last) {
            return Err(Error::TimeReversed);
        }
        if now > 604_800_000 {
            return Err(Error::TimeRange);
        }

        let lower = self.last_poll;
        let mut emitted: Vec<(u64, u8, usize, Occurrence)> = Vec::new();

        for job in &self.jobs {
            let first_minute = lower.map_or(0, |last| last / 60_000 + 1);
            let last_minute = now / 60_000;
            let mut matches = Vec::new();
            if first_minute <= last_minute {
                for minute in first_minute..=last_minute {
                    if job.cron.matches(minute) {
                        matches.push(minute * 60_000);
                    }
                }
            }

            let chosen: Vec<u64> = match job.policy {
                Policy::All => matches,
                Policy::Once => matches.into_iter().rev().take(1).collect(),
                Policy::Skip => matches
                    .into_iter()
                    .filter(|&at| at == now && now % 60_000 == 0)
                    .collect(),
            };
            for at in chosen {
                emitted.push((
                    at,
                    job.priority,
                    job.insertion,
                    Occurrence {
                        id: job.id.clone(),
                        at,
                    },
                ));
            }
        }

        emitted.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        self.last_poll = Some(now);
        Ok(emitted.into_iter().map(|entry| entry.3).collect())
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_matches() {
        let cron = Cron::parse("5-15/5 2,4").unwrap();
        assert!(cron.matches(130));
        assert!(!cron.matches(131));
    }

    #[test]
    fn rejects_invalid_layout_and_bounds() {
        assert_eq!(Cron::parse("0  1"), Err(CronError::Layout(2)));
        assert_eq!(Cron::parse("60 0"), Err(CronError::Bounds(0)));
    }

    #[test]
    fn calendar_emits_and_advances_interval() {
        let mut calendar = Calendar::new();
        calendar
            .add("ticks", "*/1 0-23", 1, Policy::All)
            .unwrap();
        assert_eq!(
            calendar.poll(60_000).unwrap(),
            vec![
                Occurrence {
                    id: "ticks".into(),
                    at: 0,
                },
                Occurrence {
                    id: "ticks".into(),
                    at: 60_000,
                },
            ]
        );
        assert!(calendar.poll(60_000).unwrap().is_empty());
    }

    #[test]
    fn ordering_uses_priority() {
        let mut calendar = Calendar::new();
        calendar.add("low", "0-0 0-0", 0, Policy::All).unwrap();
        calendar.add("high", "0-0 0-0", 9, Policy::All).unwrap();
        assert_eq!(
            calendar
                .poll(0)
                .unwrap()
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>(),
            vec!["high", "low"]
        );
    }
}