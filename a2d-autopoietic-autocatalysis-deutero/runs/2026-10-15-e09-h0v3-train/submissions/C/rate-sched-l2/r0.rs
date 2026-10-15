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

fn numeric_value(
    text: &str,
    start: usize,
    end: usize,
    limit: u32,
    error_kind: u8,
) -> Result<u32, CronError> {
    let bytes = text.as_bytes();
    if start == end {
        return Err(CronError::Syntax(start));
    }
    for i in start..end {
        if !bytes[i].is_ascii_digit() {
            return Err(CronError::Syntax(i));
        }
    }
    if end - start > 1 && bytes[start] == b'0' {
        return Err(CronError::Number(start));
    }

    let mut value = 0u32;
    for i in start..end {
        value = value
            .saturating_mul(10)
            .saturating_add((bytes[i] - b'0') as u32);
        if value > limit {
            return Err(if error_kind == 0 {
                CronError::Bounds(start)
            } else {
                CronError::Step(start)
            });
        }
    }
    Ok(value)
}

fn parse_field(
    text: &str,
    field_start: usize,
    field_end: usize,
    maximum: u32,
    step_maximum: u32,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let bytes = text.as_bytes();
    let mut item_start = field_start;

    loop {
        let mut item_end = item_start;
        while item_end < field_end && bytes[item_end] != b',' {
            item_end += 1;
        }

        if item_start == item_end {
            return Err(CronError::Syntax(item_start));
        }

        let mut slash = None;
        for i in item_start..item_end {
            if bytes[i] == b'/' {
                if slash.is_some() {
                    return Err(CronError::Syntax(i));
                }
                slash = Some(i);
            }
        }

        let base_end = slash.unwrap_or(item_end);
        let step_start = slash.map(|i| i + 1);

        if slash.is_some() {
            let has_range = bytes[item_start..base_end].contains(&b'-');
            if bytes[item_start] != b'*' && !has_range {
                return Err(CronError::Syntax(slash.unwrap()));
            }
        }

        let (range_start, range_end) = if item_start < base_end && bytes[item_start] == b'*' {
            if item_start + 1 != base_end {
                let bad = item_start + 1;
                return Err(CronError::Syntax(bad));
            }
            (0, maximum)
        } else {
            let mut dash = None;
            for i in item_start..base_end {
                if bytes[i] == b'-' && dash.is_none() {
                    dash = Some(i);
                }
            }
            if let Some(dash_pos) = dash {
                let left = numeric_value(text, item_start, dash_pos, maximum, 0)?;
                let right = numeric_value(text, dash_pos + 1, base_end, maximum, 0)?;
                if left > right {
                    return Err(CronError::Descending(dash_pos));
                }
                (left, right)
            } else {
                let value = numeric_value(text, item_start, base_end, maximum, 0)?;
                (value, value)
            }
        };

        let step = if let Some(start) = step_start {
            let value = numeric_value(text, start, item_end, step_maximum, 1)?;
            if value == 0 {
                return Err(CronError::Step(start));
            }
            value
        } else {
            1
        };

        let mut value = range_start;
        while value <= range_end {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        if item_end == field_end {
            break;
        }
        item_start = item_end + 1;
    }

    Ok(())
}

impl Cron {
    pub fn parse(text: &str) -> Result<Self, CronError> {
        let bytes = text.as_bytes();

        for (i, byte) in bytes.iter().enumerate() {
            if byte.is_ascii_whitespace() && *byte != b' ' {
                return Err(CronError::Layout(i));
            }
        }

        let spaces: Vec<usize> = bytes
            .iter()
            .enumerate()
            .filter_map(|(i, b)| (*b == b' ').then_some(i))
            .collect();

        if spaces.is_empty() {
            return Err(CronError::Layout(text.len()));
        }
        if spaces.len() > 1 {
            return Err(CronError::Layout(spaces[1]));
        }
        let separator = spaces[0];
        if separator == 0 || separator + 1 == text.len() {
            return Err(CronError::Layout(separator));
        }

        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        parse_field(text, 0, separator, 59, 60, &mut minutes)?;
        parse_field(
            text,
            separator + 1,
            text.len(),
            23,
            24,
            &mut hours,
        )?;

        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize]
            && self.hours[((minute / 60) % 24) as usize]
    }
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

        let interval_start = self.last_poll;
        let mut scheduled: Vec<(u64, u8, usize, Occurrence)> = Vec::new();

        for job in &self.jobs {
            match job.policy {
                Policy::All => {
                    let mut at = match interval_start {
                        None => 0,
                        Some(last) => (last / 60_000 + 1) * 60_000,
                    };
                    while at <= now {
                        if at % 60_000 == 0 && job.cron.matches(at / 60_000) {
                            scheduled.push((
                                at,
                                job.priority,
                                job.order,
                                Occurrence {
                                    id: job.id.clone(),
                                    at,
                                },
                            ));
                        }
                        match at.checked_add(60_000) {
                            Some(next) => at = next,
                            None => break,
                        }
                    }
                }
                Policy::Once => {
                    let mut latest = None;
                    let mut at = match interval_start {
                        None => 0,
                        Some(last) => (last / 60_000 + 1) * 60_000,
                    };
                    while at <= now {
                        if job.cron.matches(at / 60_000) {
                            latest = Some(at);
                        }
                        match at.checked_add(60_000) {
                            Some(next) => at = next,
                            None => break,
                        }
                    }
                    if let Some(at) = latest {
                        scheduled.push((
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
                    if now % 60_000 == 0
                        && interval_start.map_or(true, |last| now > last)
                        && job.cron.matches(now / 60_000)
                    {
                        scheduled.push((
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

        scheduled.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        self.last_poll = Some(now);
        Ok(scheduled.into_iter().map(|entry| entry.3).collect())
    }
}

fn main() {}