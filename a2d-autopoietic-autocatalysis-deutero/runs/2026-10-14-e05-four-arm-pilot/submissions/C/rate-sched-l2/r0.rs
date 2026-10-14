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

pub struct Cron {
    minutes: [bool; 60],
    hours: [bool; 24],
}

impl Cron {
    /// Parses a two-field minute and hour cron expression.
    pub fn parse(text: &str) -> Result<Self, CronError> {
        let bytes = text.as_bytes();

        for (i, &b) in bytes.iter().enumerate() {
            if b.is_ascii_whitespace() && b != b' ' {
                return Err(CronError::Layout(i));
            }
        }

        let spaces: Vec<usize> = bytes
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b' ').then_some(i))
            .collect();

        if spaces.is_empty() {
            return Err(CronError::Layout(text.len()));
        }
        if spaces.len() > 1 {
            return Err(CronError::Layout(spaces[1]));
        }
        let split = spaces[0];
        if split == 0 || split + 1 == bytes.len() {
            return Err(CronError::Layout(split));
        }

        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        parse_field(&text[..split], 0, 59, 60, &mut minutes)?;
        parse_field(&text[split + 1..], split + 1, 23, 24, &mut hours)?;

        Ok(Self { minutes, hours })
    }

    /// Returns whether an absolute minute matches this cron expression.
    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_field(
    field: &str,
    offset: usize,
    max: usize,
    step_max: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let mut item_start = 0;
    loop {
        let item_end = field[item_start..]
            .find(',')
            .map(|n| item_start + n)
            .unwrap_or(field.len());
        if item_start == item_end {
            return Err(CronError::Syntax(offset + item_start));
        }
        parse_item(
            &field[item_start..item_end],
            offset + item_start,
            max,
            step_max,
            selected,
        )?;
        if item_end == field.len() {
            break;
        }
        item_start = item_end + 1;
    }
    Ok(())
}

fn parse_item(
    item: &str,
    offset: usize,
    max: usize,
    step_max: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let slash_positions: Vec<usize> = item
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| (b == b'/').then_some(i))
        .collect();

    if slash_positions.len() > 1 {
        return Err(CronError::Syntax(offset + slash_positions[1]));
    }
    let slash = slash_positions.first().copied();
    let base_end = slash.unwrap_or(item.len());

    if let Some(slash_at) = slash {
        let base = &item[..base_end];
        if base != "*" && !base.contains('-') {
            return Err(CronError::Syntax(offset + slash_at));
        }
    }

    let base = &item[..base_end];
    let (start, end, dash) = if base == "*" {
        (0, max, None)
    } else if let Some(dash_at) = base.bytes().position(|b| b == b'-') {
        let left = parse_number(&base[..dash_at], offset, max)?;
        let right_start = dash_at + 1;
        let right = parse_number(&base[right_start..], offset + right_start, max)?;
        if left > right {
            return Err(CronError::Descending(offset + dash_at));
        }
        (left, right, Some(dash_at))
    } else {
        let n = parse_number(base, offset, max)?;
        (n, n, None)
    };

    let step = if let Some(slash_at) = slash {
        let step_start = slash_at + 1;
        parse_step(&item[step_start..], offset + step_start, step_max)?
    } else {
        1
    };

    let _ = dash;
    let mut value = start;
    while value <= end {
        selected[value] = true;
        match value.checked_add(step) {
            Some(next) if next > value => value = next,
            _ => break,
        }
    }
    Ok(())
}

fn parse_number(fragment: &str, offset: usize, max: usize) -> Result<usize, CronError> {
    let bytes = fragment.as_bytes();
    if bytes.is_empty() {
        return Err(CronError::Syntax(offset));
    }
    if let Some(i) = bytes.iter().position(|b| !b.is_ascii_digit()) {
        return Err(CronError::Syntax(offset + i));
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(CronError::Number(offset));
    }
    let mut value = 0usize;
    for &b in bytes {
        value = value
            .saturating_mul(10)
            .saturating_add((b - b'0') as usize);
        if value > max {
            return Err(CronError::Bounds(offset));
        }
    }
    Ok(value)
}

fn parse_step(fragment: &str, offset: usize, max: usize) -> Result<usize, CronError> {
    let bytes = fragment.as_bytes();
    if bytes.is_empty() {
        return Err(CronError::Syntax(offset));
    }
    if let Some(i) = bytes.iter().position(|b| !b.is_ascii_digit()) {
        return Err(CronError::Syntax(offset + i));
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(CronError::Number(offset));
    }
    let mut value = 0usize;
    for &b in bytes {
        value = value
            .saturating_mul(10)
            .saturating_add((b - b'0') as usize);
        if value > max {
            return Err(CronError::Step(offset));
        }
    }
    if value == 0 {
        return Err(CronError::Step(offset));
    }
    Ok(value)
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
    order: usize,
}

pub struct Calendar {
    jobs: Vec<Job>,
    ids: HashSet<String>,
    last_poll: Option<u64>,
}

impl Calendar {
    /// Creates an empty calendar with no poll watermark.
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            ids: HashSet::new(),
            last_poll: None,
        }
    }

    /// Adds a job after validating its identifier, priority, and cron expression.
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
        let owned_id = id.to_string();
        self.ids.insert(owned_id.clone());
        self.jobs.push(Job {
            id: owned_id,
            cron,
            priority,
            policy,
            order: self.jobs.len(),
        });
        Ok(())
    }

    /// Emits scheduled occurrences in the interval ending at `now`.
    pub fn poll(&mut self, now: u64) -> Result<Vec<Occurrence>, Error> {
        if let Some(last) = self.last_poll {
            if now < last {
                return Err(Error::TimeReversed);
            }
        }
        if now > 604_800_000 {
            return Err(Error::TimeRange);
        }

        let start_exclusive = self.last_poll;
        let last_minute = now / 60_000;
        let mut gathered: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let first_minute = match start_exclusive {
                Some(last) => last / 60_000 + 1,
                None => 0,
            };
            let mut matching = Vec::new();
            if first_minute <= last_minute {
                for minute in first_minute..=last_minute {
                    if job.cron.matches(minute) {
                        matching.push(minute * 60_000);
                    }
                }
            }

            let selected: Vec<u64> = match job.policy {
                Policy::All => matching,
                Policy::Once => matching.last().copied().into_iter().collect(),
                Policy::Skip => matching
                    .into_iter()
                    .filter(|&at| at == now)
                    .collect(),
            };

            for at in selected {
                gathered.push((
                    Occurrence {
                        id: job.id.clone(),
                        at,
                    },
                    job.priority,
                    job.order,
                ));
            }
        }

        gathered.sort_by(|a, b| {
            a.0.at
                .cmp(&b.0.at)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        self.last_poll = Some(now);
        Ok(gathered.into_iter().map(|entry| entry.0).collect())
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_normal_and_edge_cases() {
        let cron = Cron::parse("5-15/5 2,4").unwrap();
        assert!(cron.matches(130));
        assert!(!cron.matches(131));
        assert_eq!(Cron::parse("00 0"), Err(CronError::Number(0)));
        assert!(Cron::parse("0-59 0-23").unwrap().matches(u64::MAX));
    }

    #[test]
    fn calendar_polling_policies_and_order() {
        let mut calendar = Calendar::new();
        calendar.add("low", "*/1 0-23", 0, Policy::All).unwrap();
        calendar.add("high", "*/1 0-23", 9, Policy::Once).unwrap();
        assert_eq!(
            calendar.poll(60_000).unwrap(),
            vec![
                Occurrence { id: "low".into(), at: 0 },
                Occurrence { id: "high".into(), at: 60_000 },
                Occurrence { id: "low".into(), at: 60_000 },
            ]
        );
    }

    #[test]
    fn failed_add_and_poll_leave_state_unchanged() {
        let mut calendar = Calendar::new();
        assert_eq!(calendar.add("Bad", "* *", 1, Policy::All), Err(Error::BadId));
        calendar.add("job", "0-0 0-0", 1, Policy::All).unwrap();
        assert_eq!(calendar.poll(604_800_001), Err(Error::TimeRange));
        assert_eq!(calendar.poll(0).unwrap().len(), 1);
        assert!(calendar.poll(0).unwrap().is_empty());
    }
}