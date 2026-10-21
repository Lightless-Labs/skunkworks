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
        parse_field(text, 0, split, 60, 60, &mut minutes)?;
        parse_field(text, split + 1, text.len(), 24, 24, &mut hours)?;
        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_field(
    text: &str,
    start: usize,
    end: usize,
    max_value: usize,
    max_step: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let bytes = text.as_bytes();
    let mut item_start = start;
    loop {
        let item_end = bytes[item_start..end]
            .iter()
            .position(|&b| b == b',')
            .map(|n| item_start + n)
            .unwrap_or(end);
        parse_item(text, item_start, item_end, max_value, max_step, selected)?;
        if item_end == end {
            break;
        }
        item_start = item_end + 1;
    }
    Ok(())
}

fn parse_item(
    text: &str,
    start: usize,
    end: usize,
    max_value: usize,
    max_step: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let b = text.as_bytes();
    if start == end {
        return Err(CronError::Syntax(start));
    }

    let mut slash = None;
    for i in start..end {
        if b[i] == b'/' {
            if slash.is_some() {
                return Err(CronError::Syntax(i));
            }
            slash = Some(i);
        }
    }

    let base_end = slash.unwrap_or(end);
    let step_start = slash.map(|i| i + 1);
    let base_is_star = base_end == start + 1 && b[start] == b'*';
    let dash = (start..base_end).find(|&i| b[i] == b'-');
    let is_range = dash.is_some();

    if slash.is_some() && !base_is_star && !is_range {
        return Err(CronError::Syntax(slash.unwrap()));
    }

    let (lo, hi) = if base_is_star {
        (0, max_value - 1)
    } else if let Some(d) = dash {
        let left = parse_number(text, start, d, max_value)?;
        let right = parse_number(text, d + 1, base_end, max_value)?;
        if left > right {
            return Err(CronError::Descending(d));
        }
        (left, right)
    } else {
        let n = parse_number(text, start, base_end, max_value)?;
        (n, n)
    };

    let step = if let Some(s) = step_start {
        parse_step(text, s, end, max_step)?
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

fn parse_number(text: &str, start: usize, end: usize, limit: usize) -> Result<usize, CronError> {
    let b = text.as_bytes();
    if start == end {
        return Err(CronError::Syntax(start));
    }
    for i in start..end {
        if !b[i].is_ascii_digit() {
            return Err(CronError::Syntax(i));
        }
    }
    if end - start > 1 && b[start] == b'0' {
        return Err(CronError::Number(start));
    }
    let mut value = 0usize;
    for i in start..end {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b[i] - b'0') as usize)) {
            Some(v) => v,
            None => return Err(CronError::Bounds(start)),
        };
    }
    if value >= limit {
        return Err(CronError::Bounds(start));
    }
    Ok(value)
}

fn parse_step(text: &str, start: usize, end: usize, max_step: usize) -> Result<usize, CronError> {
    let b = text.as_bytes();
    if start == end {
        return Err(CronError::Syntax(start));
    }
    for i in start..end {
        if !b[i].is_ascii_digit() {
            return Err(CronError::Syntax(i));
        }
    }
    if end - start > 1 && b[start] == b'0' {
        return Err(CronError::Number(start));
    }
    let mut value = 0usize;
    for i in start..end {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b[i] - b'0') as usize)) {
            Some(v) => v,
            None => return Err(CronError::Step(start)),
        };
    }
    if value == 0 || value > max_step {
        return Err(CronError::Step(start));
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
        self.ids.push(owned_id.clone());
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
        let mut out: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let first = match lower_exclusive {
                Some(last) => last / 60_000 + 1,
                None => 0,
            };
            if first > upper_minute {
                continue;
            }
            let matching = |m: u64| job.cron.matches(m);
            match job.policy {
                Policy::Skip => {
                    if now % 60_000 == 0
                        && now / 60_000 >= first
                        && matching(now / 60_000)
                    {
                        out.push((
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
                    if let Some(m) = (first..=upper_minute).rev().find(|&m| matching(m)) {
                        out.push((
                            Occurrence {
                                id: job.id.clone(),
                                at: m * 60_000,
                            },
                            job.priority,
                            job.order,
                        ));
                    }
                }
                Policy::All => {
                    for m in first..=upper_minute {
                        if matching(m) {
                            out.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: m * 60_000,
                                },
                                job.priority,
                                job.order,
                            ));
                        }
                    }
                }
            }
        }

        out.sort_by(|a, b| {
            a.0.at
                .cmp(&b.0.at)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        self.last_poll = Some(now);
        Ok(out.into_iter().map(|entry| entry.0).collect())
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self::new()
    }
}