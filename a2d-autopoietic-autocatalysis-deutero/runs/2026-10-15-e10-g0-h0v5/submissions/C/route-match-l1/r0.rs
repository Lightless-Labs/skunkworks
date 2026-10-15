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
}

#[derive(Clone)]
struct Route {
    segments: Vec<Segment>,
    id: usize,
    rank: Vec<u8>,
}

pub struct Router {
    routes: Vec<Route>,
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_static(bytes: &[u8], start: usize) -> Result<String, usize> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(start + i);
            }
            let hi = hex_value(bytes[i + 1]).ok_or(start + i)?;
            let lo = hex_value(bytes[i + 2]).ok_or(start + i)?;
            let value = hi * 16 + lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(start + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(start + i);
        }
    }

    Ok(String::from_utf8(decoded).unwrap())
}

fn parse_path(input: &str, is_pattern: bool) -> Result<(Vec<Segment>, bool), Error> {
    let invalid = |at| {
        if is_pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };

    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(invalid(0));
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = bytes.last() == Some(&b'/');
    let content_end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();

    let mut start = 1;
    while start < content_end {
        let end = bytes[start..content_end]
            .iter()
            .position(|&b| b == b'/')
            .map(|p| start + p)
            .unwrap_or(content_end);

        if end == start {
            return Err(invalid(start));
        }

        let raw = &bytes[start..end];
        if is_pattern && raw[0] == b':' {
            let name = &raw[1..];
            let valid_name = !name.is_empty()
                && name[0].is_ascii_lowercase()
                && name[1..]
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_');
            if !valid_name {
                return Err(invalid(start));
            }
            let name = std::str::from_utf8(name).unwrap().to_owned();
            if !names.insert(name.clone()) {
                return Err(invalid(start));
            }
            segments.push(Segment::Param(name));
        } else if is_pattern && raw[0] == b'*' {
            return Err(invalid(start));
        } else {
            let decoded = decode_static(raw, start).map_err(invalid)?;
            segments.push(Segment::Static(decoded));
        }

        if end == content_end {
            break;
        }
        start = end + 1;
    }

    if trailing {
        return Err(invalid(bytes.len() - 1));
    }

    Ok((segments, false))
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn add(&mut self, pattern: &str) -> Result<usize, Error> {
        let (segments, _) = parse_path(pattern, true)?;
        let id = self.routes.len();
        let rank = segments
            .iter()
            .map(|s| match s {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
            })
            .collect();
        self.routes.push(Route { segments, id, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .as_bytes()
            .iter()
            .position(|&b| b == b'?' || b == b'#')
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (segments, _) = parse_path(head, false)?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if route.segments.len() != segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matches = true;

            for (pattern_segment, request_segment) in route.segments.iter().zip(&segments) {
                match (pattern_segment, request_segment) {
                    (Segment::Static(expected), Segment::Static(actual)) if expected == actual => {}
                    (Segment::Param(name), Segment::Static(value)) => {
                        params.push((name.clone(), value.clone()));
                    }
                    _ => {
                        matches = false;
                        break;
                    }
                }
            }

            if matches {
                let replace = match best.as_ref() {
                    None => true,
                    Some((best_index, _)) => route.rank > self.routes[*best_index].rank,
                };
                if replace {
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

fn main() {}