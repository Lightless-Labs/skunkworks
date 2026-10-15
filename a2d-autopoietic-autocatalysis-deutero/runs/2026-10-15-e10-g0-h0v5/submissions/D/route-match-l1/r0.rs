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

#[derive(Debug, Clone)]
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
            .map(|s| match s {
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
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request_segments = parse_path(head)?;

        let mut best: Option<(usize, &Route, Vec<(String, String)>)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
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
                    Segment::Static(_) => {
                        matched = false;
                        break;
                    }
                    Segment::Param(name) => {
                        params.push((name.clone(), request_segment.clone()));
                    }
                }
            }

            if !matched {
                continue;
            }

            let replace = match best {
                None => true,
                Some((_, current, _)) => route.rank > current.rank,
            };
            if replace {
                best = Some((route_id, route, params));
            }
        }

        match best {
            Some((route, _, params)) => Ok(Match { route, params }),
            None => Err(Error::NotFound),
        }
    }
}

fn parse_pattern(pattern: &str) -> Result<Vec<Segment>, Error> {
    parse_segments(pattern, true).map_err(|at| Error::InvalidPattern { at })
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
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
    if !input.starts_with('/') {
        return Err(0);
    }
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
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(start);
        }

        let raw = &input[start..end];
        if is_pattern && raw.as_bytes()[0] == b':' {
            let name = &raw[1..];
            if !valid_capture_name(name) {
                return Err(start);
            }
            if seen_names.iter().any(|existing| existing == name) {
                return Err(start);
            }
            seen_names.push(name.to_owned());
            result.push(Segment::Param(name.to_owned()));
        } else if is_pattern && raw.as_bytes()[0] == b'*' {
            return Err(start);
        } else {
            result.push(Segment::Static(decode_static(raw, start)?));
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

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
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
            let hi = uppercase_hex(bytes[i + 1]).ok_or(base + i)?;
            let lo = uppercase_hex(bytes[i + 2]).ok_or(base + i)?;
            let value = hi * 16 + lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(byte) {
            decoded.push(byte);
            i += 1;
        } else {
            return Err(base + i);
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