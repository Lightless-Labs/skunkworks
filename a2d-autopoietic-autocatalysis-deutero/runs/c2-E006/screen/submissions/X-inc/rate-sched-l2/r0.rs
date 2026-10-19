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
        let sep = spaces[0];
        if sep == 0 || sep + 1 == bytes.len() {
            return Err(CronError::Layout(sep));
        }

        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        parse_field(&bytes[..sep], 0, 60, &mut minutes)?;
        parse_field(&bytes[sep + 1..], sep + 1, 24, &mut hours)?;

        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
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
            .map_or(field.len(), |p| start + p);
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
        .filter_map(|(i, &b)| (b == b'/').then_some(i))
        .collect();

    if slash_positions.len() > 1 {
        return Err(CronError::Syntax(offset + slash_positions[1]));
    }

    let slash = slash_positions.first().copied();
    let base_end = slash.unwrap_or(item.len());

    if item[..base_end].is_empty() {
        return Err(CronError::Syntax(offset));
    }

    let base = &item[..base_end];
    let dash = base.iter().position(|&b| b == b'-');
    let is_star = base == b"*";

    if slash.is_some() && !is_star && dash.is_none() {
        return Err(CronError::Syntax(offset + slash.unwrap()));
    }

    let (range_start, range_end, dash_at) = if is_star {
        (0usize, limit - 1, None)
    } else if let Some(d) = dash {
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

    let step = if let Some(slash_pos) = slash {
        let step_start = slash_pos + 1;
        let value = parse_number(&item[step_start..], offset + step_start, usize::MAX)?;
        let max_step = if limit == 60 { 60 } else { 24 };
        if value == 0 || value > max_step {
            return Err(CronError::Step(offset + step_start));
        }
        value
    } else {
        1
    };

    let _ = dash_at;
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

fn parse_number(fragment: &[u8], offset: usize, limit: usize) -> Result<usize, CronError> {
    if fragment.is_empty() {
        return Err(CronError::Syntax(offset));
    }

    if let Some(i) = fragment.iter().position(|b| !b.is_ascii_digit()) {
        return Err(CronError::Syntax(offset + i));
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
            None => return Err(CronError::Bounds(offset)),
        };
        if value >= limit {
            return Err(CronError::Bounds(offset));
        }
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
    first_minute: u64,
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
        let first_minute = self.last_poll.map_or(0, |t| t / 60_000 + 1);
        let order = self.jobs.len();
        self.ids.push(id.to_owned());
        self.jobs.push(Job {
            id: id.to_owned(),
            cron,
            priority,
            policy,
            order,
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

        let lower_exclusive = self.last_poll;
        let max_minute = now / 60_000;
        let mut emitted: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let start = job.first_minute.max(
                lower_exclusive.map_or(0, |t| t / 60_000 + 1),
            );
            if start > max_minute {
                continue;
            }

            match job.policy {
                Policy::All => {
                    for minute in start..=max_minute {
                        let at = minute * 60_000;
                        if lower_exclusive.map_or(true, |last| at > last)
                            && at <= now
                            && job.cron.matches(minute)
                        {
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
                }
                Policy::Once => {
                    let mut latest = None;
                    for minute in start..=max_minute {
                        let at = minute * 60_000;
                        if lower_exclusive.map_or(true, |last| at > last)
                            && at <= now
                            && job.cron.matches(minute)
                        {
                            latest = Some(at);
                        }
                    }
                    if let Some(at) = latest {
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
                    if now % 60_000 == 0 {
                        let minute = now / 60_000;
                        if minute >= start
                            && lower_exclusive.map_or(true, |last| now > last)
                            && job.cron.matches(minute)
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