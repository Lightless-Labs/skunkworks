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

        parse_field(&text[..split], 0, 60, &mut minutes)?;
        parse_field(&text[split + 1..], split + 1, 24, &mut hours)?;

        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_field(
    field: &str,
    offset: usize,
    limit: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let mut start = 0;
    loop {
        let end = field[start..]
            .find(',')
            .map(|n| start + n)
            .unwrap_or(field.len());
        let item = &field[start..end];
        parse_item(item, offset + start, limit, selected)?;

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
    limit: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let bytes = item.as_bytes();

    let slash = bytes.iter().position(|&b| b == b'/');
    if let Some(s) = slash {
        if let Some(second) = bytes[s + 1..].iter().position(|&b| b == b'/') {
            return Err(CronError::Syntax(offset + s + 1 + second));
        }
    }

    let base_end = slash.unwrap_or(bytes.len());
    let base = &item[..base_end];

    let step_allowed = base == "*" || base.contains('-');
    if slash.is_some() && !step_allowed {
        return Err(CronError::Syntax(offset + slash.unwrap()));
    }

    let (lo, hi, dash) = if base == "*" {
        (0usize, limit - 1, None)
    } else if let Some(d) = base.as_bytes().iter().position(|&b| b == b'-') {
        let left = parse_number(&base[..d], offset, limit)?;
        let right = parse_number(&base[d + 1..], offset + d + 1, limit)?;
        if left > right {
            return Err(CronError::Descending(offset + d));
        }
        (left, right, Some(d))
    } else {
        let n = parse_number(base, offset, limit)?;
        (n, n, None)
    };

    let step = if let Some(s) = slash {
        parse_step(&item[s + 1..], offset + s + 1, limit)?
    } else {
        1
    };

    let _ = dash;
    let mut value = lo;
    while value <= hi {
        selected[value] = true;
        match value.checked_add(step) {
            Some(next) if next > value => value = next,
            _ => break,
        }
    }

    Ok(())
}

fn parse_number(fragment: &str, offset: usize, limit: usize) -> Result<usize, CronError> {
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
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as usize)) {
            Some(v) => v,
            None => return Err(CronError::Bounds(offset)),
        };
    }
    if value >= limit {
        return Err(CronError::Bounds(offset));
    }
    Ok(value)
}

fn parse_step(fragment: &str, offset: usize, limit: usize) -> Result<usize, CronError> {
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
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as usize)) {
            Some(v) => v,
            None => return Err(CronError::Step(offset)),
        };
    }
    if value == 0 || value > limit {
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

        let lower = self.last_poll;
        let first_minute = match lower {
            None => 0,
            Some(last) => last / 60_000 + 1,
        };
        let last_minute = now / 60_000;
        let now_is_boundary = now % 60_000 == 0;
        let mut emitted: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let mut matching = Vec::new();
            if first_minute <= last_minute {
                for minute in first_minute..=last_minute {
                    let at = minute * 60_000;
                    if lower.map_or(true, |last| at > last) && job.cron.matches(minute) {
                        matching.push(at);
                    }
                }
            }

            match job.policy {
                Policy::All => {
                    for at in matching {
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
                Policy::Once => {
                    if let Some(at) = matching.last().copied() {
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
                Policy::Skip => {
                    if now_is_boundary && matching.last().copied() == Some(now) {
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