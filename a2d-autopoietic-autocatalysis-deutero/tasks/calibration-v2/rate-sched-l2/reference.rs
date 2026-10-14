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
    minutes: Vec<bool>,
    hours: Vec<bool>,
}
impl Cron {
    pub fn parse(text: &str) -> Result<Self, CronError> {
        let b = text.as_bytes();
        if let Some(i) = b.iter().position(|c| c.is_ascii_whitespace() && *c != b' ') {
            return Err(CronError::Layout(i));
        }
        let spaces: Vec<_> = b
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == b' ')
            .map(|(i, _)| i)
            .collect();
        let split = match spaces.len() {
            0 => return Err(CronError::Layout(b.len())),
            1 => spaces[0],
            _ => return Err(CronError::Layout(spaces[1])),
        };
        if split == 0 || split == b.len() - 1 {
            return Err(CronError::Layout(split));
        }
        Ok(Self {
            minutes: Self::field(&text[..split], 0, 59)?,
            hours: Self::field(&text[split + 1..], split + 1, 23)?,
        })
    }
    fn number(s: &str, off: usize) -> Result<u64, CronError> {
        if s.is_empty() {
            return Err(CronError::Syntax(off));
        }
        if let Some(i) = s.bytes().position(|c| !c.is_ascii_digit()) {
            return Err(CronError::Syntax(off + i));
        }
        if s.len() > 1 && s.starts_with('0') {
            return Err(CronError::Number(off));
        }
        Ok(s.bytes().fold(0u64, |a, b| {
            a.saturating_mul(10).saturating_add((b - b'0') as u64)
        }))
    }
    fn field(s: &str, off: usize, max: u64) -> Result<Vec<bool>, CronError> {
        let mut selected = vec![false; (max + 1) as usize];
        let mut start = off;
        for item in s.split(',') {
            let slashes: Vec<_> = item.match_indices('/').map(|(i, _)| i).collect();
            if slashes.len() > 1 {
                return Err(CronError::Syntax(start + slashes[1]));
            }
            let (base, step) = if let Some(&p) = slashes.first() {
                (&item[..p], Some((&item[p + 1..], start + p + 1, p)))
            } else {
                (item, None)
            };
            let dash = base.find('-');
            if let Some((_, _, p)) = step {
                if base != "*" && dash.is_none() {
                    return Err(CronError::Syntax(start + p));
                }
            }
            let (lo, hi) = if base == "*" {
                (0, max)
            } else if let Some(p) = dash {
                let lo = Self::number(&base[..p], start)?;
                if lo > max {
                    return Err(CronError::Bounds(start));
                }
                let hi = Self::number(&base[p + 1..], start + p + 1)?;
                if hi > max {
                    return Err(CronError::Bounds(start + p + 1));
                }
                if lo > hi {
                    return Err(CronError::Descending(start + p));
                }
                (lo, hi)
            } else {
                let n = Self::number(base, start)?;
                if n > max {
                    return Err(CronError::Bounds(start));
                }
                (n, n)
            };
            let stride = if let Some((s, pos, _)) = step {
                let n = Self::number(s, pos)?;
                if n == 0 || n > max + 1 {
                    return Err(CronError::Step(pos));
                }
                n
            } else {
                1
            };
            let mut n = lo;
            while n <= hi {
                selected[n as usize] = true;
                n += stride;
            }
            start += item.len() + 1;
        }
        Ok(selected)
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
struct Entry {
    id: String,
    cron: Cron,
    priority: u8,
    policy: Policy,
}
pub struct Calendar {
    entries: Vec<Entry>,
    last: Option<u64>,
}
impl Calendar {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            last: None,
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
        if self.entries.iter().any(|e| e.id == id) {
            return Err(Error::Duplicate);
        }
        let cron = Cron::parse(expression).map_err(Error::Cron)?;
        self.entries.push(Entry {
            id: id.into(),
            cron,
            priority,
            policy,
        });
        Ok(())
    }
    pub fn poll(&mut self, now: u64) -> Result<Vec<Occurrence>, Error> {
        if self.last.is_some_and(|v| now < v) {
            return Err(Error::TimeReversed);
        }
        if now > 604800000 {
            return Err(Error::TimeRange);
        }
        let first = self.last.map_or(0, |v| v / 60000 + 1);
        let end = now / 60000;
        let mut out = Vec::new();
        for (seq, e) in self.entries.iter().enumerate() {
            let mut times = Vec::new();
            for m in first..=end {
                if e.cron.matches(m) {
                    times.push(m * 60000);
                }
            }
            match e.policy {
                Policy::All => (),
                Policy::Once => {
                    if let Some(last) = times.last().copied() {
                        times = vec![last];
                    }
                }
                Policy::Skip => times.retain(|t| *t == now),
            }
            for at in times {
                out.push((at, std::cmp::Reverse(e.priority), seq, e.id.clone()));
            }
        }
        out.sort_by_key(|v| (v.0, v.1, v.2));
        self.last = Some(now);
        Ok(out
            .into_iter()
            .map(|v| Occurrence { id: v.3, at: v.0 })
            .collect())
    }
}
