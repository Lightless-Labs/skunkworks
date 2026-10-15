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
        let segments = parse_pattern(pattern)?;
        let id = self.routes.len();
        let rank = segments
            .iter()
            .map(|s| match s {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
            })
            .collect();

        self.routes.push(Route { id, segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let decoded = parse_path(head)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if route.segments.len() != decoded.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;
            for (pattern_seg, path_seg) in route.segments.iter().zip(&decoded) {
                match pattern_seg {
                    Segment::Static(s) if s == path_seg => {}
                    Segment::Static(_) => {
                        matched = false;
                        break;
                    }
                    Segment::Param(name) => params.push((name.clone(), path_seg.clone())),
                }
            }

            if matched {
                let replace = match best.as_ref() {
                    None => true,
                    Some((old, _)) => route.rank > old.rank,
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

fn parse_pattern(pattern: &str) -> Result<Vec<Segment>, Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok(Vec::new());
    }

    let bytes = pattern.as_bytes();
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
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

        let segment = &pattern[start..end];
        let first = bytes[start];

        if first == b':' {
            let name = &segment[1..];
            if !valid_param_name(name) || !seen.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            result.push(Segment::Param(name.to_owned()));
        } else if first == b'*' {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_segment(bytes, start).map_err(|at| Error::InvalidPattern { at })?;
            result.push(Segment::Static(decoded));
        }

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPattern { at: end });
        }
    }

    Ok(result)
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(Vec::new());
    }

    let bytes = path.as_bytes();
    let mut result = Vec::new();
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

        let decoded = decode_segment(bytes, start).map_err(|at| Error::InvalidPath { at })?;
        result.push(decoded);

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
    }

    Ok(result)
}

fn valid_param_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_segment(bytes: &[u8], start: usize) -> Result<String, usize> {
    let mut out = Vec::new();
    let mut i = start;

    while i < bytes.len() && bytes[i] != b'/' {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len()
                || !is_upper_hex(bytes[i + 1])
                || !is_upper_hex(bytes[i + 2])
            {
                return Err(i);
            }
            let decoded = (hex_value(bytes[i + 1]) << 4) | hex_value(bytes[i + 2]);
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || is_unreserved(decoded)
            {
                return Err(i);
            }
            out.push(decoded);
            i += 3;
        } else {
            if !is_unreserved(b) {
                return Err(i);
            }
            out.push(b);
            i += 1;
        }
    }

    // All accepted bytes are ASCII.
    Ok(String::from_utf8(out).expect("validated ASCII segment"))
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn is_upper_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'A'..=b'F').contains(&b)
}

fn hex_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

fn main() {}