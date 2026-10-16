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
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let segments = parse_path(head)?;

        let mut best: Option<(usize, &Route)> = None;

        for (route_id, route) in self.routes.iter().enumerate() {
            if route.segments.len() != segments.len() {
                continue;
            }

            let mut params = Vec::new();
            let mut matched = true;

            for (pattern_segment, path_segment) in route.segments.iter().zip(&segments) {
                match pattern_segment {
                    Segment::Static(expected) if expected == path_segment => {}
                    Segment::Static(_) => {
                        matched = false;
                        break;
                    }
                    Segment::Param(name) => {
                        params.push((name.clone(), path_segment.clone()));
                    }
                }
            }

            if !matched {
                continue;
            }

            let replace = match best {
                None => true,
                Some((_, previous)) => route.rank > previous.rank,
            };

            if replace {
                best = Some((route_id, route));
            }
        }

        let (route_id, route) = best.ok_or(Error::NotFound)?;
        let params = route
            .segments
            .iter()
            .zip(&segments)
            .filter_map(|(pattern_segment, value)| match pattern_segment {
                Segment::Param(name) => Some((name.clone(), value.clone())),
                Segment::Static(_) => None,
            })
            .collect();

        Ok(Match {
            route: route_id,
            params,
        })
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_pattern(input: &str) -> Result<Vec<Segment>, Error> {
    let raw_segments = split_and_validate_shape(input, true)?;
    let mut parsed = Vec::with_capacity(raw_segments.len());
    let mut names = std::collections::HashSet::new();

    for (start, raw) in raw_segments {
        if raw.starts_with(':') {
            let name = &raw[1..];
            let valid = {
                let bytes = name.as_bytes();
                !bytes.is_empty()
                    && bytes[0].is_ascii_lowercase()
                    && bytes[1..]
                        .iter()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
            };

            if !valid {
                return Err(Error::InvalidPattern { at: start });
            }
            if !names.insert(name.to_string()) {
                return Err(Error::InvalidPattern { at: start });
            }
            parsed.push(Segment::Param(name.to_string()));
        } else if raw.starts_with('*') {
            return Err(Error::InvalidPattern { at: start });
        } else {
            parsed.push(Segment::Static(decode_static(raw, start, true)?));
        }
    }

    Ok(parsed)
}

fn parse_path(input: &str) -> Result<Vec<String>, Error> {
    let raw_segments = split_and_validate_shape(input, false)?;
    raw_segments
        .into_iter()
        .map(|(start, raw)| decode_static(raw, start, false))
        .collect()
}

fn split_and_validate_shape<'a>(
    input: &'a str,
    pattern: bool,
) -> Result<Vec<(usize, &'a str)>, Error> {
    let invalid = |at| {
        if pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };

    if !input.starts_with('/') {
        return Err(invalid(0));
    }
    if input == "/" {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut segments = Vec::new();
    let mut start = 1;

    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            if i == bytes.len() - 1 {
                if start == i {
                    return Err(invalid(start));
                }
                segments.push((start, &input[start..i]));
                return Err(invalid(i));
            }
            if start == i {
                return Err(invalid(start));
            }
            segments.push((start, &input[start..i]));
            start = i + 1;
        }
    }

    if start == bytes.len() {
        return Err(invalid(start));
    }
    segments.push((start, &input[start..]));
    Ok(segments)
}

fn decode_static(raw: &str, start: usize, pattern: bool) -> Result<String, Error> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(invalid_at(pattern, start + i));
            }
            let Some(hi) = upper_hex(bytes[i + 1]) else {
                return Err(invalid_at(pattern, start + i));
            };
            let Some(lo) = upper_hex(bytes[i + 2]) else {
                return Err(invalid_at(pattern, start + i));
            };
            let decoded = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || is_unreserved(decoded)
            {
                return Err(invalid_at(pattern, start + i));
            }
            out.push(decoded);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(invalid_at(pattern, start + i));
        }
    }

    Ok(String::from_utf8(out).expect("decoded bytes are ASCII"))
}

fn invalid_at(pattern: bool, at: usize) -> Error {
    if pattern {
        Error::InvalidPattern { at }
    } else {
        Error::InvalidPath { at }
    }
}

fn upper_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}