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
        parse_field(&bytes[..split], 0, 60, &mut minutes)?;
        parse_field(&bytes[split + 1..], split + 1, 24, &mut hours)?;

        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_field(
    field: &[u8],
    offset: usize,
    limit: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let mut start = 0;
    loop {
        let end = field[start..]
            .iter()
            .position(|&b| b == b',')
            .map_or(field.len(), |i| start + i);
        parse_item(&field[start..end], offset + start, limit, selected)?;
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
    limit: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let slash = item.iter().position(|&b| b == b'/');
    if let Some(first) = slash {
        if let Some(second_rel) = item[first + 1..].iter().position(|&b| b == b'/') {
            return Err(CronError::Syntax(offset + first + 1 + second_rel));
        }
    }

    let base_end = slash.unwrap_or(item.len());
    let base = &item[..base_end];

    let (range_start, range_end, dash_pos) = if base == b"*" {
        (0usize, limit - 1, None)
    } else if let Some(dash) = base.iter().position(|&b| b == b'-') {
        let left = parse_number(&base[..dash], offset, limit)?;
        let right = parse_number(&base[dash + 1..], offset + dash + 1, limit)?;
        if left > right {
            return Err(CronError::Descending(offset + dash));
        }
        (left, right, Some(dash))
    } else {
        if slash.is_some() {
            return Err(CronError::Syntax(offset + slash.unwrap()));
        }
        let number = parse_number(base, offset, limit)?;
        (number, number, None)
    };

    let step = if let Some(slash_pos) = slash {
        if base != b"*" && dash_pos.is_none() {
            return Err(CronError::Syntax(offset + slash_pos));
        }
        let step_start = slash_pos + 1;
        let step_limit = if limit == 60 { 60 } else { 24 };
        parse_step(&item[step_start..], offset + step_start, step_limit)?
    } else {
        1
    };

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

fn parse_number(fragment: &[u8], offset: usize, limit: usize) -> Result<usize, CronError> {
    if fragment.is_empty() {
        return Err(CronError::Syntax(offset));
    }
    for (i, &b) in fragment.iter().enumerate() {
        if !b.is_ascii_digit() {
            return Err(CronError::Syntax(offset + i));
        }
    }
    if fragment.len() > 1 && fragment[0] == b'0' {
        return Err(CronError::Number(offset));
    }

    let mut value = 0usize;
    for &b in fragment {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as usize))
        {
            Some(v) => v,
            None => return Err(CronError::Bounds(offset)),
        };
        if value >= limit {
            return Err(CronError::Bounds(offset));
        }
    }
    Ok(value)
}

fn parse_step(fragment: &[u8], offset: usize, max_step: usize) -> Result<usize, CronError> {
    if fragment.is_empty() {
        return Err(CronError::Syntax(offset));
    }
    for (i, &b) in fragment.iter().enumerate() {
        if !b.is_ascii_digit() {
            return Err(CronError::Syntax(offset + i));
        }
    }
    if fragment.len() > 1 && fragment[0] == b'0' {
        return Err(CronError::Number(offset));
    }

    let mut value = 0usize;
    for &b in fragment {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as usize))
        {
            Some(v) => v,
            None => return Err(CronError::Step(offset)),
        };
        if value > max_step {
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
        self.ids.push(id.to_owned());
        self.jobs.push(Job {
            id: id.to_owned(),
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

        let first_minute = self
            .last_poll
            .map_or(0, |last| last / 60_000 + 1);
        let last_minute = now / 60_000;
        let mut emitted: Vec<(Occurrence, u8, usize)> = Vec::new();

        if first_minute <= last_minute {
            for job in &self.jobs {
                match job.policy {
                    Policy::All => {
                        let mut minute = first_minute;
                        while minute <= last_minute {
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
                            minute += 1;
                        }
                    }
                    Policy::Once => {
                        let latest = (first_minute..=last_minute)
                            .rev()
                            .find(|&minute| job.cron.matches(minute));
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
                            if minute >= first_minute && job.cron.matches(minute) {
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