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
    id: usize,
    segments: Vec<Segment>,
    rank: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ParsedPath {
    segments: Vec<String>,
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
            .find_map(|(i, ch)| if ch == '?' || ch == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let parsed = parse_path_head(&path[..head_end], path.len())?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if route.segments.len() != parsed.segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, value) in route.segments.iter().zip(&parsed.segments) {
                match pattern_segment {
                    Segment::Static(expected) if expected == value => {}
                    Segment::Static(_) => {
                        matched = false;
                        break;
                    }
                    Segment::Param(name) => params.push((name.clone(), value.clone())),
                }
            }

            if !matched {
                continue;
            }

            let replace = match best {
                None => true,
                Some((current, _)) => route.rank > current.rank,
            };
            if replace {
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

fn parse_pattern(pattern: &str) -> Result<Vec<Segment>, Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok(Vec::new());
    }

    let bytes = pattern.as_bytes();
    let mut raw_segments = split_segments(bytes);
    let trailing_slash = bytes.last() == Some(&b'/');
    if trailing_slash {
        raw_segments.pop();
    }

    let mut segments = Vec::with_capacity(raw_segments.len());
    let mut names = std::collections::HashSet::new();

    for (start, end) in raw_segments {
        if start == end {
            return Err(Error::InvalidPattern { at: start });
        }

        let first = bytes[start];
        if first == b'*' {
            return Err(Error::InvalidPattern { at: start });
        }

        if first == b':' {
            let name_bytes = &bytes[start + 1..end];
            if !valid_capture_name(name_bytes) {
                return Err(Error::InvalidPattern { at: start });
            }
            let name = std::str::from_utf8(name_bytes).unwrap().to_owned();
            if !names.insert(name.clone()) {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(Segment::Param(name));
        } else {
            let decoded = decode_segment(bytes, start, end, ErrorKind::Pattern)?;
            segments.push(Segment::Static(decoded));
        }
    }

    if trailing_slash {
        return Err(Error::InvalidPattern {
            at: bytes.len() - 1,
        });
    }

    Ok(segments)
}

fn parse_path_head(head: &str, _original_len: usize) -> Result<ParsedPath, Error> {
    let bytes = head.as_bytes();
    if !head.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if head == "/" {
        return Ok(ParsedPath {
            segments: Vec::new(),
        });
    }

    let mut raw_segments = split_segments(bytes);
    let trailing_slash = bytes.last() == Some(&b'/');
    if trailing_slash {
        raw_segments.pop();
    }

    let mut segments = Vec::with_capacity(raw_segments.len());
    for (start, end) in raw_segments {
        if start == end {
            return Err(Error::InvalidPath { at: start });
        }
        segments.push(decode_segment(bytes, start, end, ErrorKind::Path)?);
    }

    if trailing_slash {
        return Err(Error::InvalidPath {
            at: bytes.len() - 1,
        });
    }

    Ok(ParsedPath { segments })
}

fn split_segments(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut start = 1;
    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            result.push((start, i));
            start = i + 1;
        }
    }
    result.push((start, bytes.len()));
    result
}

#[derive(Clone, Copy)]
enum ErrorKind {
    Pattern,
    Path,
}

fn decode_segment(
    bytes: &[u8],
    start: usize,
    end: usize,
    kind: ErrorKind,
) -> Result<String, Error> {
    let mut decoded = Vec::new();
    let mut i = start;

    while i < end {
        let byte = bytes[i];
        if byte == b'%' {
            if i + 2 >= end
                || !is_upper_hex(bytes[i + 1])
                || !is_upper_hex(bytes[i + 2])
            {
                return Err(make_error(kind, i));
            }
            let value = (hex_value(bytes[i + 1]) << 4) | hex_value(bytes[i + 2]);
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(make_error(kind, i));
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(byte) {
            decoded.push(byte);
            i += 1;
        } else {
            return Err(make_error(kind, i));
        }
    }

    Ok(String::from_utf8(decoded).unwrap())
}

fn valid_capture_name(bytes: &[u8]) -> bool {
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn is_upper_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte)
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'A'..=b'F' => byte - b'A' + 10,
        _ => unreachable!(),
    }
}

fn make_error(kind: ErrorKind, at: usize) -> Error {
    match kind {
        ErrorKind::Pattern => Error::InvalidPattern { at },
        ErrorKind::Path => Error::InvalidPath { at },
    }
}