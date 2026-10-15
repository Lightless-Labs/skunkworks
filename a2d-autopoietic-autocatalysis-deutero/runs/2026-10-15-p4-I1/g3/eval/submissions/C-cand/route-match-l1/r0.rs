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

#[derive(Debug, Clone)]
enum Segment {
    Static(String),
    Param(String),
}

#[derive(Debug, Clone)]
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
        let rank = segments
            .iter()
            .map(|segment| match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
            })
            .collect();

        let id = self.routes.len();
        self.routes.push(Route { segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .bytes()
            .position(|b| b == b'?' || b == b'#')
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request_segments = parse_path(head)?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
            if route.segments.len() != request_segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, value) in route.segments.iter().zip(&request_segments) {
                match pattern_segment {
                    Segment::Static(expected) if expected == value => {}
                    Segment::Param(name) => params.push((name.clone(), value.clone())),
                    _ => {
                        matched = false;
                        break;
                    }
                }
            }

            if !matched {
                continue;
            }

            let replace = match best.as_ref() {
                None => true,
                Some((best_id, _)) => route.rank > self.routes[*best_id].rank,
            };

            if replace {
                best = Some((route_id, params));
            }
        }

        match best {
            Some((route, params)) => Ok(Match { route, params }),
            None => Err(Error::NotFound),
        }
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_pattern(pattern: &str) -> Result<Vec<Segment>, Error> {
    let bytes = pattern.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok(Vec::new());
    }

    let end = bytes.len();
    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    while start < end {
        let segment_end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(end);

        if segment_end == start {
            if segment_end == end - 1 {
                return Err(Error::InvalidPattern { at: segment_end });
            }
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &bytes[start..segment_end];
        if raw[0] == b':' {
            let name = &raw[1..];
            if !valid_capture_name(name) {
                return Err(Error::InvalidPattern { at: start });
            }
            let name = std::str::from_utf8(name).unwrap().to_owned();
            if !names.insert(name.clone()) {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(Segment::Param(name));
        } else if raw[0] == b'*' {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_segment(raw, start).map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }

        if segment_end == end {
            break;
        }
        if segment_end == end - 1 {
            return Err(Error::InvalidPattern { at: segment_end });
        }
        start = segment_end + 1;
    }

    Ok(segments)
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
    let bytes = path.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(Vec::new());
    }

    let end = bytes.len();
    let mut segments = Vec::new();
    let mut start = 1;

    while start < end {
        let segment_end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(end);

        if segment_end == start {
            if segment_end == end - 1 {
                return Err(Error::InvalidPath { at: segment_end });
            }
            return Err(Error::InvalidPath { at: start });
        }

        let decoded =
            decode_segment(&bytes[start..segment_end], start).map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);

        if segment_end == end {
            break;
        }
        if segment_end == end - 1 {
            return Err(Error::InvalidPath { at: segment_end });
        }
        start = segment_end + 1;
    }

    Ok(segments)
}

fn valid_capture_name(name: &[u8]) -> bool {
    if name.is_empty() || !name[0].is_ascii_lowercase() {
        return false;
    }
    name[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_segment(raw: &[u8], offset: usize) -> Result<String, usize> {
    let mut decoded = Vec::with_capacity(raw.len());
    let mut i = 0;

    while i < raw.len() {
        let byte = raw[i];
        if byte == b'%' {
            if i + 2 >= raw.len() {
                return Err(offset + i);
            }
            let hi = uppercase_hex(raw[i + 1]).ok_or(offset + i)?;
            let lo = uppercase_hex(raw[i + 2]).ok_or(offset + i)?;
            let value = (hi << 4) | lo;

            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(offset + i);
            }

            decoded.push(value);
            i += 3;
        } else {
            if !is_unreserved(byte) {
                return Err(offset + i);
            }
            decoded.push(byte);
            i += 1;
        }
    }

    Ok(String::from_utf8(decoded).unwrap())
}

fn uppercase_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn main() {}