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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    reserved_ids: Vec<String>,
    last_poll: Option<u64>,
}

impl Cron {
    pub fn parse(text: &str) -> Result<Self, CronError> {
        let bytes = text.as_bytes();

        for (i, &byte) in bytes.iter().enumerate() {
            if byte.is_ascii_whitespace() && byte != b' ' {
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
        if split == 0 {
            return Err(CronError::Layout(0));
        }
        if split + 1 == bytes.len() {
            return Err(CronError::Layout(split));
        }

        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        parse_field(&text[..split], 0, 59, &mut minutes)?;
        parse_field(&text[split + 1..], split + 1, 23, &mut hours)?;
        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize]
            && self.hours[((minute / 60) % 24) as usize]
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
            .map(|i| start + i)
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
    let mut slash = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'/' {
            if slash.is_some() {
                return Err(CronError::Syntax(offset + i));
            }
            slash = Some(i);
        }
    }

    let base_end = slash.unwrap_or(bytes.len());
    let base = &item[..base_end];

    if slash.is_some() && base != "*" && !base.contains('-') {
        return Err(CronError::Syntax(offset + slash.unwrap()));
    }

    let (range_start, range_end, dash_offset) = if base == "*" {
        (0, max, None)
    } else if let Some(dash) = base.find('-') {
        let left = parse_number(&base[..dash], offset, max)?;
        let right = parse_number(&base[dash + 1..], offset + dash + 1, max)?;
        if left > right {
            return Err(CronError::Descending(offset + dash));
        }
        (left, right, Some(dash))
    } else {
        let n = parse_number(base, offset, max)?;
        (n, n, None)
    };

    let step = if let Some(slash_pos) = slash {
        let step_offset = offset + slash_pos + 1;
        parse_step(&item[slash_pos + 1..], step_offset, max)?
    } else {
        1
    };

    let _ = dash_offset;
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

fn parse_step(fragment: &str, offset: usize, max: usize) -> Result<usize, CronError> {
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
            .and_then(|v| v.checked_add((b - b'0') as usize))
        {
            Some(v) => v,
            None => return Err(CronError::Step(offset)),
        };
        if value > max + 1 {
            return Err(CronError::Step(offset));
        }
    }
    if value == 0 || value > max + 1 {
        return Err(CronError::Step(offset));
    }
    Ok(value)
}

impl Calendar {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            reserved_ids: Vec::new(),
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
        if self.reserved_ids.iter().any(|existing| existing == id) {
            return Err(Error::Duplicate);
        }
        let cron = Cron::parse(expression).map_err(Error::Cron)?;

        let owned_id = id.to_owned();
        self.reserved_ids.push(owned_id.clone());
        self.jobs.push(Job {
            id: owned_id,
            cron,
            priority,
            policy,
            order: self.jobs.len(),
        });
        Ok(())
    }

    pub fn poll(&mut self, now: u64) -> Result<Vec<Occurrence>, Error> {
        if self.last_poll.is_some_and(|last| now < last) {
            return Err(Error::TimeReversed);
        }
        if now > 604_800_000 {
            return Err(Error::TimeRange);
        }

        let start_minute = match self.last_poll {
            Some(last) => last / 60_000 + 1,
            None => 0,
        };
        let end_minute = now / 60_000;
        let mut occurrences = Vec::new();

        if start_minute <= end_minute {
            for job in &self.jobs {
                let matching: Vec<u64> = (start_minute..=end_minute)
                    .filter(|&minute| job.cron.matches(minute))
                    .collect();

                let emitted: Vec<u64> = match job.policy {
                    Policy::All => matching,
                    Policy::Once => matching.last().copied().into_iter().collect(),
                    Policy::Skip => matching
                        .into_iter()
                        .filter(|&minute| minute.checked_mul(60_000) == Some(now))
                        .collect(),
                };

                for minute in emitted {
                    if let Some(at) = minute.checked_mul(60_000) {
                        occurrences.push((Occurrence {
                            id: job.id.clone(),
                            at,
                        }, job.priority, job.order));
                    }
                }
            }
        }

        occurrences.sort_by(|a, b| {
            a.0.at
                .cmp(&b.0.at)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        self.last_poll = Some(now);
        Ok(occurrences.into_iter().map(|(occurrence, _, _)| occurrence).collect())
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self::new()
    }
}