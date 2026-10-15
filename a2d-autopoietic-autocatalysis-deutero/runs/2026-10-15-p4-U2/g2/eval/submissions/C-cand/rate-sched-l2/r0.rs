#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronError {
    Layout(usize),
    Syntax(usize),
    Number(usize),
    Bounds(usize),
    Descending(usize),
    Step(usize),
}

#[derive(Debug, Clone)]
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

        parse_field(&bytes[..split], 0, 59, &mut minutes)?;
        parse_field(&bytes[split + 1..], split + 1, 23, &mut hours)?;

        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_field(
    field: &[u8],
    offset: usize,
    max: usize,
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

        parse_item(&field[start..end], offset + start, max, selected)?;

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
    max: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let slash_positions: Vec<usize> = item
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'/').then_some(i))
        .collect();

    if slash_positions.len() > 1 {
        return Err(CronError::Syntax(offset + slash_positions[1]));
    }

    let (base_end, step_start) = if let Some(&slash) = slash_positions.first() {
        (slash, Some(slash + 1))
    } else {
        (item.len(), None)
    };

    let base = &item[..base_end];
    let dash = base.iter().position(|&b| b == b'-');

    if step_start.is_some() && base != b"*" && dash.is_none() {
        return Err(CronError::Syntax(offset + base_end));
    }

    let (range_start, range_end) = if base == b"*" {
        (0, max)
    } else if let Some(dash_pos) = dash {
        let left = parse_number(&base[..dash_pos], offset, max)?;
        let right = parse_number(&base[dash_pos + 1..], offset + dash_pos + 1, max)?;
        if left > right {
            return Err(CronError::Descending(offset + dash_pos));
        }
        (left, right)
    } else {
        let value = parse_number(base, offset, max)?;
        (value, value)
    };

    let step = if let Some(step_start) = step_start {
        let value = parse_number(&item[step_start..], offset + step_start, usize::MAX)?;
        let limit = if max == 59 { 60 } else { 24 };
        if value == 0 || value > limit {
            return Err(CronError::Step(offset + step_start));
        }
        value
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

fn parse_number(fragment: &[u8], offset: usize, max: usize) -> Result<usize, CronError> {
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
        if value > max {
            return Err(CronError::Bounds(offset));
        }
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
    last_poll: Option<u64>,
}

impl Calendar {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
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
        if self.jobs.iter().any(|job| job.id == id) {
            return Err(Error::Duplicate);
        }

        let cron = Cron::parse(expression).map_err(Error::Cron)?;
        self.jobs.push(Job {
            id: id.to_owned(),
            cron,
            priority,
            policy,
            order: self.jobs.len(),
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
        let mut emitted: Vec<(u64, u8, usize, Occurrence)> = Vec::new();

        for job in &self.jobs {
            let first_minute = match lower_exclusive {
                Some(last) => last / 60_000 + 1,
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
                            let at = minute * 60_000;
                            emitted.push((
                                at,
                                job.priority,
                                job.order,
                                Occurrence {
                                    id: job.id.clone(),
                                    at,
                                },
                            ));
                        }
                    }
                }
                Policy::Once => {
                    if let Some(minute) = (first_minute..=last_minute)
                        .rev()
                        .find(|&minute| job.cron.matches(minute))
                    {
                        let at = minute * 60_000;
                        emitted.push((
                            at,
                            job.priority,
                            job.order,
                            Occurrence {
                                id: job.id.clone(),
                                at,
                            },
                        ));
                    }
                }
                Policy::Skip => {
                    if now % 60_000 == 0 {
                        let minute = now / 60_000;
                        if minute >= first_minute && job.cron.matches(minute) {
                            emitted.push((
                                now,
                                job.priority,
                                job.order,
                                Occurrence {
                                    id: job.id.clone(),
                                    at: now,
                                },
                            ));
                        }
                    }
                }
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