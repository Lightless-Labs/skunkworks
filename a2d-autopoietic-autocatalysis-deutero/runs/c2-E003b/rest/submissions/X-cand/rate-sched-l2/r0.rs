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
        let sep = spaces[0];
        if sep == 0 {
            return Err(CronError::Layout(sep));
        }
        if sep + 1 == text.len() {
            return Err(CronError::Layout(sep));
        }

        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        parse_field(&text[..sep], 0, 60, &mut minutes)?;
        parse_field(&text[sep + 1..], sep + 1, 24, &mut hours)?;

        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_field(
    field: &str,
    offset: usize,
    limit: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let mut start = 0;
    loop {
        let end = field[start..]
            .find(',')
            .map(|n| start + n)
            .unwrap_or(field.len());

        if start == end {
            return Err(CronError::Syntax(offset + start));
        }
        parse_item(&field[start..end], offset + start, limit, selected)?;

        if end == field.len() {
            break;
        }
        start = end + 1;
        if start == field.len() {
            return Err(CronError::Syntax(offset + start));
        }
    }
    Ok(())
}

fn parse_item(
    item: &str,
    offset: usize,
    limit: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let bytes = item.as_bytes();
    let slash = bytes.iter().position(|&b| b == b'/');
    if let Some(s) = slash {
        if let Some(second) = bytes[s + 1..].iter().position(|&b| b == b'/') {
            return Err(CronError::Syntax(offset + s + 1 + second));
        }
    }

    let base_end = slash.unwrap_or(bytes.len());
    let base = &item[..base_end];
    let step_start = slash.map(|s| s + 1);

    if let Some(s) = slash {
        if base != "*" && !base.contains('-') {
            return Err(CronError::Syntax(offset + s));
        }
    }

    let (range_start, range_end, dash) = if base == "*" {
        (0, limit - 1, None)
    } else if let Some(dash) = base.as_bytes().iter().position(|&b| b == b'-') {
        let left = parse_number(&base[..dash], offset, limit)?;
        let right = parse_number(&base[dash + 1..], offset + dash + 1, limit)?;
        if left > right {
            return Err(CronError::Descending(offset + dash));
        }
        (left, right, Some(dash))
    } else {
        let n = parse_number(base, offset, limit)?;
        (n, n, None)
    };
    let _ = dash;

    let step = if let Some(s) = step_start {
        let value = parse_number(&item[s..], offset + s, usize::MAX)?;
        if value == 0 || value > limit {
            return Err(CronError::Step(offset + s));
        }
        value
    } else {
        1
    };

    let mut n = range_start;
    while n <= range_end {
        selected[n] = true;
        match n.checked_add(step) {
            Some(next) => n = next,
            None => break,
        }
    }
    Ok(())
}

fn parse_number(fragment: &str, offset: usize, limit: usize) -> Result<usize, CronError> {
    let bytes = fragment.as_bytes();
    if bytes.is_empty() {
        return Err(CronError::Syntax(offset));
    }
    for (i, &b) in bytes.iter().enumerate() {
        if !b.is_ascii_digit() {
            return Err(CronError::Syntax(offset + i));
        }
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(CronError::Number(offset));
    }

    let mut value = 0usize;
    for &b in bytes {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as usize)) {
            Some(v) => v,
            None => return Err(CronError::Bounds(offset)),
        };
        if value > limit {
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
        if self.last_poll.is_some_and(|last| now < last) {
            return Err(Error::TimeReversed);
        }
        if now > 604_800_000 {
            return Err(Error::TimeRange);
        }

        let first_boundary = match self.last_poll {
            None => 0,
            Some(last) => last / 60_000 + 1,
        };
        let last_boundary = now / 60_000;
        let now_is_boundary = now % 60_000 == 0;
        let mut pending: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let mut matching = Vec::new();
            if first_boundary <= last_boundary {
                for boundary in first_boundary..=last_boundary {
                    if job.cron.matches(boundary) {
                        matching.push(boundary);
                    }
                }
            }

            let chosen: Vec<u64> = match job.policy {
                Policy::All => matching,
                Policy::Once => matching.last().copied().into_iter().collect(),
                Policy::Skip => {
                    if now_is_boundary
                        && last_boundary >= first_boundary
                        && job.cron.matches(last_boundary)
                    {
                        vec![last_boundary]
                    } else {
                        Vec::new()
                    }
                }
            };

            for boundary in chosen {
                pending.push((
                    Occurrence {
                        id: job.id.clone(),
                        at: boundary * 60_000,
                    },
                    job.priority,
                    job.order,
                ));
            }
        }

        pending.sort_by(|a, b| {
            a.0.at
                .cmp(&b.0.at)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        self.last_poll = Some(now);
        Ok(pending.into_iter().map(|(occurrence, _, _)| occurrence).collect())
    }
}