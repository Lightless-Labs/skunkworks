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

impl Cron {
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
        parse_field(&bytes[..split], 0, 60, 60, &mut minutes)?;
        parse_field(&bytes[split + 1..], split + 1, 24, 24, &mut hours)?;
        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_field(
    field: &[u8],
    offset: usize,
    max_value: usize,
    max_step: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let mut start = 0;
    loop {
        let end = field[start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| start + p)
            .unwrap_or(field.len());
        if start == end {
            return Err(CronError::Syntax(offset + start));
        }

        parse_item(
            &field[start..end],
            offset + start,
            max_value,
            max_step,
            selected,
        )?;

        if end == field.len() {
            break;
        }
        start = end + 1;
    }
    Ok(())
}

fn parse_item(
    item: &[u8],
    offset: usize,
    max_value: usize,
    max_step: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let slash = item.iter().position(|&b| b == b'/');
    if let Some(s) = slash {
        if let Some(second) = item[s + 1..].iter().position(|&b| b == b'/') {
            return Err(CronError::Syntax(offset + s + 1 + second));
        }
    }

    let base_end = slash.unwrap_or(item.len());
    let base = &item[..base_end];

    if slash.is_some() && base != b"*" && !base.contains(&b'-') {
        return Err(CronError::Syntax(offset + slash.unwrap()));
    }

    let (range_start, range_end) = if base == b"*" {
        (0, max_value - 1)
    } else if let Some(dash) = base.iter().position(|&b| b == b'-') {
        let left = parse_number(&base[..dash], offset, max_value)?;
        let right = parse_number(&base[dash + 1..], offset + dash + 1, max_value)?;
        if left > right {
            return Err(CronError::Descending(offset + dash));
        }
        (left, right)
    } else {
        let value = parse_number(base, offset, max_value)?;
        (value, value)
    };

    let step = if let Some(slash_pos) = slash {
        parse_step(
            &item[slash_pos + 1..],
            offset + slash_pos + 1,
            max_step,
        )?
    } else {
        1
    };

    let mut value = range_start;
    while value <= range_end {
        selected[value] = true;
        match value.checked_add(step) {
            Some(next) => value = next,
            None => break,
        }
    }
    Ok(())
}

fn parse_number(fragment: &[u8], offset: usize, max_value: usize) -> Result<usize, CronError> {
    if fragment.is_empty() {
        return Err(CronError::Syntax(offset));
    }

    if let Some(i) = fragment.iter().position(|b| !b.is_ascii_digit()) {
        return Err(CronError::Syntax(offset + i));
    }

    if fragment.len() > 1 && fragment[0] == b'0' {
        return Err(CronError::Number(offset));
    }

    let mut value = 0usize;
    for &b in fragment {
        value = value
            .saturating_mul(10)
            .saturating_add((b - b'0') as usize);
        if value > max_value {
            return Err(CronError::Bounds(offset));
        }
    }
    if value > max_value {
        return Err(CronError::Bounds(offset));
    }
    Ok(value)
}

fn parse_step(fragment: &[u8], offset: usize, max_step: usize) -> Result<usize, CronError> {
    if fragment.is_empty() {
        return Err(CronError::Syntax(offset));
    }

    if let Some(i) = fragment.iter().position(|b| !b.is_ascii_digit()) {
        return Err(CronError::Syntax(offset + i));
    }

    if fragment.len() > 1 && fragment[0] == b'0' {
        return Err(CronError::Number(offset));
    }

    let mut value = 0usize;
    for &b in fragment {
        value = value
            .saturating_mul(10)
            .saturating_add((b - b'0') as usize);
        if value > max_step {
            return Err(CronError::Step(offset));
        }
    }
    if value == 0 || value > max_step {
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
    added_after: Option<u64>,
}

pub struct Calendar {
    jobs: Vec<Job>,
    ids: Vec<String>,
    last_poll: Option<u64>,
}

impl Calendar {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            ids: Vec::new(),
            last_poll: None,
        }
    }

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
        if self.ids.iter().any(|existing| existing == id) {
            return Err(Error::Duplicate);
        }
        let cron = Cron::parse(expression).map_err(Error::Cron)?;

        let order = self.jobs.len();
        let added_after = self.last_poll;
        self.ids.push(id.to_owned());
        self.jobs.push(Job {
            id: id.to_owned(),
            cron,
            priority,
            policy,
            order,
            added_after,
        });
        Ok(())
    }

    pub fn poll(&mut self, now: u64) -> Result<Vec<Occurrence>, Error> {
        if let Some(last) = self.last_poll {
            if now < last {
                return Err(Error::TimeReversed);
            }
        }
        if now > 604_800_000 {
            return Err(Error::TimeRange);
        }

        let lower_exclusive = self.last_poll;
        let mut emitted: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let interval_start = match (lower_exclusive, job.added_after) {
                (Some(last), Some(added)) => Some(last.max(added)),
                (Some(last), None) => Some(last),
                (None, Some(added)) => Some(added),
                (None, None) => None,
            };

            let first_minute = match interval_start {
                Some(t) => t / 60_000 + 1,
                None => 0,
            };
            let last_minute = now / 60_000;

            if first_minute > last_minute {
                continue;
            }

            match job.policy {
                Policy::All => {
                    for minute in first_minute..=last_minute {
                        if job.cron.matches(minute) {
                            emitted.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: minute * 60_000,
                                },
                                job.priority,
                                job.order,
                            ));
                        }
                    }
                }
                Policy::Once => {
                    let mut latest = None;
                    for minute in first_minute..=last_minute {
                        if job.cron.matches(minute) {
                            latest = Some(minute);
                        }
                    }
                    if let Some(minute) = latest {
                        emitted.push((
                            Occurrence {
                                id: job.id.clone(),
                                at: minute * 60_000,
                            },
                            job.priority,
                            job.order,
                        ));
                    }
                }
                Policy::Skip => {
                    if now % 60_000 == 0 {
                        let minute = now / 60_000;
                        if minute >= first_minute
                            && minute <= last_minute
                            && job.cron.matches(minute)
                        {
                            emitted.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: now,
                                },
                                job.priority,
                                job.order,
                            ));
                        }
                    }
                }
            }
        }

        emitted.sort_by(|a, b| {
            a.0.at
                .cmp(&b.0.at)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        self.last_poll = Some(now);
        Ok(emitted.into_iter().map(|item| item.0).collect())
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {}