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
        parse_field(&text[..sep], 0, 60, 60, &mut minutes)?;
        parse_field(&text[sep + 1..], sep + 1, 24, 24, &mut hours)?;

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
    max_step: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let mut start = 0;
    loop {
        let end = field[start..]
            .find(',')
            .map(|i| start + i)
            .unwrap_or(field.len());

        parse_item(
            &field[start..end],
            offset + start,
            max,
            max_step,
            selected,
        )?;

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
    max_step: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let bytes = item.as_bytes();

    let slash = bytes.iter().position(|&b| b == b'/');
    if let Some(slash_at) = slash {
        if let Some(second) = bytes[slash_at + 1..].iter().position(|&b| b == b'/') {
            return Err(CronError::Syntax(offset + slash_at + 1 + second));
        }
    }

    let (base, step_part, slash_at) = match slash {
        Some(i) => (&item[..i], Some(&item[i + 1..]), Some(i)),
        None => (item, None, None),
    };

    let base_is_star = base == "*";
    let has_range = base.as_bytes().contains(&b'-');

    if slash_at.is_some() && !base_is_star && !has_range {
        return Err(CronError::Syntax(offset + slash_at.unwrap()));
    }

    let (range_start, range_end, dash_offset) = if base_is_star {
        (0, max - 1, None)
    } else if let Some(dash) = base.as_bytes().iter().position(|&b| b == b'-') {
        let left = parse_number(&base[..dash], offset, max)?;
        let right = parse_number(&base[dash + 1..], offset + dash + 1, max)?;
        if left > right {
            return Err(CronError::Descending(offset + dash));
        }
        (left, right, Some(dash))
    } else {
        let number = parse_number(base, offset, max)?;
        (number, number, None)
    };

    let step = if let Some(step_text) = step_part {
        let step_offset = offset + slash_at.unwrap() + 1;
        let value = parse_step_number(step_text, step_offset, max_step)?;
        if value == 0 || value > max_step {
            return Err(CronError::Step(step_offset));
        }
        value
    } else {
        1
    };

    let _ = dash_offset;
    let mut value = range_start;
    while value <= range_end {
        selected[value] = true;
        match value.checked_add(step) {
            Some(next) if next > value => value = next,
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
    if let Some(i) = bytes.iter().position(|b| !b.is_ascii_digit()) {
        return Err(CronError::Syntax(offset + i));
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
    }
    if value > max - 1 {
        return Err(CronError::Bounds(offset));
    }
    Ok(value)
}

fn parse_step_number(fragment: &str, offset: usize, max_step: usize) -> Result<usize, CronError> {
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
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as usize))
        {
            Some(v) => v,
            None => return Err(CronError::Step(offset)),
        };
    }
    if value == 0 || value > max_step {
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
        if self.reserved_ids.iter().any(|reserved| reserved == id) {
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
            let first_minute = match interval_start {
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
                    while minute <= last_minute {
                        if job.cron.matches(minute) {
                            emitted.push((
                                Occurrence {
                                    id: job.id.clone(),
                                    at: minute * 60_000,
                                },
                                job.priority,
                                job.order,
                            ));
                        }
                        minute += 1;
                    }
                }
                Policy::Once => {
                    let mut latest = None;
                    let mut minute = first_minute;
                    while minute <= last_minute {
                        if job.cron.matches(minute) {
                            latest = Some(minute);
                        }
                        minute += 1;
                    }
                    if let Some(minute) = latest {
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
                        if minute >= first_minute && minute <= last_minute && job.cron.matches(minute)
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