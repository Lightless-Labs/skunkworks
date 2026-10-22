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
        let (segments, trailing) = parse_pattern(pattern)?;
        if trailing {
            return Err(Error::InvalidPattern {
                at: pattern.len() - 1,
            });
        }

        let mut rank = Vec::with_capacity(segments.len() + 1);
        let has_wildcard = segments
            .iter()
            .any(|segment| matches!(segment, Segment::Wildcard(_)));

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
        self.routes.push(Route {
            id,
            segments,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let request = parse_request(head)?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if let Some(params) = match_route(route, &request) {
                let replace = match &best {
                    None => true,
                    Some((best_route, _)) => route.rank > best_route.rank,
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

fn parse_pattern(input: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    parse_segments(input, true)
}

fn parse_request(input: &str) -> Result<Vec<String>, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    let (segments, trailing) = parse_segments(input, false)?;
    if trailing {
        return Err(Error::InvalidPath {
            at: input.len() - 1,
        });
    }
    let mut decoded = Vec::with_capacity(segments.len());
    for segment in segments {
        match segment {
            Segment::Static(value) => decoded.push(value),
            _ => unreachable!(),
        }
    }
    Ok(decoded)
}

fn parse_segments(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), Error> {
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = input.as_bytes();
    let mut end = bytes.len();
    let trailing = bytes[end - 1] == b'/';
    if trailing {
        end -= 1;
    }

    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::<String>::new();
    let mut start = 1;

    while start < end {
        let seg_end = bytes[start..end]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(end);

        if seg_end == start {
            return Err(if pattern {
                Error::InvalidPattern { at: start }
            } else {
                Error::InvalidPath { at: start }
            });
        }

        let segment = &input[start..seg_end];
        if pattern && (segment.starts_with(':') || segment.starts_with('*')) {
            let marker = segment.as_bytes()[0];
            let name = &segment[1..];
            let valid_name = {
                let nb = name.as_bytes();
                !nb.is_empty()
                    && nb[0].is_ascii_lowercase()
                    && nb[1..]
                        .iter()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
            };
            if !valid_name || !names.insert(name.to_string()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if marker == b'*' && seg_end != end {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(if marker == b':' {
                Segment::Param(name.to_string())
            } else {
                Segment::Wildcard(name.to_string())
            });
        } else {
            let decoded = decode_segment(segment, start, pattern)?;
            segments.push(Segment::Static(decoded));
        }

        if seg_end == end {
            break;
        }
        start = seg_end + 1;
    }

    Ok((segments, trailing))
}

fn decode_segment(segment: &str, base: usize, pattern: bool) -> Result<String, Error> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            let fail = || {
                if pattern {
                    Error::InvalidPattern { at: base + i }
                } else {
                    Error::InvalidPath { at: base + i }
                }
            };
            if i + 2 >= bytes.len() {
                return Err(fail());
            }
            let hi = hex_upper(bytes[i + 1]).ok_or_else(fail)?;
            let lo = hex_upper(bytes[i + 2]).ok_or_else(fail)?;
            let value = hi * 16 + lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(fail());
            }
            out.push(value);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(if pattern {
                Error::InvalidPattern { at: base + i }
            } else {
                Error::InvalidPath { at: base + i }
            });
        }
    }

    Ok(String::from_utf8(out).expect("decoded bytes are ASCII"))
}

fn hex_upper(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_route(route: &Route, request: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut request_index = 0;

    for segment in &route.segments {
        match segment {
            Segment::Static(value) => {
                if request.get(request_index)? != value {
                    return None;
                }
                request_index += 1;
            }
            Segment::Param(name) => {
                let value = request.get(request_index)?.clone();
                params.push((name.clone(), value));
                request_index += 1;
            }
            Segment::Wildcard(name) => {
                let value = request[request_index..].join("/");
                params.push((name.clone(), value));
                request_index = request.len();
                break;
            }
        }
    }

    if request_index != request.len() {
        return None;
    }
    Some(params)
}