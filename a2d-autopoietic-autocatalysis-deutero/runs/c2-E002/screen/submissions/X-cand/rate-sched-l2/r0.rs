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
        if split == 0 {
            return Err(CronError::Layout(0));
        }
        if split + 1 == text.len() {
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
    let bytes = field.as_bytes();
    let mut start = 0;

    loop {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b',')
            .map(|p| start + p)
            .unwrap_or(bytes.len());

        parse_item(&field[start..end], offset + start, max, selected)?;

        if end == bytes.len() {
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

    let slash = bytes.iter().position(|&b| b == b'/');
    if let Some(s) = slash {
        if let Some(second) = bytes[s + 1..].iter().position(|&b| b == b'/') {
            return Err(CronError::Syntax(offset + s + 1 + second));
        }
    }

    let (base_end, step_start) = match slash {
        Some(s) => (s, Some(s + 1)),
        None => (bytes.len(), None),
    };
    let base = &item[..base_end];

    let is_star = base == "*";
    let dash = base.as_bytes().iter().position(|&b| b == b'-');
    if step_start.is_some() && !is_star && dash.is_none() {
        return Err(CronError::Syntax(offset + base_end));
    }

    let (range_start, range_end) = if is_star {
        (0, max)
    } else if let Some(d) = dash {
        let left = parse_number(&base[..d], offset, max)?;
        let right = parse_number(&base[d + 1..], offset + d + 1, max)?;
        if left > right {
            return Err(CronError::Descending(offset + d));
        }
        (left, right)
    } else {
        let n = parse_number(base, offset, max)?;
        (n, n)
    };

    let step = if let Some(s) = step_start {
        parse_step(&item[s..], offset + s, max)?
    } else {
        1
    };

    let mut value = range_start;
    loop {
        selected[value] = true;
        if range_end - value < step {
            break;
        }
        value += step;
    }
    Ok(())
}

fn parse_number(fragment: &str, offset: usize, max: usize) -> Result<usize, CronError> {
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

    let mut value: usize = 0;
    for &b in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as usize))
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

fn parse_step(fragment: &str, offset: usize, max: usize) -> Result<usize, CronError> {
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

    let mut value: usize = 0;
    for &b in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as usize))
        {
            Some(v) => v,
            None => return Err(CronError::Step(offset)),
        };
    }

    let step_max = if max == 59 { 60 } else { 24 };
    if value == 0 || value > step_max {
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

        let id = id.to_owned();
        self.ids.push(id.clone());
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

        let lower = self.last_poll;
        let mut occurrences: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let first_minute = match lower {
                None => 0,
                Some(last) => last / 60_000 + 1,
            };
            let last_minute = now / 60_000;
            if first_minute > last_minute {
                continue;
            }

            match job.policy {
                Policy::All => {
                    let mut minute = first_minute;
                    loop {
                        if job.cron.matches(minute) {
                            occurrences.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: minute * 60_000,
                                },
                                job.priority,
                                job.order,
                            ));
                        }
                        if minute == last_minute {
                            break;
                        }
                        minute += 1;
                    }
                }
                Policy::Once => {
                    let mut minute = last_minute;
                    loop {
                        if minute < first_minute {
                            break;
                        }
                        if job.cron.matches(minute) {
                            occurrences.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: minute * 60_000,
                                },
                                job.priority,
                                job.order,
                            ));
                            break;
                        }
                        if minute == 0 {
                            break;
                        }
                        minute -= 1;
                    }
                }
                Policy::Skip => {
                    if now % 60_000 == 0 {
                        let minute = now / 60_000;
                        if minute >= first_minute && job.cron.matches(minute) {
                            occurrences.push((
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

        occurrences.sort_by(|a, b| {
            a.0.at
                .cmp(&b.0.at)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        self.last_poll = Some(now);
        Ok(occurrences.into_iter().map(|entry| entry.0).collect())
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self::new()
    }
}