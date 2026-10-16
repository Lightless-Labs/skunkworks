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
        let has_wildcard = segments.iter().any(|s| matches!(s, Segment::Wildcard(_)));
        for segment in &segments {
            rank.push(match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            });
        }
        if !has_wildcard {
            rank.push(3);
        }

        let id = self.routes.len();
        self.routes.push(Route { segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request = parse_path(head)?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;
        for (route_id, route) in self.routes.iter().enumerate() {
            if let Some(params) = match_route(&route.segments, &request) {
                let replace = match best.as_ref() {
                    None => true,
                    Some((best_id, _)) => {
                        let best_rank = &self.routes[*best_id].rank;
                        route.rank > *best_rank
                            || (route.rank == *best_rank && route_id < *best_id)
                    }
                };
                if replace {
                    best = Some((route_id, params));
                }
            }
        }

        match best {
            Some((route, params)) => Ok(Match { route, params }),
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

    let bytes = input.as_bytes();
    let mut start = 1;
    let mut pieces: Vec<(usize, usize)> = Vec::new();

    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            if i == bytes.len() - 1 {
                if let Some((_, end)) = pieces.last() {
                    if *end == start {
                        return Err(Error::InvalidPattern { at: i });
                    }
                } else {
                    return Err(Error::InvalidPattern { at: i });
                }
                // The preceding segment is parsed before trailing slash rejection.
                pieces.push((start, i));
                let parsed = parse_pattern_segments(input, &pieces)?;
                let _ = parsed;
                return Err(Error::InvalidPattern { at: i });
            }
            if i == start {
                return Err(Error::InvalidPattern { at: i });
            }
            pieces.push((start, i));
            start = i + 1;
        }
    }

    if start == bytes.len() {
        return Err(Error::InvalidPattern { at: bytes.len() - 1 });
    }
    pieces.push((start, bytes.len()));
    parse_pattern_segments(input, &pieces)
}

fn parse_pattern_segments(input: &str, pieces: &[(usize, usize)]) -> Result<Vec<Segment>, Error> {
    let mut result = Vec::with_capacity(pieces.len());
    let mut names = std::collections::HashSet::new();

    for &(start, end) in pieces {
        let raw = &input[start..end];
        if let Some(name) = raw.strip_prefix(':') {
            if !valid_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            result.push(Segment::Param(name.to_owned()));
        } else if let Some(name) = raw.strip_prefix('*') {
            if !valid_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if end != input.len() {
                return Err(Error::InvalidPattern { at: start });
            }
            result.push(Segment::Wildcard(name.to_owned()));
        } else {
            result.push(Segment::Static(decode_segment(raw, start).map_err(|at| {
                Error::InvalidPattern { at }
            })?));
        }
    }
    Ok(result)
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

fn parse_path(input: &str) -> Result<Vec<String>, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut start = 1;
    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            if i == bytes.len() - 1 {
                if i == start {
                    return Err(Error::InvalidPath { at: i });
                }
                result.push(
                    decode_segment(&input[start..i], start)
                        .map_err(|at| Error::InvalidPath { at })?,
                );
                return Err(Error::InvalidPath { at: i });
            }
            if i == start {
                return Err(Error::InvalidPath { at: i });
            }
            result.push(
                decode_segment(&input[start..i], start)
                    .map_err(|at| Error::InvalidPath { at })?,
            );
            start = i + 1;
        }
    }
    if start == bytes.len() {
        return Err(Error::InvalidPath {
            at: bytes.len() - 1,
        });
    }
    result.push(
        decode_segment(&input[start..], start).map_err(|at| Error::InvalidPath { at })?,
    );
    Ok(result)
}

fn decode_segment(raw: &str, offset: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(offset + i);
            }
            let hi = upper_hex(bytes[i + 1]).ok_or(offset + i)?;
            let lo = upper_hex(bytes[i + 2]).ok_or(offset + i)?;
            let decoded = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || is_unreserved(decoded)
            {
                return Err(offset + i);
            }
            out.push(decoded);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(offset + i);
        }
    }

    Ok(String::from_utf8(out).expect("decoded bytes are ASCII"))
}

fn upper_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_route(pattern: &[Segment], request: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut ri = 0;

    for segment in pattern {
        match segment {
            Segment::Static(value) => {
                if request.get(ri)? != value {
                    return None;
                }
                ri += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), request.get(ri)?.clone()));
                ri += 1;
            }
            Segment::Wildcard(name) => {
                params.push((name.clone(), request[ri..].join("/")));
                return Some(params);
            }
        }
    }

    if ri == request.len() {
        Some(params)
    } else {
        None
    }
}