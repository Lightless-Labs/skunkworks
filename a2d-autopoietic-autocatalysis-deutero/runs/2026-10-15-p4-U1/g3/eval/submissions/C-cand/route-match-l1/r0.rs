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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Static(String),
    Param(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Route {
    id: usize,
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
        self.routes.push(Route { id, segments });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, ch)| if ch == '?' || ch == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let segments = parse_path_head(&path[..head_end])?;

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;

        for route in &self.routes {
            if route.segments.len() != segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, path_segment) in route.segments.iter().zip(&segments) {
                match pattern_segment {
                    Segment::Static(value) => {
                        if value != path_segment {
                            matched = false;
                            break;
                        }
                    }
                    Segment::Param(name) => params.push((name.clone(), path_segment.clone())),
                }
            }

            if !matched {
                continue;
            }

            let replace = match &best {
                None => true,
                Some((prior, _)) => route_is_more_specific(route, prior),
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

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

fn route_is_more_specific(candidate: &Route, prior: &Route) -> bool {
    for (a, b) in candidate.segments.iter().zip(&prior.segments) {
        let rank_a = match a {
            Segment::Static(_) => 2,
            Segment::Param(_) => 1,
        };
        let rank_b = match b {
            Segment::Static(_) => 2,
            Segment::Param(_) => 1,
        };
        if rank_a != rank_b {
            return rank_a > rank_b;
        }
    }
    false
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
    let mut names = Vec::<String>::new();

    let mut start = 1;
    let mut i = 1;
    while i <= bytes.len() {
        if i == bytes.len() || bytes[i] == b'/' {
            if i == start {
                if i == bytes.len() {
                    return Err(Error::InvalidPattern { at: i - 1 });
                }
                return Err(Error::InvalidPattern { at: start });
            }

            let raw = &pattern[start..i];
            if raw.as_bytes()[0] == b':' {
                let name_bytes = &raw.as_bytes()[1..];
                let valid = !name_bytes.is_empty()
                    && name_bytes[0].is_ascii_lowercase()
                    && name_bytes
                        .iter()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_');
                if !valid {
                    return Err(Error::InvalidPattern { at: start });
                }
                let name = raw[1..].to_owned();
                if names.iter().any(|existing| existing == &name) {
                    return Err(Error::InvalidPattern { at: start });
                }
                names.push(name.clone());
                result.push(Segment::Param(name));
            } else if raw.as_bytes()[0] == b'*' {
                return Err(Error::InvalidPattern { at: start });
            } else {
                result.push(Segment::Static(decode_static_segment(raw, start, true)?));
            }

            if i == bytes.len() {
                break;
            }
            if i + 1 == bytes.len() {
                return Err(Error::InvalidPattern { at: i });
            }
            start = i + 1;
        }
        i += 1;
    }

    Ok(result)
}

fn parse_path_head(path: &str) -> Result<Vec<String>, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(Vec::new());
    }

    let bytes = path.as_bytes();
    let mut result = Vec::new();
    let mut start = 1;
    let mut i = 1;

    while i <= bytes.len() {
        if i == bytes.len() || bytes[i] == b'/' {
            if i == start {
                if i == bytes.len() {
                    return Err(Error::InvalidPath { at: i - 1 });
                }
                return Err(Error::InvalidPath { at: start });
            }

            result.push(decode_static_segment(&path[start..i], start, false)?);

            if i == bytes.len() {
                break;
            }
            if i + 1 == bytes.len() {
                return Err(Error::InvalidPath { at: i });
            }
            start = i + 1;
        }
        i += 1;
    }

    Ok(result)
}

fn decode_static_segment(raw: &str, offset: usize, is_pattern: bool) -> Result<String, Error> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return invalid_segment(is_pattern, offset + i);
            }
            let hi = uppercase_hex(bytes[i + 1]);
            let lo = uppercase_hex(bytes[i + 2]);
            let (Some(hi), Some(lo)) = (hi, lo) else {
                return invalid_segment(is_pattern, offset + i);
            };
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return invalid_segment(is_pattern, offset + i);
            }
            decoded.push(value);
            i += 3;
        } else if b.is_ascii() && is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return invalid_segment(is_pattern, offset + i);
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn invalid_segment<T>(is_pattern: bool, at: usize) -> Result<T, Error> {
    if is_pattern {
        Err(Error::InvalidPattern { at })
    } else {
        Err(Error::InvalidPath { at })
    }
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

fn main() {}