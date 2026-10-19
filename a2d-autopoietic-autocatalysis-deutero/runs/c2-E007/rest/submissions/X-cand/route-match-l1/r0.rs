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
            .map(|segment| match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
            })
            .collect();

        self.routes.push(Route { id, segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .bytes()
            .position(|byte| byte == b'?' || byte == b'#')
            .unwrap_or(path.len());
        let request_segments = parse_path(&path[..head_end])?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if route.segments.len() != request_segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, request_segment) in
                route.segments.iter().zip(request_segments.iter())
            {
                match pattern_segment {
                    Segment::Static(value) if value == request_segment => {}
                    Segment::Param(name) => {
                        params.push((name.clone(), request_segment.clone()));
                    }
                    _ => {
                        matched = false;
                        break;
                    }
                }
            }

            if !matched {
                continue;
            }

            if best
                .as_ref()
                .is_none_or(|(previous, _)| route.rank > previous.rank)
            {
                best = Some((route, params));
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

    let bytes = input.as_bytes();
    let mut parsed = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    loop {
        let end = bytes[start..]
            .iter()
            .position(|&byte| byte == b'/')
            .map_or(bytes.len(), |offset| start + offset);

        if end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let first = bytes[start];
        if first == b':' {
            let name_bytes = &bytes[start + 1..end];
            let valid = !name_bytes.is_empty()
                && name_bytes[0].is_ascii_lowercase()
                && name_bytes[1..]
                    .iter()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_');

            if !valid {
                return Err(Error::InvalidPattern { at: start });
            }

            let name = std::str::from_utf8(name_bytes)
                .expect("capture names contain only ASCII")
                .to_owned();
            if !names.insert(name.clone()) {
                return Err(Error::InvalidPattern { at: start });
            }
            parsed.push(Segment::Param(name));
        } else if first == b'*' {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_static_segment(bytes, start, end)
                .map_err(|at| Error::InvalidPattern { at })?;
            parsed.push(Segment::Static(decoded));
        }

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPattern { at: end });
        }
        start = end + 1;
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

    let bytes = input.as_bytes();
    let mut parsed = Vec::new();
    let mut start = 1;

    loop {
        let end = bytes[start..]
            .iter()
            .position(|&byte| byte == b'/')
            .map_or(bytes.len(), |offset| start + offset);

        if end == start {
            return Err(Error::InvalidPath { at: start });
        }

        let decoded =
            decode_static_segment(bytes, start, end).map_err(|at| Error::InvalidPath { at })?;
        parsed.push(decoded);

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
        start = end + 1;
    }

    Ok(parsed)
}

fn decode_static_segment(bytes: &[u8], start: usize, end: usize) -> Result<String, usize> {
    let mut decoded = Vec::with_capacity(end - start);
    let mut index = start;

    while index < end {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= end {
                return Err(index);
            }

            let high = hex_upper(bytes[index + 1]).ok_or(index)?;
            let low = hex_upper(bytes[index + 2]).ok_or(index)?;
            let value = (high << 4) | low;

            if !(0x20..=0x7e).contains(&value) || value == b'/' || is_unreserved(value) {
                return Err(index);
            }

            decoded.push(value);
            index += 3;
        } else {
            if !is_unreserved(byte) {
                return Err(index);
            }
            decoded.push(byte);
            index += 1;
        }
    }

    Ok(String::from_utf8(decoded).expect("validated ASCII segment"))
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn hex_upper(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}