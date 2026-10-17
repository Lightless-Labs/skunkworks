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
        let parsed = parse_pattern(pattern)?;
        let rank = parsed
            .iter()
            .map(|segment| match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
            })
            .collect();

        let id = self.routes.len();
        self.routes.push(Route {
            segments: parsed,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, path: &str) -> Result<Match, Error> {
        let head_end = path
            .char_indices()
            .find_map(|(i, ch)| (ch == '?' || ch == '#').then_some(i))
            .unwrap_or(path.len());
        let segments = parse_path_head(path, head_end)?;

        let mut best: Option<(usize, &Route)> = None;

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

            let should_replace = match best {
                None => true,
                Some((_, prior)) => route.rank > prior.rank,
            };

            if should_replace {
                best = Some((route_id, route));
                // Store captures separately below by recomputing from the winning route.
            }
        }

        let (route_id, route) = best.ok_or(Error::NotFound)?;
        let mut params = Vec::new();
        for (pattern_segment, value) in route.segments.iter().zip(&segments) {
            if let Segment::Param(name) = pattern_segment {
                params.push((name.clone(), value.clone()));
            }
        }

        Ok(Match {
            route: route_id,
            params,
        })
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
    let mut segments = Vec::new();
    let mut names = Vec::<String>::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &pattern[start..end];
        let first = bytes[start];

        if first == b'*' {
            return Err(Error::InvalidPattern { at: start });
        }

        if first == b':' {
            let name_bytes = &bytes[start + 1..end];
            let valid = !name_bytes.is_empty()
                && name_bytes[0].is_ascii_lowercase()
                && name_bytes[1..]
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
            segments.push(Segment::Param(name));
        } else {
            let decoded = decode_static_segment(raw, start)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPattern { at: end });
        }
        start = end + 1;
    }

    Ok(segments)
}

fn parse_path_head(path: &str, head_end: usize) -> Result<Vec<String>, Error> {
    let head = &path[..head_end];
    if !head.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if head == "/" {
        return Ok(Vec::new());
    }

    let bytes = head.as_bytes();
    let mut segments = Vec::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(Error::InvalidPath { at: start });
        }

        let raw = &head[start..end];
        let decoded = decode_static_segment(raw, start)
            .map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(Error::InvalidPath { at: end });
        }
        start = end + 1;
    }

    Ok(segments)
}

fn decode_static_segment(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let Some(high) = uppercase_hex(bytes[i + 1]) else {
                return Err(base + i);
            };
            let Some(low) = uppercase_hex(bytes[i + 2]) else {
                return Err(base + i);
            };
            let value = (high << 4) | low;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + i);
            }
            decoded.push(value);
            i += 3;
            continue;
        }

        if is_unreserved(byte) {
            decoded.push(byte);
            i += 1;
            continue;
        }

        return Err(base + i);
    }

    // All accepted raw bytes and escapes decode to ASCII.
    Ok(String::from_utf8(decoded).expect("decoded segment is ASCII"))
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn uppercase_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}