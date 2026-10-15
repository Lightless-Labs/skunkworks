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
        for (offset, byte) in text.bytes().enumerate() {
            if byte.is_ascii_whitespace() && byte != b' ' {
                return Err(CronError::Layout(offset));
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
        parse_field(&text[..split], 0, 59, &mut minutes)?;
        parse_field(&text[split + 1..], split + 1, 23, &mut hours)?;

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
    selected: &mut [bool],
) -> Result<(), CronError> {
    let mut start = 0;
    loop {
        let end = field[start..]
            .find(',')
            .map(|i| start + i)
            .unwrap_or(field.len());
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
    if item.is_empty() {
        return Err(CronError::Syntax(offset));
    }

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
    let base = &item[..base_end];

    if let Some(slash_at) = slash {
        if base != "*" && !base.contains('-') {
            return Err(CronError::Syntax(offset + slash_at));
        }
    }

    let (range_start, range_end, dash) = if base == "*" {
        (0, max, None)
    } else if let Some(dash_at) = base.find('-') {
        let left = parse_number(&base[..dash_at], offset, max)?;
        let right = parse_number(
            &base[dash_at + 1..],
            offset + dash_at + 1,
            max,
        )?;
        if left > right {
            return Err(CronError::Descending(offset + dash_at));
        }
        (left, right, Some(dash_at))
    } else {
        let number = parse_number(base, offset, max)?;
        (number, number, None)
    };

    let step = if let Some(slash_at) = slash {
        let step_start = slash_at + 1;
        let value = parse_unbounded_number(&item[step_start..], offset + step_start)?;
        let step_max = if max == 59 { 60 } else { 24 };
        if value == 0 || value > step_max {
            return Err(CronError::Step(offset + step_start));
        }
        value
    } else {
        1
    };

    let _ = dash;
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

fn parse_number(fragment: &str, offset: usize, max: usize) -> Result<usize, CronError> {
    let value = parse_unbounded_number(fragment, offset)?;
    if value > max {
        return Err(CronError::Bounds(offset));
    }
    Ok(value)
}

fn parse_unbounded_number(fragment: &str, offset: usize) -> Result<usize, CronError> {
    if fragment.is_empty() {
        return Err(CronError::Syntax(offset));
    }

    for (i, byte) in fragment.bytes().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(CronError::Syntax(offset + i));
        }
    }

    if fragment.len() > 1 && fragment.as_bytes()[0] == b'0' {
        return Err(CronError::Number(offset));
    }

    let mut value = 0usize;
    for byte in fragment.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as usize))
        {
            Some(v) => v,
            None => return Err(CronError::Bounds(offset)),
        };
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
    insertion_order: usize,
    first_minute: u64,
}

pub struct Calendar {
    jobs: Vec<Job>,
    last_poll: Option<u64>,
    reserved_ids: Vec<String>,
}

impl Calendar {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            last_poll: None,
            reserved_ids: Vec::new(),
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

        let first_minute = self
            .last_poll
            .map(|time| time / 60_000 + 1)
            .unwrap_or(0);
        let insertion_order = self.jobs.len();

        self.reserved_ids.push(id.to_owned());
        self.jobs.push(Job {
            id: id.to_owned(),
            cron,
            priority,
            policy,
            insertion_order,
            first_minute,
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

        let interval_start = self.last_poll;
        let mut emitted: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            match job.policy {
                Policy::All => {
                    let mut minute = job.first_minute;
                    loop {
                        let at = minute.saturating_mul(60_000);
                        if at > now {
                            break;
                        }
                        let after_previous = interval_start
                            .map(|last| at > last)
                            .unwrap_or(true);
                        if after_previous && job.cron.matches(minute) {
                            emitted.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at,
                                },
                                job.priority,
                                job.insertion_order,
                            ));
                        }
                        minute += 1;
                    }
                }
                Policy::Once => {
                    let upper_minute = now / 60_000;
                    let mut minute = job.first_minute;
                    let mut latest = None;
                    while minute <= upper_minute {
                        let at = minute * 60_000;
                        let after_previous = interval_start
                            .map(|last| at > last)
                            .unwrap_or(true);
                        if after_previous && job.cron.matches(minute) {
                            latest = Some(at);
                        }
                        minute += 1;
                    }
                    if let Some(at) = latest {
                        emitted.push((
                            Occurrence {
                                id: job.id.clone(),
                                at,
                            },
                            job.priority,
                            job.insertion_order,
                        ));
                    }
                }
                Policy::Skip => {
                    if now % 60_000 == 0 {
                        let minute = now / 60_000;
                        let after_previous = interval_start
                            .map(|last| now > last)
                            .unwrap_or(true);
                        if minute >= job.first_minute
                            && after_previous
                            && job.cron.matches(minute)
                        {
                            emitted.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: now,
                                },
                                job.priority,
                                job.insertion_order,
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