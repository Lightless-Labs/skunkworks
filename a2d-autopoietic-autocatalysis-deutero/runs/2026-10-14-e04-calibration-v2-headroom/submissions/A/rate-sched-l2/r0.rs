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

        let separator = spaces[0];
        if separator == 0 || separator + 1 == bytes.len() {
            return Err(CronError::Layout(separator));
        }

        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        parse_field(&bytes[..separator], 0, 60, &mut minutes)?;
        parse_field(&bytes[separator + 1..], separator + 1, 24, &mut hours)?;

        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_field(
    field: &[u8],
    offset: usize,
    upper: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let mut start = 0;
    loop {
        let end = field[start..]
            .iter()
            .position(|&b| b == b',')
            .map_or(field.len(), |relative| start + relative);

        if start == end {
            return Err(CronError::Syntax(offset + start));
        }

        parse_item(&field[start..end], offset + start, upper, selected)?;

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
    upper: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let slash = item.iter().position(|&b| b == b'/');
    if let Some(first_slash) = slash {
        if let Some(second_relative) = item[first_slash + 1..].iter().position(|&b| b == b'/') {
            return Err(CronError::Syntax(offset + first_slash + 1 + second_relative));
        }
    }

    let base_end = slash.unwrap_or(item.len());
    let base = &item[..base_end];
    let step_offset = slash.map(|s| offset + s + 1);

    let is_star = base == b"*";
    let dash = base.iter().position(|&b| b == b'-');

    if slash.is_some() && !is_star && dash.is_none() {
        return Err(CronError::Syntax(offset + slash.unwrap()));
    }

    let (range_start, range_end) = if is_star {
        (0, upper - 1)
    } else if let Some(dash_at) = dash {
        let left = parse_number(&base[..dash_at], offset, upper)?;
        let right = parse_number(&base[dash_at + 1..], offset + dash_at + 1, upper)?;
        if left > right {
            return Err(CronError::Descending(offset + dash_at));
        }
        (left, right)
    } else {
        let n = parse_number(base, offset, upper)?;
        (n, n)
    };

    let step = if let Some(step_start) = step_offset {
        let value = parse_number(&item[slash.unwrap() + 1..], step_start, usize::MAX)?;
        if value == 0 || value > upper {
            return Err(CronError::Step(step_start));
        }
        value
    } else {
        1
    };

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

fn parse_number(fragment: &[u8], offset: usize, upper: usize) -> Result<usize, CronError> {
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

    let mut value = 0usize;
    for &b in fragment {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as usize))
        {
            Some(v) => v,
            None => {
                return Err(if upper == usize::MAX {
                    CronError::Step(offset)
                } else {
                    CronError::Bounds(offset)
                });
            }
        };
    }

    if value > upper {
        return Err(if upper == usize::MAX {
            CronError::Step(offset)
        } else {
            CronError::Bounds(offset)
        });
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
        let insertion_order = self.jobs.len();
        self.jobs.push(Job {
            id: id.to_owned(),
            cron,
            priority,
            policy,
            insertion_order,
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
        let first_minute = match lower_exclusive {
            None => 0,
            Some(last) => last / 60_000 + 1,
        };
        let last_minute = now / 60_000;

        let mut emitted: Vec<(Occurrence, u8, usize)> = Vec::new();
        if first_minute <= last_minute {
            for job in &self.jobs {
                match job.policy {
                    Policy::All => {
                        let mut minute = first_minute;
                        while minute <= last_minute {
                            if job.cron.matches(minute) {
                                emitted.push((
                                    Occurrence {
                                        id: job.id.clone(),
                                        at: minute * 60_000,
                                    },
                                    job.priority,
                                    job.insertion_order,
                                ));
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
                                emitted.push((
                                    Occurrence {
                                        id: job.id.clone(),
                                        at: minute * 60_000,
                                    },
                                    job.priority,
                                    job.insertion_order,
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
                        if now % 60_000 == 0
                            && now / 60_000 >= first_minute
                            && job.cron.matches(now / 60_000)
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