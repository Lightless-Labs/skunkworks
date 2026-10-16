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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    TimeReversed,
    TimeRange,
    BadId,
    BadPriority,
    Duplicate,
    Cron(CronError),
}

#[derive(Debug, Clone)]
pub struct Cron {
    minutes: [bool; 60],
    hours: [bool; 24],
}

fn parse_number(s: &str, start: usize, max: u32) -> Result<u32, CronError> {
    if s.is_empty() {
        return Err(CronError::Syntax(start));
    }
    for (i, b) in s.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(CronError::Syntax(start + i));
        }
    }
    if s.len() > 1 && s.as_bytes()[0] == b'0' {
        return Err(CronError::Number(start));
    }

    let mut n = 0u32;
    for b in s.bytes() {
        n = n.saturating_mul(10).saturating_add((b - b'0') as u32);
        if n > max {
            return Err(CronError::Bounds(start));
        }
    }
    Ok(n)
}

fn parse_field<const N: usize>(
    field: &str,
    offset: usize,
    max: u32,
    is_minute: bool,
) -> Result<[bool; N], CronError> {
    let mut selected = [false; N];
    let mut item_start = 0;

    loop {
        let item_end = field[item_start..]
            .find(',')
            .map(|i| item_start + i)
            .unwrap_or(field.len());
        let item = &field[item_start..item_end];
        let abs = offset + item_start;

        if item.is_empty() {
            return Err(CronError::Syntax(abs));
        }

        let slash_positions: Vec<usize> = item
            .bytes()
            .enumerate()
            .filter_map(|(i, b)| (b == b'/').then_some(i))
            .collect();

        if slash_positions.len() > 1 {
            return Err(CronError::Syntax(abs + slash_positions[1]));
        }

        let slash = slash_positions.first().copied();
        let base_end = slash.unwrap_or(item.len());
        let base = &item[..base_end];

        let dash_positions: Vec<usize> = base
            .bytes()
            .enumerate()
            .filter_map(|(i, b)| (b == b'-').then_some(i))
            .collect();

        let (start, end, dash) = if base == "*" {
            (0u32, max, None)
        } else if let Some(&d) = dash_positions.first() {
            let left = parse_number(&base[..d], abs, max)?;
            let right = parse_number(&base[d + 1..], abs + d + 1, max)?;
            if left > right {
                return Err(CronError::Descending(abs + d));
            }
            (left, right, Some(d))
        } else {
            let n = parse_number(base, abs, max)?;
            (n, n, None)
        };

        if let Some(&d) = dash_positions.get(1) {
            let _ = d;
            // The second dash is reported by numeric parsing of the right fragment.
        }

        let step = if let Some(slash_pos) = slash {
            if base != "*" && dash.is_none() {
                return Err(CronError::Syntax(abs + slash_pos));
            }
            let step_start = abs + slash_pos + 1;
            let step_text = &item[slash_pos + 1..];
            let step_max = if is_minute { 60 } else { 24 };
            let value = parse_number(step_text, step_start, step_max)
                .map_err(|e| match e {
                    CronError::Bounds(_) => CronError::Step(step_start),
                    other => other,
                })?;
            if value == 0 {
                return Err(CronError::Step(step_start));
            }
            value
        } else {
            1
        };

        let mut value = start;
        while value <= end {
            selected[value as usize] = true;
            match value.checked_add(step) {
                Some(next) if next > value => value = next,
                _ => break,
            }
        }

        if item_end == field.len() {
            break;
        }
        item_start = item_end + 1;
    }

    Ok(selected)
}

impl Cron {
    pub fn parse(text: &str) -> Result<Self, CronError> {
        if let Some((i, _)) = text
            .bytes()
            .enumerate()
            .find(|(_, b)| b.is_ascii_whitespace() && *b != b' ')
        {
            return Err(CronError::Layout(i));
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
        if split == 0 {
            return Err(CronError::Layout(split));
        }
        if split + 1 == text.len() {
            return Err(CronError::Layout(split));
        }

        let minutes = parse_field::<60>(&text[..split], 0, 59, true)?;
        let hours = parse_field::<24>(&text[split + 1..], split + 1, 23, false)?;
        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize]
            && self.hours[((minute / 60) % 24) as usize]
    }
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

        let first_minute = match self.last_poll {
            None => 0,
            Some(last) => last / 60_000 + 1,
        };
        let last_minute = now / 60_000;
        let mut emitted: Vec<(Occurrence, u8, usize)> = Vec::new();

        if first_minute <= last_minute {
            for job in &self.jobs {
                let mut matching = Vec::new();
                let mut minute = first_minute;
                while minute <= last_minute {
                    if job.cron.matches(minute) {
                        matching.push(minute);
                    }
                    match minute.checked_add(1) {
                        Some(next) => minute = next,
                        None => break,
                    }
                }

                match job.policy {
                    Policy::All => {
                        for minute in matching {
                            emitted.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: minute * 60_000,
                                },
                                job.priority,
                                job.order,
                            ));
                        }
                    }
                    Policy::Once => {
                        if let Some(minute) = matching.last().copied() {
                            emitted.push((
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
                        if now % 60_000 == 0 {
                            let minute = now / 60_000;
                            if job.cron.matches(minute)
                                && minute >= first_minute
                                && minute <= last_minute
                            {
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