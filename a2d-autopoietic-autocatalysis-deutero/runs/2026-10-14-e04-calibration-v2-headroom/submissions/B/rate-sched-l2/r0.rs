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

        if let Some((offset, _)) = bytes.iter().enumerate().find(|(_, b)| {
            b.is_ascii_whitespace() && **b != b' '
        }) {
            return Err(CronError::Layout(offset));
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
        parse_field(&text[..split], 0, 59, &mut minutes)?;
        parse_field(&text[split + 1..], split + 1, 23, &mut hours)?;

        Ok(Self { minutes, hours })
    }

    pub fn matches(&self, minute: u64) -> bool {
        self.minutes[(minute % 60) as usize]
            && self.hours[((minute / 60) % 24) as usize]
    }
}

fn parse_number(fragment: &str, offset: usize, max: u32) -> Result<u32, CronError> {
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

    let mut value = 0u32;
    for &b in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
        {
            Some(v) => v,
            None => return Err(CronError::Bounds(offset)),
        };
        if value > max {
            return Err(CronError::Bounds(offset));
        }
    }
    Ok(value)
}

fn parse_field(
    field: &str,
    field_offset: usize,
    max: u32,
    selected: &mut [bool],
) -> Result<(), CronError> {
    let mut item_start = 0usize;

    loop {
        let item_end = field[item_start..]
            .find(',')
            .map(|n| item_start + n)
            .unwrap_or(field.len());
        let item = &field[item_start..item_end];
        let item_offset = field_offset + item_start;

        if item.is_empty() {
            return Err(CronError::Syntax(item_offset));
        }

        let bytes = item.as_bytes();
        let slash_positions: Vec<usize> = bytes
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| (b == b'/').then_some(i))
            .collect();
        if slash_positions.len() > 1 {
            return Err(CronError::Syntax(item_offset + slash_positions[1]));
        }

        let slash = slash_positions.first().copied();
        let (base, step_text, step_offset) = match slash {
            Some(pos) => (&item[..pos], Some(&item[pos + 1..]), item_offset + pos + 1),
            None => (item, None, 0),
        };

        let is_star = base == "*";
        let dash = base.as_bytes().iter().position(|&b| b == b'-');
        if slash.is_some() && !is_star && dash.is_none() {
            return Err(CronError::Syntax(item_offset + slash.unwrap()));
        }

        let (start, end, range_dash) = if is_star {
            (0, max, None)
        } else if let Some(pos) = dash {
            let left = parse_number(&base[..pos], item_offset, max)?;
            let right = parse_number(&base[pos + 1..], item_offset + pos + 1, max)?;
            if left > right {
                return Err(CronError::Descending(item_offset + pos));
            }
            (left, right, Some(pos))
        } else {
            let n = parse_number(base, item_offset, max)?;
            (n, n, None)
        };

        let step = if let Some(step_fragment) = step_text {
            let n = parse_number(step_fragment, step_offset, max)?;
            if n == 0 {
                return Err(CronError::Step(step_offset));
            }
            n
        } else {
            1
        };

        let _ = range_dash;
        let mut n = start;
        while n <= end {
            selected[n as usize] = true;
            match n.checked_add(step) {
                Some(next) if next > n => n = next,
                _ => break,
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
        self.jobs.push(Job {
            id: id.to_owned(),
            cron,
            priority,
            policy,
            insertion_order: self.jobs.len(),
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
        let mut tagged: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let first_minute = match lower_exclusive {
                Some(last) => last / 60_000 + 1,
                None => 0,
            };
            let last_minute = now / 60_000;

            if first_minute > last_minute {
                continue;
            }

            let matching: Vec<u64> = (first_minute..=last_minute)
                .filter(|&minute| job.cron.matches(minute))
                .collect();

            match job.policy {
                Policy::All => {
                    for minute in matching {
                        tagged.push((
                            Occurrence {
                                id: job.id.clone(),
                                at: minute * 60_000,
                            },
                            job.priority,
                            job.insertion_order,
                        ));
                    }
                }
                Policy::Once => {
                    if let Some(&minute) = matching.last() {
                        tagged.push((
                            Occurrence {
                                id: job.id.clone(),
                                at: minute * 60_000,
                            },
                            job.priority,
                            job.insertion_order,
                        ));
                    }
                }
                Policy::Skip => {
                    if now % 60_000 == 0 {
                        let minute = now / 60_000;
                        if job.cron.matches(minute)
                            && lower_exclusive
                                .map_or(true, |last| minute * 60_000 > last)
                        {
                            tagged.push((
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

        tagged.sort_by(|a, b| {
            a.0.at
                .cmp(&b.0.at)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        self.last_poll = Some(now);
        Ok(tagged.into_iter().map(|entry| entry.0).collect())
    }
}