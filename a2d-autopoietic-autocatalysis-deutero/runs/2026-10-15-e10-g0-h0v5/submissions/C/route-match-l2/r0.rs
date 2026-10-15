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
    id: usize,
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
        let segments = parse_pattern(pattern)?;
        let id = self.routes.len();
        let mut rank = Vec::with_capacity(segments.len() + 1);
        let mut has_wildcard = false;

        for segment in &segments {
            match segment {
                Segment::Static(_) => rank.push(2),
                Segment::Param(_) => rank.push(1),
                Segment::Wildcard(_) => {
                    rank.push(0);
                    has_wildcard = true;
                    break;
                }
            }
        }
        if !has_wildcard {
            rank.push(3);
        }

        self.routes.push(Route { segments, id, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request = parse_path(head)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if let Some(params) = match_route(route, &request) {
                let replace = match best.as_ref() {
                    None => true,
                    Some((current, _)) => {
                        route.rank > current.rank
                            || (route.rank == current.rank && route.id < current.id)
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

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_pattern(input: &str) -> Result<Vec<Segment>, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }

    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut end = bytes.len();
    let trailing = bytes[end - 1] == b'/';
    if trailing {
        end -= 1;
    }

    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }

        if stop == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &input[start..stop];
        if raw.as_bytes()[0] == b':' || raw.as_bytes()[0] == b'*' {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }

            if marker == b'*' {
                let is_final = stop == end;
                if !is_final {
                    return Err(Error::InvalidPattern { at: start });
                }
                segments.push(Segment::Wildcard(name.to_owned()));
            } else {
                segments.push(Segment::Param(name.to_owned()));
            }
        } else {
            let decoded = decode_segment(raw, start).map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }

        if stop < end {
            start = stop + 1;
        } else {
            start = end;
        }
    }

    if trailing {
        return Err(Error::InvalidPattern { at: bytes.len() - 1 });
    }

    Ok(segments)
}

struct ParsedPath {
    segments: Vec<String>,
}

fn parse_path(input: &str) -> Result<ParsedPath, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if input == "/" {
        return Ok(ParsedPath { segments: Vec::new() });
    }

    let bytes = input.as_bytes();
    let mut end = bytes.len();
    let trailing = bytes[end - 1] == b'/';
    if trailing {
        end -= 1;
    }

    let mut segments = Vec::new();
    let mut start = 1;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(Error::InvalidPath { at: start });
        }

        let raw = &input[start..stop];
        let decoded = decode_segment(raw, start).map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);

        if stop < end {
            start = stop + 1;
        } else {
            start = end;
        }
    }

    if trailing {
        return Err(Error::InvalidPath { at: bytes.len() - 1 });
    }

    Ok(ParsedPath { segments })
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
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = uppercase_hex(bytes[i + 1]).ok_or(base + i)?;
            let lo = uppercase_hex(bytes[i + 2]).ok_or(base + i)?;
            let decoded = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || is_unreserved(decoded)
            {
                return Err(base + i);
            }
            out.push(decoded);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(base + i);
        }
    }

    Ok(String::from_utf8(out).unwrap())
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn uppercase_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn match_route(route: &Route, request: &ParsedPath) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut i = 0;

    for segment in &route.segments {
        match segment {
            Segment::Static(value) => {
                if request.segments.get(i)? != value {
                    return None;
                }
                i += 1;
            }
            Segment::Param(name) => {
                let value = request.segments.get(i)?;
                params.push((name.clone(), value.clone()));
                i += 1;
            }
            Segment::Wildcard(name) => {
                let value = request.segments[i..].join("/");
                params.push((name.clone(), value));
                return Some(params);
            }
        }
    }

    if i == request.segments.len() {
        Some(params)
    } else {
        None
    }
}

fn main() {}