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
            .char_indices()
            .find_map(|(i, c)| (c == '?' || c == '#').then_some(i))
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request_segments = parse_path(head)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if route.segments.len() != request_segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matches = true;

            for (pattern_segment, request_segment) in
                route.segments.iter().zip(request_segments.iter())
            {
                match pattern_segment {
                    Segment::Static(value) if value == request_segment => {}
                    Segment::Static(_) => {
                        matches = false;
                        break;
                    }
                    Segment::Param(name) => {
                        params.push((name.clone(), request_segment.clone()));
                    }
                }
            }

            if matches {
                let replace = best
                    .as_ref()
                    .map(|(best_route, _)| route.rank > best_route.rank)
                    .unwrap_or(true);
                if replace {
                    best = Some((route, params));
                }
            }
        }

        best.map(|(route, params)| Match {
            route: route.id,
            params,
        })
        .ok_or(Error::NotFound)
    }
}

fn parse_pattern(pattern: &str) -> Result<Vec<Segment>, Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    parse_segments(pattern, true).map_err(|at| Error::InvalidPattern { at })
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    parse_segments(path, false)
        .map(|segments| {
            segments
                .into_iter()
                .map(|segment| match segment {
                    Segment::Static(value) => value,
                    Segment::Param(_) => unreachable!(),
                })
                .collect()
        })
        .map_err(|at| Error::InvalidPath { at })
}

fn parse_segments(input: &str, is_pattern: bool) -> Result<Vec<Segment>, usize> {
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut seen_names = Vec::<String>::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&byte| byte == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(start);
        }

        let raw = &input[start..end];

        if is_pattern && raw.starts_with(':') {
            let name = &raw[1..];
            if !valid_name(name) || seen_names.iter().any(|existing| existing == name) {
                return Err(start);
            }
            seen_names.push(name.to_owned());
            result.push(Segment::Param(name.to_owned()));
        } else {
            if is_pattern && raw.starts_with('*') {
                return Err(start);
            }
            let decoded = decode_static(raw, start)?;
            result.push(Segment::Static(decoded));
        }

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(end);
        }
        start = end + 1;
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
        .all(|&byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn decode_static(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = hex_upper(bytes[i + 1]).ok_or(base + i)?;
            let lo = hex_upper(bytes[i + 2]).ok_or(base + i)?;
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
            if !is_unreserved(byte) {
                return Err(base + i);
            }
            decoded.push(byte);
            i += 1;
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
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