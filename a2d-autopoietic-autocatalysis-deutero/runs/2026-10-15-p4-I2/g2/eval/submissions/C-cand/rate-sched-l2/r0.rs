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

fn parse_number(
    text: &str,
    offset: usize,
    max: u32,
    error_kind: NumberKind,
) -> Result<u32, CronError> {
    if text.is_empty() {
        return Err(CronError::Syntax(offset));
    }

    if let Some((index, _)) = text.bytes().enumerate().find(|(_, b)| !b.is_ascii_digit()) {
        return Err(CronError::Syntax(offset + index));
    }

    if text.len() > 1 && text.as_bytes()[0] == b'0' {
        return Err(CronError::Number(offset));
    }

    let mut value: u32 = 0;
    for byte in text.bytes() {
        let digit = (byte - b'0') as u32;
        value = match value.checked_mul(10).and_then(|v| v.checked_add(digit)) {
            Some(v) => v,
            None => {
                return Err(match error_kind {
                    NumberKind::Base => CronError::Bounds(offset),
                    NumberKind::Step => CronError::Step(offset),
                });
            }
        };
    }

    if value > max {
        return Err(match error_kind {
            NumberKind::Base => CronError::Bounds(offset),
            NumberKind::Step => CronError::Step(offset),
        });
    }

    Ok(value)
}

#[derive(Clone, Copy)]
enum NumberKind {
    Base,
    Step,
}

fn parse_field<const N: usize>(
    field: &str,
    field_offset: usize,
    max: u32,
    bits: &mut [bool; N],
) -> Result<(), CronError> {
    let mut item_start = 0;

    loop {
        let relative_end = field[item_start..].find(',').unwrap_or(field.len() - item_start);
        let item_end = item_start + relative_end;
        let item = &field[item_start..item_end];
        let item_offset = field_offset + item_start;

        if item.is_empty() {
            return Err(CronError::Syntax(item_offset));
        }

        let slash = item.find('/');
        if let Some(first_slash) = slash {
            if let Some(second_rel) = item[first_slash + 1..].find('/') {
                return Err(CronError::Syntax(item_offset + first_slash + 1 + second_rel));
            }
        }

        let base_end = slash.unwrap_or(item.len());
        let base = &item[..base_end];
        let step_text = slash.map(|s| &item[s + 1..]);

        let is_star = base == "*";
        let dash = base.find('-');

        if step_text.is_some() && !is_star && dash.is_none() {
            return Err(CronError::Syntax(item_offset + slash.unwrap()));
        }

        let (start, end, dash_offset) = if is_star {
            (0, max, None)
        } else if let Some(dash_pos) = dash {
            let left_text = &base[..dash_pos];
            let right_text = &base[dash_pos + 1..];
            let left = parse_number(
                left_text,
                item_offset,
                max,
                NumberKind::Base,
            )?;
            let right = parse_number(
                right_text,
                item_offset + dash_pos + 1,
                max,
                NumberKind::Base,
            )?;
            if left > right {
                return Err(CronError::Descending(item_offset + dash_pos));
            }
            (left, right, Some(item_offset + dash_pos))
        } else {
            let number = parse_number(base, item_offset, max, NumberKind::Base)?;
            (number, number, None)
        };

        let step = if let Some(step_text) = step_text {
            let step_offset = item_offset + slash.unwrap() + 1;
            parse_number(step_text, step_offset, max, NumberKind::Step)?
        } else {
            1
        };

        if step == 0 || step > max + 1 {
            return Err(CronError::Step(
                item_offset + slash.map(|s| s + 1).unwrap_or(0),
            ));
        }

        let _ = dash_offset;
        let mut value = start;
        while value <= end {
            bits[value as usize] = true;
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

    Ok(())
}

impl Cron {
    pub fn parse(text: &str) -> Result<Self, CronError> {
        for (index, ch) in text.char_indices() {
            if ch.is_ascii_whitespace() && ch != ' ' {
                return Err(CronError::Layout(index));
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

        let separator = spaces[0];
        if separator == 0 || separator + 1 == text.len() {
            return Err(CronError::Layout(separator));
        }

        let minute_field = &text[..separator];
        let hour_field = &text[separator + 1..];

        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        parse_field(minute_field, 0, 59, &mut minutes)?;
        parse_field(hour_field, separator + 1, 23, &mut hours)?;

        Ok(Self { minutes, hours })
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
        let order = self.jobs.len();
        let owned_id = id.to_owned();
        self.ids.push(owned_id.clone());
        self.jobs.push(Job {
            id: owned_id,
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

        let lower = self.last_poll;
        let mut emitted: Vec<(Occurrence, u8, usize)> = Vec::new();

        for job in &self.jobs {
            let mut matching = Vec::new();
            let mut boundary = 0u64;
            while boundary <= now {
                if lower.map_or(true, |last| boundary > last)
                    && boundary % 60_000 == 0
                    && job.cron.matches(boundary / 60_000)
                {
                    matching.push(boundary);
                }
                boundary += 60_000;
            }

            let selected: Vec<u64> = match job.policy {
                Policy::All => matching,
                Policy::Once => matching.last().copied().into_iter().collect(),
                Policy::Skip => {
                    if now % 60_000 == 0 && matching.contains(&now) {
                        vec![now]
                    } else {
                        Vec::new()
                    }
                }
            };

            for at in selected {
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

fn main() {}