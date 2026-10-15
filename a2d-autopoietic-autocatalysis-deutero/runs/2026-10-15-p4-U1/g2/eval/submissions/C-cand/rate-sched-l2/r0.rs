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
        if split == 0 {
            return Err(CronError::Layout(0));
        }
        if split + 1 == text.len() {
            return Err(CronError::Layout(split));
        }

        let mut cron = Self {
            minutes: [false; 60],
            hours: [false; 24],
        };
        parse_field(&text[..split], 0, 59, &mut cron.minutes)?;
        parse_field(&text[split + 1..], split + 1, 23, &mut cron.hours)?;
        Ok(cron)
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_field(
    field: &str,
    offset: usize,
    max: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let mut start = 0;
    loop {
        let end = field[start..]
            .find(',')
            .map(|n| start + n)
            .unwrap_or(field.len());
        if start == end {
            return Err(CronError::Syntax(offset + start));
        }

        parse_item(&field[start..end], offset + start, max, selected)?;
        if end == field.len() {
            break;
        }
        start = end + 1;
    }
    Ok(())
}

fn parse_item(
    item: &str,
    offset: usize,
    max: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let bytes = item.as_bytes();
    let slash_positions: Vec<usize> = bytes
        .iter()
        .enumerate()
        .filter_map(|(i, b)| (*b == b'/').then_some(i))
        .collect();

    if slash_positions.len() > 1 {
        return Err(CronError::Syntax(offset + slash_positions[1]));
    }
    let slash = slash_positions.first().copied();
    let base_end = slash.unwrap_or(bytes.len());

    if let Some(s) = slash {
        if bytes.first() != Some(&b'*') && !bytes[..base_end].contains(&b'-') {
            return Err(CronError::Syntax(offset + s));
        }
    }

    let base = &item[..base_end];
    let (range_start, range_end, dash) = if base == "*" {
        (0usize, max, None)
    } else if let Some(dash) = base.as_bytes().iter().position(|b| *b == b'-') {
        let left = parse_number(&base[..dash], offset, max)?;
        let right = parse_number(&base[dash + 1..], offset + dash + 1, max)?;
        if left > right {
            return Err(CronError::Descending(offset + dash));
        }
        (left, right, Some(dash))
    } else {
        let number = parse_number(base, offset, max)?;
        (number, number, None)
    };

    let step = if let Some(s) = slash {
        let step_start = s + 1;
        let value = parse_digits(&item[step_start..], offset + step_start)?;
        let max_step = if max == 59 { 60 } else { 24 };
        if value == 0 || value > max_step {
            return Err(CronError::Step(offset + step_start));
        }
        value
    } else {
        1
    };

    let _ = dash;
    let mut n = range_start;
    while n <= range_end {
        selected[n] = true;
        match n.checked_add(step) {
            Some(next) if next > n => n = next,
            _ => break,
        }
    }
    Ok(())
}

fn parse_number(text: &str, offset: usize, max: usize) -> Result<usize, CronError> {
    let value = parse_digits(text, offset)?;
    if value > max {
        return Err(CronError::Bounds(offset));
    }
    Ok(value)
}

fn parse_digits(text: &str, offset: usize) -> Result<usize, CronError> {
    let bytes = text.as_bytes();
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
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as usize))
            .unwrap_or(usize::MAX);
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

        let owned_id = id.to_owned();
        let order = self.jobs.len();
        self.ids.push(owned_id.clone());
        self.jobs.push(Job {
            id: owned_id,
            cron,
            priority,
            policy,
            order,
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
            match job.policy {
                Policy::Skip => {
                    if now % 60_000 == 0
                        && boundary_in_interval(now, lower_exclusive, now)
                        && job.cron.matches(now / 60_000)
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
                Policy::Once => {
                    if let Some(at) = latest_match(&job.cron, lower_exclusive, now) {
                        emitted.push((
                            Occurrence {
                                id: job.id.clone(),
                                at,
                            },
                            job.priority,
                            job.order,
                        ));
                    }
                }
                Policy::All => {
                    let mut minute = first_minute_after(lower_exclusive);
                    while minute <= now / 60_000 {
                        let at = minute * 60_000;
                        if at <= now && boundary_in_interval(at, lower_exclusive, now)
                            && job.cron.matches(minute)
                        {
                            emitted.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at,
                                },
                                job.priority,
                                job.order,
                            ));
                        }
                        minute += 1;
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
        Ok(emitted.into_iter().map(|entry| entry.0).collect())
    }
}

fn boundary_in_interval(at: u64, lower_exclusive: Option<u64>, upper_inclusive: u64) -> bool {
    at <= upper_inclusive && lower_exclusive.is_none_or(|lower| at > lower)
}

fn first_minute_after(lower_exclusive: Option<u64>) -> u64 {
    match lower_exclusive {
        None => 0,
        Some(t) => t / 60_000 + 1,
    }
}

fn latest_match(cron: &Cron, lower_exclusive: Option<u64>, now: u64) -> Option<u64> {
    let max_minute = now / 60_000;
    let first = first_minute_after(lower_exclusive);
    if first > max_minute {
        return None;
    }
    let mut minute = max_minute;
    loop {
        let at = minute * 60_000;
        if boundary_in_interval(at, lower_exclusive, now) && cron.matches(minute) {
            return Some(at);
        }
        if minute == first {
            break;
        }
        minute -= 1;
    }
    None
}

fn main() {}