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
        let parsed = parse_pattern(pattern)?;
        let id = self.routes.len();
        self.routes.push(parsed);
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let segments = parse_path(head)?;

        let mut best: Option<(usize, Vec<u8>, Vec<(String, String)>)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
            if route.segments.len() != segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, value) in route.segments.iter().zip(&segments) {
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

            let should_replace = match &best {
                None => true,
                Some((best_id, best_rank, _)) => {
                    route.rank > *best_rank
                        || (route.rank == *best_rank && route_id < *best_id)
                }
            };

            if should_replace {
                best = Some((route_id, route.rank.clone(), params));
            }
        }

        match best {
            Some((route, _, params)) => Ok(Match { route, params }),
            None => Err(Error::NotFound),
        }
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_pattern(pattern: &str) -> Result<Route, Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }

    let (raw_segments, trailing_slash) = split_segments(pattern);
    let mut segments = Vec::with_capacity(raw_segments.len());
    let mut rank = Vec::with_capacity(raw_segments.len());
    let mut names = std::collections::HashSet::new();

    for (start, raw) in raw_segments {
        if raw.is_empty() {
            return Err(Error::InvalidPattern { at: start });
        }

        if raw.starts_with(':') {
            let name = &raw[1..];
            if !valid_param_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(Segment::Param(name.to_owned()));
            rank.push(1);
        } else if raw.starts_with('*') {
            return Err(Error::InvalidPattern { at: start });
        } else {
            let decoded = decode_static(raw, start).map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
            rank.push(2);
        }
    }

    if trailing_slash {
        return Err(Error::InvalidPattern {
            at: pattern.len() - 1,
        });
    }

    Ok(Route { segments, rank })
}

fn parse_path(path: &str) -> Result<Vec<String>, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }

    let (raw_segments, trailing_slash) = split_segments(path);
    let mut segments = Vec::with_capacity(raw_segments.len());

    for (start, raw) in raw_segments {
        if raw.is_empty() {
            return Err(Error::InvalidPath { at: start });
        }
        let decoded = decode_static(raw, start).map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);
    }

    if trailing_slash {
        return Err(Error::InvalidPath {
            at: path.len() - 1,
        });
    }

    Ok(segments)
}

// Root has no segments. For other paths, return nonempty or empty raw segments
// with their byte offsets, plus whether the path has a trailing slash.
fn split_segments(path: &str) -> (Vec<(usize, &str)>, bool) {
    if path == "/" {
        return (Vec::new(), false);
    }

    let trailing_slash = path.ends_with('/');
    let body = &path[1..];
    let body_end = if trailing_slash {
        body.len().saturating_sub(1)
    } else {
        body.len()
    };
    let body = &body[..body_end];

    let mut segments = Vec::new();
    let mut start = 1usize;

    for part in body.split('/') {
        segments.push((start, part));
        start += part.len() + 1;
    }

    (segments, trailing_slash)
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

fn decode_static(raw: &str, start: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(start + i);
            }
            let hi = uppercase_hex(bytes[i + 1]).ok_or(start + i)?;
            let lo = uppercase_hex(bytes[i + 2]).ok_or(start + i)?;
            let value = (hi << 4) | lo;

            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(start + i);
            }

            decoded.push(value);
            i += 3;
        } else {
            if !is_unreserved(b) {
                return Err(start + i);
            }
            decoded.push(b);
            i += 1;
        }
    }

    // All accepted bytes are ASCII, so this conversion cannot fail.
    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn uppercase_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}