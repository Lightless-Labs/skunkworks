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

struct Route {
    segments: Vec<Segment>,
    rank: Vec<u8>,
    id: usize,
}

pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn add(&mut self, pattern: &str) -> Result<usize, Error> {
        let segments = parse_pattern(pattern)?;
        let mut rank = Vec::with_capacity(segments.len() + 1);
        for segment in &segments {
            match segment {
                Segment::Static(_) => rank.push(2),
                Segment::Param(_) => rank.push(1),
                Segment::Wildcard(_) => rank.push(0),
            }
        }
        if !matches!(segments.last(), Some(Segment::Wildcard(_))) {
            rank.push(3);
        }

        let id = self.routes.len();
        self.routes.push(Route { segments, rank, id });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let segments = parse_path(&path[..head_end])?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;
        for route in &self.routes {
            if let Some(params) = match_route(&route.segments, &segments) {
                let replace = match &best {
                    None => true,
                    Some((best_route, _)) => {
                        route.rank > best_route.rank
                            || (route.rank == best_route.rank && route.id < best_route.id)
                    }
                };
                if replace {
                    best = Some((route, params));
                }
            }
        }

        match best {
            Some((route, params)) => Ok(Match {
                route: route.id,
                params,
            }),
            None => Err(Error::NotFound),
        }
    }
}

fn parse_pattern(input: &str) -> Result<Vec<Segment>, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let body_start = 1;
    let trailing = input.ends_with('/');
    let body_end = if trailing { input.len() - 1 } else { input.len() };
    let body = &input[body_start..body_end];

    let mut parsed = Vec::new();
    let mut names = Vec::<String>::new();
    let mut offset = body_start;

    for (index, raw) in body.split('/').enumerate() {
        if raw.is_empty() {
            return Err(Error::InvalidPattern { at: offset });
        }

        let segment = if raw.starts_with(':') || raw.starts_with('*') {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_name(name) || names.iter().any(|n| n == name) {
                return Err(Error::InvalidPattern { at: offset });
            }
            if marker == b'*' && index + 1 != body.split('/').count() {
                return Err(Error::InvalidPattern { at: offset });
            }
            names.push(name.to_string());
            if marker == b':' {
                Segment::Param(name.to_string())
            } else {
                Segment::Wildcard(name.to_string())
            }
        } else {
            Segment::Static(decode_segment(raw, offset).map_err(|at| Error::InvalidPattern { at })?)
        };

        parsed.push(segment);
        offset += raw.len() + 1;
    }

    if trailing {
        return Err(Error::InvalidPattern {
            at: input.len() - 1,
        });
    }

    Ok(parsed)
}

fn parse_path(input: &str) -> Result<Vec<String>, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let trailing = input.ends_with('/');
    let body_end = if trailing { input.len() - 1 } else { input.len() };
    let body = &input[1..body_end];

    let mut parsed = Vec::new();
    let mut offset = 1;
    for raw in body.split('/') {
        if raw.is_empty() {
            return Err(Error::InvalidPath { at: offset });
        }
        parsed.push(decode_segment(raw, offset).map_err(|at| Error::InvalidPath { at })?);
        offset += raw.len() + 1;
    }

    if trailing {
        return Err(Error::InvalidPath {
            at: input.len() - 1,
        });
    }

    Ok(parsed)
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_segment(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = uppercase_hex(bytes[i + 1]).ok_or(base + i)?;
            let lo = uppercase_hex(bytes[i + 2]).ok_or(base + i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + i);
            }
            decoded.push(value);
            i += 3;
        } else {
            if !is_unreserved(b) {
                return Err(base + i);
            }
            decoded.push(b);
            i += 1;
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn uppercase_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_route(pattern: &[Segment], path: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut pi = 0;
    let mut si = 0;

    while pi < pattern.len() {
        match &pattern[pi] {
            Segment::Static(expected) => {
                if path.get(si)? != expected {
                    return None;
                }
                pi += 1;
                si += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), path.get(si)?.clone()));
                pi += 1;
                si += 1;
            }
            Segment::Wildcard(name) => {
                let value = path[si..].join("/");
                params.push((name.clone(), value));
                return Some(params);
            }
        }
    }

    if si == path.len() {
        Some(params)
    } else {
        None
    }
}