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
        for (i, byte) in text.bytes().enumerate() {
            if (byte.is_ascii_whitespace() || byte == 0x0B) && byte != b' ' {
                return Err(CronError::Layout(i));
            }
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
        if split == 0 || split + 1 == text.len() {
            return Err(CronError::Layout(split));
        }

        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        parse_field(&text[..split], 0, 60, &mut minutes)?;
        parse_field(&text[split + 1..], split + 1, 24, &mut hours)?;
        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize] && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_number(fragment: &str, start: usize, max: usize) -> Result<usize, CronError> {
    if fragment.is_empty() {
        return Err(CronError::Syntax(start));
    }
    for (i, b) in fragment.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(CronError::Syntax(start + i));
        }
    }
    if fragment.len() > 1 && fragment.as_bytes()[0] == b'0' {
        return Err(CronError::Number(start));
    }

    let mut value = 0usize;
    for b in fragment.bytes() {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as usize)) {
            Some(v) => v,
            None => return Err(CronError::Bounds(start)),
        };
    }
    if value > max {
        return Err(CronError::Bounds(start));
    }
    Ok(value)
}

fn parse_field(
    field: &str,
    offset: usize,
    max: usize,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let mut item_start = 0;
    loop {
        let item_end = field[item_start..]
            .find(',')
            .map(|n| item_start + n)
            .unwrap_or(field.len());
        let item = &field[item_start..item_end];
        let base_offset = offset + item_start;

        if item.is_empty() {
            return Err(CronError::Syntax(base_offset));
        }

        let slash_positions: Vec<usize> = item
            .bytes()
            .enumerate()
            .filter_map(|(i, b)| (b == b'/').then_some(i))
            .collect();
        if slash_positions.len() > 1 {
            return Err(CronError::Syntax(base_offset + slash_positions[1]));
        }

        let slash = slash_positions.first().copied();
        let base_end = slash.unwrap_or(item.len());
        let base = &item[..base_end];
        let step_start = slash.map(|p| p + 1);

        let is_star = base == "*";
        let dash = base.find('-');
        let is_range = dash.is_some();

        // A step is permitted only on a star or range. Check this before
        // attempting to parse any numeric content in an invalid base.
        if let Some(slash_pos) = slash {
            if !is_star && !is_range {
                return Err(CronError::Syntax(base_offset + slash_pos));
            }
        }

        let mut range_start;
        let mut range_end;

        if is_star {
            range_start = 0;
            range_end = max - 1;
        } else if let Some(dash_pos) = dash {
            let left = &base[..dash_pos];
            let right = &base[dash_pos + 1..];
            range_start = parse_number(left, base_offset, max - 1)?;
            range_end = parse_number(right, base_offset + dash_pos + 1, max - 1)?;
            if range_start > range_end {
                return Err(CronError::Descending(base_offset + dash_pos));
            }
        } else {
            range_start = parse_number(base, base_offset, max - 1)?;
            range_end = range_start;
        }

        let step = if let Some(p) = step_start {
            let step_offset = base_offset + p;
            let value = parse_number(&item[p..], step_offset, usize::MAX)
                .map_err(|e| match e {
                    CronError::Bounds(_) => CronError::Step(step_offset),
                    other => other,
                })?;
            if value == 0 || value > max {
                return Err(CronError::Step(step_offset));
            }
            value
        } else {
            1
        };

        let mut value = range_start;
        while value <= range_end {
            selected[value] = true;
            match value.checked_add(step) {
                Some(next) => value = next,
                None => break,
            }
        }

        if item_end == field.len() {
            break;
        }
        item_start = item_end + 1;
    }

    Ok(())
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
    added_after: Option<u64>,
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

        let order = self.jobs.len();
        self.reserved.push(id.to_owned());
        self.jobs.push(Job {
            id: id.to_owned(),
            cron,
            priority,
            policy,
            order,
            added_after: self.last_poll,
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
        let mut emitted: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let first_boundary = match (lower_exclusive, job.added_after) {
                (Some(last), _) => last / 60_000 + 1,
                (None, _) => 0,
            };
            let first_boundary = if let Some(added) = job.added_after {
                first_boundary.max(added / 60_000 + 1)
            } else {
                first_boundary
            };
            let last_boundary = now / 60_000;
            if first_boundary > last_boundary {
                continue;
            }

            match job.policy {
                Policy::Skip => {
                    if now % 60_000 == 0
                        && now / 60_000 >= first_boundary
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
                    let mut latest = None;
                    for minute in first_boundary..=last_boundary {
                        if job.cron.matches(minute) {
                            latest = Some(minute);
                        }
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
                Policy::All => {
                    for minute in first_boundary..=last_boundary {
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