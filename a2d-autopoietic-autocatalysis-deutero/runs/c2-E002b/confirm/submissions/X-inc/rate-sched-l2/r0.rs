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

        // Layout validation is performed over the complete input first.
        for (i, &b) in bytes.iter().enumerate() {
            if b.is_ascii_whitespace() && b != b' ' {
                return Err(CronError::Layout(i));
            }
        }

        let spaces: Vec<usize> = bytes
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| if b == b' ' { Some(i) } else { None })
            .collect();

        if spaces.is_empty() {
            return Err(CronError::Layout(text.len()));
        }
        if spaces.len() > 1 {
            return Err(CronError::Layout(spaces[1]));
        }
        let split = spaces[0];
        if split == 0 {
            return Err(CronError::Layout(split));
        }
        if split + 1 == bytes.len() {
            return Err(CronError::Layout(split));
        }

        let mut minutes = [false; 60];
        let mut hours = [false; 24];

        parse_field(&bytes[..split], 0, 60, &mut minutes)?;
        parse_field(&bytes[split + 1..], split + 1, 24, &mut hours)?;

        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize]
            && self.hours[((minute / 60) % 24) as usize]
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
            .map(|i| start + i)
            .unwrap_or(field.len());

        if start == end {
            return Err(CronError::Syntax(offset + start));
        }

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
    let slash_positions: Vec<usize> = item
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b == b'/' { Some(i) } else { None })
        .collect();

    if slash_positions.len() > 1 {
        return Err(CronError::Syntax(offset + slash_positions[1]));
    }

    let slash = slash_positions.first().copied();
    let base_end = slash.unwrap_or(item.len());

    if let Some(slash_at) = slash {
        let base = &item[..base_end];
        if base != b"*" && !base.contains(&b'-') {
            return Err(CronError::Syntax(offset + slash_at));
        }
    }

    let base = &item[..base_end];
    let (range_start, range_end, dash_pos) = if base == b"*" {
        (0, limit - 1, None)
    } else if let Some(dash) = base.iter().position(|&b| b == b'-') {
        let left = parse_number(&base[..dash], offset, limit)?;
        let right = parse_number(&base[dash + 1..], offset + dash + 1, limit)?;
        if left > right {
            return Err(CronError::Descending(offset + dash));
        }
        (left, right, Some(dash))
    } else {
        let n = parse_number(base, offset, limit)?;
        (n, n, None)
    };

    let step = if let Some(slash_at) = slash {
        parse_step(
            &item[slash_at + 1..],
            offset + slash_at + 1,
            limit,
        )?
    } else {
        1
    };

    let _ = dash_pos;
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

    let mut value: usize = 0;
    for &b in fragment {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as usize)) {
            Some(v) => v,
            None => return Err(CronError::Bounds(offset)),
        };
        if value >= limit {
            return Err(CronError::Bounds(offset));
        }
    }

    Ok(value)
}

fn parse_step(fragment: &[u8], offset: usize, limit: usize) -> Result<usize, CronError> {
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

    let mut value: usize = 0;
    for &b in fragment {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as usize)) {
            Some(v) => v,
            None => return Err(CronError::Step(offset)),
        };
        if value > 60 {
            return Err(CronError::Step(offset));
        }
    }

    let max_step = if limit == 60 { 60 } else { 24 };
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
        let order = self.jobs.len();
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

        let lower_exclusive = self.last_poll;
        let upper_minute = now / 60_000;
        let mut ranked: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let first_minute = match lower_exclusive {
                Some(last) => last / 60_000 + 1,
                None => 0,
            };

            if first_minute > upper_minute {
                continue;
            }

            match job.policy {
                Policy::All => {
                    let mut minute = first_minute;
                    loop {
                        if job.cron.matches(minute) {
                            ranked.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: minute * 60_000,
                                },
                                job.priority,
                                job.order,
                            ));
                        }
                        if minute == upper_minute {
                            break;
                        }
                        minute += 1;
                    }
                }
                Policy::Once => {
                    let mut latest = None;
                    let mut minute = first_minute;
                    loop {
                        if job.cron.matches(minute) {
                            latest = Some(minute);
                        }
                        if minute == upper_minute {
                            break;
                        }
                        minute += 1;
                    }
                    if let Some(minute) = latest {
                        ranked.push((
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
                    if now % 60_000 == 0
                        && upper_minute >= first_minute
                        && job.cron.matches(upper_minute)
                    {
                        ranked.push((
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

        ranked.sort_by(|a, b| {
            a.0.at
                .cmp(&b.0.at)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        self.last_poll = Some(now);
        Ok(ranked.into_iter().map(|entry| entry.0).collect())
    }
}