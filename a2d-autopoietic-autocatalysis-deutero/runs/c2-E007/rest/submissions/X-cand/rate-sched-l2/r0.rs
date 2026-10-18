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

#[derive(Clone)]
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
        if split == 0 || split + 1 == text.len() {
            return Err(CronError::Layout(split));
        }

        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        parse_field(&text[..split], 0, 59, 60, &mut minutes)?;
        parse_field(&text[split + 1..], split + 1, 23, 24, &mut hours)?;

        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_field(
    field: &str,
    offset: usize,
    max: usize,
    step_limit: usize,
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

        parse_item(
            &field[start..end],
            offset + start,
            max,
            step_limit,
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
    item: &str,
    offset: usize,
    max: usize,
    step_limit: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let bytes = item.as_bytes();

    let slash = bytes.iter().position(|&b| b == b'/');
    if let Some(s) = slash {
        if let Some(second_relative) = bytes[s + 1..].iter().position(|&b| b == b'/') {
            return Err(CronError::Syntax(offset + s + 1 + second_relative));
        }
    }

    let (base_end, step_part) = match slash {
        Some(s) => (s, Some(&item[s + 1..])),
        None => (item.len(), None),
    };
    let base = &item[..base_end];

    if step_part.is_some() && base != "*" && !base.contains('-') {
        return Err(CronError::Syntax(offset + base_end));
    }

    let (lo, hi) = if base == "*" {
        (0, max)
    } else if let Some(dash) = base.as_bytes().iter().position(|&b| b == b'-') {
        let left = parse_number(&base[..dash], offset, max)?;
        let right_start = dash + 1;
        let right = parse_number(&base[right_start..], offset + right_start, max)?;
        if left > right {
            return Err(CronError::Descending(offset + dash));
        }
        (left, right)
    } else {
        let n = parse_number(base, offset, max)?;
        (n, n)
    };

    let step = if let Some(part) = step_part {
        let step_offset = offset + base_end + 1;
        let value = parse_step(part, step_offset, step_limit)?;
        value
    } else {
        1
    };

    let mut n = lo;
    while n <= hi {
        selected[n] = true;
        match n.checked_add(step) {
            Some(next) => n = next,
            None => break,
        }
    }

    Ok(())
}

fn parse_step(fragment: &str, offset: usize, limit: usize) -> Result<usize, CronError> {
    let bytes = fragment.as_bytes();

    if bytes.is_empty() {
        return Err(CronError::Syntax(offset));
    }
    for (i, &b) in bytes.iter().enumerate() {
        if !b.is_ascii_digit() {
            return Err(CronError::Syntax(offset + i));
        }
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(CronError::Number(offset));
    }

    let mut value = 0usize;
    for &b in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as usize))
        {
            Some(v) => v,
            None => return Err(CronError::Step(offset)),
        };
    }

    if value == 0 || value > limit {
        return Err(CronError::Step(offset));
    }
    Ok(value)
}

fn parse_number(fragment: &str, offset: usize, max: usize) -> Result<usize, CronError> {
    let bytes = fragment.as_bytes();

    if bytes.is_empty() {
        return Err(CronError::Syntax(offset));
    }
    for (i, &b) in bytes.iter().enumerate() {
        if !b.is_ascii_digit() {
            return Err(CronError::Syntax(offset + i));
        }
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(CronError::Number(offset));
    }

    let mut value = 0usize;
    for &b in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as usize))
        {
            Some(v) => v,
            None => return Err(CronError::Bounds(offset)),
        };
    }

    if value > max {
        return Err(CronError::Bounds(offset));
    }
    Ok(value)
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
    first_poll: Option<u64>,
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
            insertion: self.jobs.len(),
            first_poll: self.last_poll,
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

        let lower = self.last_poll;
        let mut emitted: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let interval_start = match (lower, job.first_poll) {
                (Some(last), _) => last,
                (None, Some(first)) => first,
                (None, None) => 0,
            };
            let min_boundary = if lower.is_some() || job.first_poll.is_some() {
                interval_start / 60_000 + 1
            } else {
                0
            };
            let max_boundary = now / 60_000;

            if min_boundary > max_boundary {
                continue;
            }

            match job.policy {
                Policy::All => {
                    for boundary in min_boundary..=max_boundary {
                        if job.cron.matches(boundary) {
                            emitted.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: boundary * 60_000,
                                },
                                job.priority,
                                job.insertion,
                            ));
                        }
                    }
                }
                Policy::Once => {
                    let mut latest = None;
                    for boundary in min_boundary..=max_boundary {
                        if job.cron.matches(boundary) {
                            latest = Some(boundary);
                        }
                    }
                    if let Some(boundary) = latest {
                        emitted.push((
                            Occurrence {
                                id: job.id.clone(),
                                at: boundary * 60_000,
                            },
                            job.priority,
                            job.insertion,
                        ));
                    }
                }
                Policy::Skip => {
                    if now % 60_000 == 0 {
                        let boundary = now / 60_000;
                        if boundary >= min_boundary
                            && boundary <= max_boundary
                            && job.cron.matches(boundary)
                        {
                            emitted.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: now,
                                },
                                job.priority,
                                job.insertion,
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
        Ok(emitted.into_iter().map(|entry| entry.0).collect())
    }
}