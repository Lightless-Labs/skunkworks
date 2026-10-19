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

        if let Some(i) = bytes
            .iter()
            .position(|&b| b.is_ascii_whitespace() && b != b' ')
        {
            return Err(CronError::Layout(i));
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

    let bytes = item.as_bytes();
    let slash = bytes.iter().position(|&b| b == b'/');
    if let Some(s) = slash {
        if let Some(second) = bytes[s + 1..].iter().position(|&b| b == b'/') {
            return Err(CronError::Syntax(offset + s + 1 + second));
        }
    }

    let base_end = slash.unwrap_or(bytes.len());
    let base = &item[..base_end];

    if slash.is_some() && base != "*" && !base.contains('-') {
        return Err(CronError::Syntax(offset + slash.unwrap()));
    }

    let (range_start, range_end, dash) = if base == "*" {
        (0, max, None)
    } else if let Some(dash_index) = base.as_bytes().iter().position(|&b| b == b'-') {
        let left = parse_number(&base[..dash_index], offset, max)?;
        let right = parse_number(
            &base[dash_index + 1..],
            offset + dash_index + 1,
            max,
        )?;
        if left > right {
            return Err(CronError::Descending(offset + dash_index));
        }
        (left, right, Some(dash_index))
    } else {
        let n = parse_number(base, offset, max)?;
        (n, n, None)
    };

    let step = if let Some(slash_index) = slash {
        let step_start = slash_index + 1;
        parse_step(&item[step_start..], offset + step_start, max)?
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

fn parse_number(fragment: &str, offset: usize, max: usize) -> Result<usize, CronError> {
    let bytes = fragment.as_bytes();
    if let Some(i) = bytes.iter().position(|b| !b.is_ascii_digit()) {
        return Err(CronError::Syntax(offset + i));
    }
    if bytes.is_empty() {
        return Err(CronError::Syntax(offset));
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(CronError::Number(offset));
    }

    let mut value = 0usize;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as usize))
            .ok_or(CronError::Bounds(offset))?;
    }

    if value > max {
        return Err(CronError::Bounds(offset));
    }
    Ok(value)
}

fn parse_step(fragment: &str, offset: usize, max: usize) -> Result<usize, CronError> {
    let bytes = fragment.as_bytes();
    if let Some(i) = bytes.iter().position(|b| !b.is_ascii_digit()) {
        return Err(CronError::Syntax(offset + i));
    }
    if bytes.is_empty() {
        return Err(CronError::Syntax(offset));
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(CronError::Number(offset));
    }

    let mut value = 0usize;
    for &b in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as usize))
            .ok_or(CronError::Step(offset))?;
    }

    if value == 0 || value > max + 1 {
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
    reserved: Vec<String>,
}

impl Calendar {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            last_poll: None,
            reserved: Vec::new(),
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
        if self.reserved.iter().any(|existing| existing == id) {
            return Err(Error::Duplicate);
        }
        let cron = Cron::parse(expression).map_err(Error::Cron)?;

        let id = id.to_owned();
        self.reserved.push(id.clone());
        self.jobs.push(Job {
            id,
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

        const MINUTE_MS: u64 = 60_000;
        let first_minute = match self.last_poll {
            None => 0,
            Some(last) => last / MINUTE_MS + 1,
        };
        let last_minute = now / MINUTE_MS;
        let now_is_boundary = now % MINUTE_MS == 0;

        let mut tagged: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            match job.policy {
                Policy::All => {
                    if first_minute <= last_minute {
                        for minute in first_minute..=last_minute {
                            if job.cron.matches(minute) {
                                tagged.push((
                                    Occurrence {
                                        id: job.id.clone(),
                                        at: minute * MINUTE_MS,
                                    },
                                    job.priority,
                                    job.order,
                                ));
                            }
                        }
                    }
                }
                Policy::Once => {
                    if first_minute <= last_minute {
                        if let Some(minute) = (first_minute..=last_minute)
                            .rev()
                            .find(|&minute| job.cron.matches(minute))
                        {
                            tagged.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: minute * MINUTE_MS,
                                },
                                job.priority,
                                job.order,
                            ));
                        }
                    }
                }
                Policy::Skip => {
                    if now_is_boundary {
                        let minute = now / MINUTE_MS;
                        if minute >= first_minute && minute <= last_minute && job.cron.matches(minute) {
                            tagged.push((
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

        tagged.sort_by(|a, b| {
            a.0.at
                .cmp(&b.0.at)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        self.last_poll = Some(now);
        Ok(tagged.into_iter().map(|(occurrence, _, _)| occurrence).collect())
    }
}