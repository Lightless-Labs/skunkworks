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
        Self::parse_field(&text[..separator], 0, 59, &mut minutes)?;
        Self::parse_field(&text[separator + 1..], separator + 1, 23, &mut hours)?;

        Ok(Self { minutes, hours })
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
                .map(|n| start + n)
                .unwrap_or(field.len());
            let item = &field[start..end];
            if item.is_empty() {
                return Err(CronError::Syntax(offset + start));
            }

            Self::parse_item(item, offset + start, max, selected)?;

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

        let slash = bytes.iter().position(|&b| b == b'/');
        if let Some(first_slash) = slash {
            if let Some(second) = bytes[first_slash + 1..]
                .iter()
                .position(|&b| b == b'/')
            {
                return Err(CronError::Syntax(offset + first_slash + 1 + second));
            }
        }

        let base_end = slash.unwrap_or(bytes.len());
        let base = &item[..base_end];
        let step = if let Some(s) = slash {
            if base != "*" && !base.contains('-') {
                return Err(CronError::Syntax(offset + s));
            }
            Some((&item[s + 1..], offset + s + 1))
        } else {
            None
        };

        let (range_start, range_end, is_star, dash_offset) = if base == "*" {
            (0usize, max, true, None)
        } else if let Some(dash) = base.as_bytes().iter().position(|&b| b == b'-') {
            let left = Self::parse_number(&base[..dash], offset, max)?;
            let right = Self::parse_number(&base[dash + 1..], offset + dash + 1, max)?;
            if left > right {
                return Err(CronError::Descending(offset + dash));
            }
            (left, right, false, Some(dash))
        } else {
            let n = Self::parse_number(base, offset, max)?;
            (n, n, false, None)
        };

        let stride = if let Some((fragment, step_offset)) = step {
            Self::parse_step(fragment, step_offset, max)?
        } else {
            1
        };

        let _ = dash_offset;
        let mut value = range_start;
        while value <= range_end {
            selected[value] = true;
            match value.checked_add(stride) {
                Some(next) if next > value => value = next,
                _ => break,
            }
        }
        let _ = is_star;
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
        if bytes.is_empty() {
            return Err(CronError::Syntax(offset));
        }
        if let Some(i) = bytes.iter().position(|b| !b.is_ascii_digit()) {
            return Err(CronError::Syntax(offset + i));
        }
        if bytes.len() > 1 && bytes[0] == b'0' {
            return Err(CronError::Number(offset));
        }

        let upper = if max == 59 { 60 } else { 24 };
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
        if value == 0 || value > upper {
            return Err(CronError::Step(offset));
        }
        Ok(value)
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
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
    reserved: Vec<String>,
    last_poll: Option<u64>,
}

impl Calendar {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            reserved: Vec::new(),
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

        let lower = self.last_poll;
        let mut emitted: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            match job.policy {
                Policy::Skip => {
                    if now % 60_000 == 0
                        && lower.is_none_or(|last| now > last)
                        && job.cron.matches(now / 60_000)
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
                Policy::Once => {
                    if let Some(at) = latest_match(&job.cron, lower, now) {
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
                Policy::All => {
                    let start = lower
                        .map(|last| last / 60_000 + 1)
                        .unwrap_or(0);
                    let end = now / 60_000;
                    if start <= end {
                        for minute in start..=end {
                            let at = minute * 60_000;
                            if job.cron.matches(minute) {
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
        Ok(emitted.into_iter().map(|(occurrence, _, _)| occurrence).collect())
    }
}

fn latest_match(cron: &Cron, lower: Option<u64>, now: u64) -> Option<u64> {
    let end = now / 60_000;
    let start = lower.map(|last| last / 60_000 + 1).unwrap_or(0);
    if start > end {
        return None;
    }

    let mut minute = end;
    loop {
        if minute >= start && cron.matches(minute) {
            return Some(minute * 60_000);
        }
        if minute == start {
            break;
        }
        minute -= 1;
    }
    None
}

fn main() {}