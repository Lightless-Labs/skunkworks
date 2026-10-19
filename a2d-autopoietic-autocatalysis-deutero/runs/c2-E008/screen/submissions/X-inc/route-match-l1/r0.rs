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
            .char_indices()
            .find_map(|(i, c)| (c == '?' || c == '#').then_some(i))
            .unwrap_or(path.len());
        let segments = parse_path_head(&path[..head_end])?;

        let mut best: Option<(usize, Vec<(String, String)>)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
            if route.segments.len() != segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, request_segment) in
                route.segments.iter().zip(segments.iter())
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

            if matched {
                let replace = match &best {
                    None => true,
                    Some((best_id, _)) => route.rank > self.routes[*best_id].rank,
                };
                if replace {
                    best = Some((route_id, params));
                }
            }
        }

        match best {
            Some((route, params)) => Ok(Match { route, params }),
            None => Err(Error::NotFound),
        }
    }
}

fn parse_pattern(input: &str) -> Result<Vec<Segment>, Error> {
    let invalid = |at| Error::InvalidPattern { at };
    if !input.starts_with('/') {
        return Err(invalid(0));
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(invalid(start));
        }

        let segment = &input[start..end];
        if segment.as_bytes()[0] == b':' {
            let name = &segment[1..];
            let name_bytes = name.as_bytes();
            let valid = !name_bytes.is_empty()
                && name_bytes[0].is_ascii_lowercase()
                && name_bytes[1..]
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_');
            if !valid || names.contains(name) {
                return Err(invalid(start));
            }
            names.insert(name.to_owned());
            result.push(Segment::Param(name.to_owned()));
        } else if segment.as_bytes()[0] == b'*' {
            return Err(invalid(start));
        } else {
            result.push(Segment::Static(parse_static_segment(segment, start, &invalid)?));
        }

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(invalid(end));
        }
        start = end + 1;
    }

    Ok(result)
}

fn parse_path_head(input: &str) -> Result<Vec<String>, Error> {
    let invalid = |at| Error::InvalidPath { at };
    if !input.starts_with('/') {
        return Err(invalid(0));
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut start = 1;

    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'/')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(invalid(start));
        }

        let segment = &input[start..end];
        result.push(parse_static_segment(segment, start, &invalid)?);

        if end == bytes.len() {
            break;
        }
        if end + 1 == bytes.len() {
            return Err(invalid(end));
        }
        start = end + 1;
    }

    Ok(result)
}

fn parse_static_segment<F>(segment: &str, base: usize, invalid: &F) -> Result<String, Error>
where
    F: Fn(usize) -> Error,
{
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'%' {
            if i + 2 >= bytes.len() {
                return Err(invalid(base + i));
            }
            let hi = hex_upper(bytes[i + 1]).ok_or_else(|| invalid(base + i))?;
            let lo = hex_upper(bytes[i + 2]).ok_or_else(|| invalid(base + i))?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(invalid(base + i));
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(byte) {
            decoded.push(byte);
            i += 1;
        } else {
            return Err(invalid(base + i));
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