#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPattern { at: usize },
    InvalidPath { at: usize },
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub route: usize,
    pub params: Vec<(String, String)>,
}

#[derive(Clone)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Clone)]
struct Route {
    id: usize,
    segments: Vec<Segment>,
    rank: Vec<u8>,
}

pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn add(&mut self, pattern: &str) -> Result<usize, Error> {
        let parsed = parse_pattern(pattern)?;
        let id = self.routes.len();
        self.routes.push(Route {
            id,
            segments: parsed.0,
            rank: parsed.1,
        });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let segments = parse_path(head)?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;
        let mut best_rank: Option<&[u8]> = None;

        for route in &self.routes {
            if let Some(params) = match_route(&route.segments, &segments) {
                let wins = match best_rank {
                    None => true,
                    Some(rank) => route.rank.as_slice() > rank,
                };
                if wins {
                    best_rank = Some(&route.rank);
                    best = Some((route.id, params));
                }
            }
        }

        match best {
            Some((route, params)) => Ok(Match { route, params }),
            None => Err(Error::NotFound),
        }
    }
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, Vec<u8>), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok((Vec::new(), vec![3]));
    }

    let bytes = pattern.as_bytes();
    let mut segments = Vec::new();
    let mut names = Vec::<String>::new();
    let mut rank = Vec::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|p| start + p)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &pattern[start..end];
        let first = bytes[start];

        if first == b':' || first == b'*' {
            let is_wildcard = first == b'*';
            let name = &raw[1..];
            if !valid_name(name) || names.iter().any(|n| n == name) {
                return Err(Error::InvalidPattern { at: start });
            }
            if is_wildcard && end < bytes.len() && end + 1 < bytes.len() {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.to_string());
            if is_wildcard {
                segments.push(Segment::Wildcard(name.to_string()));
                rank.push(0);
                if end < bytes.len() {
                    return Err(Error::InvalidPattern { at: end });
                }
            } else {
                segments.push(Segment::Param(name.to_string()));
                rank.push(1);
            }
        } else {
            let decoded = decode_segment(raw, start, true)?;
            segments.push(Segment::Static(decoded));
            rank.push(2);
        }

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPattern { at: end });
        }
    }

    if !segments.iter().any(|s| matches!(s, Segment::Wildcard(_))) {
        rank.push(3);
    }
    Ok((segments, rank))
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(Vec::new());
    }

    let bytes = path.as_bytes();
    let mut segments = Vec::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|p| start + p)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPath { at: start });
        }
        segments.push(decode_segment(&path[start..end], start, false)?);

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
    }
    Ok(segments)
}

fn valid_name(name: &str) -> bool {
    let b = name.as_bytes();
    if b.is_empty() || !b[0].is_ascii_lowercase() {
        return false;
    }
    b[1..]
        .iter()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

fn decode_segment(raw: &str, offset: usize, pattern: bool) -> Result<String, Error> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len()
                || !is_upper_hex(bytes[i + 1])
                || !is_upper_hex(bytes[i + 2])
            {
                return Err(if pattern {
                    Error::InvalidPattern { at: offset + i }
                } else {
                    Error::InvalidPath { at: offset + i }
                });
            }
            let value = (hex(bytes[i + 1]) << 4) | hex(bytes[i + 2]);
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(if pattern {
                    Error::InvalidPattern { at: offset + i }
                } else {
                    Error::InvalidPath { at: offset + i }
                });
            }
            out.push(value);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(if pattern {
                Error::InvalidPattern { at: offset + i }
            } else {
                Error::InvalidPath { at: offset + i }
            });
        }
    }

    Ok(String::from_utf8(out).expect("decoded bytes are ASCII"))
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn is_upper_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'A'..=b'F').contains(&b)
}

fn hex(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

fn match_route(pattern: &[Segment], path: &[String]) -> Option<Vec<(String, String)>> {
    let wildcard_at = pattern.iter().position(|s| matches!(s, Segment::Wildcard(_)));
    let fixed_len = wildcard_at.unwrap_or(pattern.len());

    if wildcard_at.is_none() && path.len() != pattern.len() {
        return None;
    }
    if path.len() < fixed_len {
        return None;
    }

    let mut params = Vec::new();
    for i in 0..fixed_len {
        match &pattern[i] {
            Segment::Static(value) if value != &path[i] => return None,
            Segment::Static(_) => {}
            Segment::Param(name) => params.push((name.clone(), path[i].clone())),
            Segment::Wildcard(_) => unreachable!(),
        }
    }

    if let Some(index) = wildcard_at {
        if let Segment::Wildcard(name) = &pattern[index] {
            params.push((name.clone(), path[index..].join("/")));
        }
    }
    Some(params)
}