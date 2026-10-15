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

#[derive(Clone)]
struct Route {
    id: usize,
    segments: Vec<Segment>,
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

fn decode_segment(bytes: &[u8], start: usize) -> Result<String, usize> {
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
            let value = (hi << 4) | lo;

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

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn valid_name(name: &[u8]) -> bool {
    matches!(name.first(), Some(b'a'..=b'z'))
        && name[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn parse_parts(input: &str, is_pattern: bool) -> Result<Vec<Segment>, Error> {
    let bytes = input.as_bytes();
    let invalid = |at| {
        if is_pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };

    if bytes.first() != Some(&b'/') {
        return Err(invalid(0));
    }
    if bytes == b"/" {
        return Ok(Vec::new());
    }

    let trailing_slash = bytes.last() == Some(&b'/');
    let end = if trailing_slash {
        bytes.len() - 1
    } else {
        bytes.len()
    };
    let mut segments = Vec::new();
    let mut names = Vec::<Vec<u8>>::new();
    let mut start = 1;

    while start < end {
        let segment_end = bytes[start..end]
            .iter()
            .position(|b| *b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(end);

        if segment_end == start {
            return Err(invalid(start));
        }

        let segment = &bytes[start..segment_end];

        if is_pattern && matches!(segment.first(), Some(b':') | Some(b'*')) {
            let marker = segment[0];
            let name = &segment[1..];

            if !valid_name(name) || names.iter().any(|existing| existing == name) {
                return Err(invalid(start));
            }

            if marker == b'*' && segment_end != end {
                return Err(invalid(start));
            }

            names.push(name.to_vec());
            let name = String::from_utf8(name.to_vec()).expect("capture names are ASCII");
            if marker == b':' {
                segments.push(Segment::Param(name));
            } else {
                segments.push(Segment::Wildcard(name));
            }
        } else {
            let decoded = decode_segment(segment, start).map_err(invalid)?;
            segments.push(Segment::Static(decoded));
        }

        if segment_end == end {
            break;
        }
        start = segment_end + 1;
    }

    if trailing_slash {
        return Err(invalid(bytes.len() - 1));
    }

    Ok(segments)
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    pub fn add(&mut self, pattern: &str) -> Result<usize, Error> {
        let segments = parse_parts(pattern, true)?;
        let id = self.routes.len();
        let mut rank = Vec::with_capacity(segments.len() + 1);

        for segment in &segments {
            rank.push(match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            });
            if matches!(segment, Segment::Wildcard(_)) {
                break;
            }
        }
        if !segments.iter().any(|s| matches!(s, Segment::Wildcard(_))) {
            rank.push(3);
        }

        self.routes.push(Route { id, segments, rank });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .bytes()
            .position(|b| b == b'?' || b == b'#')
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request = parse_parts(head, false)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        'routes: for route in &self.routes {
            let mut params = Vec::new();
            let mut request_index = 0;

            for segment in &route.segments {
                match segment {
                    Segment::Static(expected) => {
                        let Some(Segment::Static(actual)) = request.get(request_index) else {
                            continue 'routes;
                        };
                        if actual != expected {
                            continue 'routes;
                        }
                        request_index += 1;
                    }
                    Segment::Param(name) => {
                        let Some(Segment::Static(value)) = request.get(request_index) else {
                            continue 'routes;
                        };
                        params.push((name.clone(), value.clone()));
                        request_index += 1;
                    }
                    Segment::Wildcard(name) => {
                        let remaining = request[request_index..]
                            .iter()
                            .map(|part| match part {
                                Segment::Static(value) => value.as_str(),
                                _ => "",
                            })
                            .collect::<Vec<_>>()
                            .join("/");
                        params.push((name.clone(), remaining));
                        request_index = request.len();
                        break;
                    }
                }
            }

            if request_index != request.len() {
                continue;
            }

            let replace = match &best {
                None => true,
                Some((current, _)) => route.rank > current.rank,
            };
            if replace {
                best = Some((route, params));
            }
        }

        best.map(|(route, params)| Match {
            route: route.id,
            params,
        })
        .ok_or(Error::NotFound)
    }
}

fn main() {}