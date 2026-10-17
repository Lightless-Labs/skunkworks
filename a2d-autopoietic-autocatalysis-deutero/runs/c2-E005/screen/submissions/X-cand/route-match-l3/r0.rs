#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPattern { at: usize },
    InvalidPath { at: usize },
    NotFound,
    InvalidMethod,
    MethodNotAllowed { allowed: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub route: usize,
    pub params: Vec<(String, String)>,
    pub redirect: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashPolicy {
    Strict,
    Ignore,
    Redirect,
}

#[derive(Debug, Clone)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Debug, Clone)]
struct Route {
    id: usize,
    method: String,
    segments: Vec<Segment>,
    trailing_slash: bool,
    rank: Vec<u8>,
}

#[derive(Debug)]
pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self {
            policy,
            routes: Vec::new(),
        }
    }

    pub fn add(&mut self, method: &str, pattern: &str) -> Result<usize, Error> {
        validate_method(method)?;
        let (segments, trailing_slash) = parse_pattern(pattern)?;

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
            method: method.to_owned(),
            segments,
            trailing_slash,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        validate_method(method)?;
        let head_end = path
            .char_indices()
            .find_map(|(index, ch)| matches!(ch, '?' | '#').then_some(index))
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (request_segments, request_trailing_slash) = parse_path(head)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict
                && route.trailing_slash != request_trailing_slash
            {
                continue;
            }
            if let Some(params) = match_route(&route.segments, &request_segments) {
                candidates.push((route, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<(&Route, Vec<(String, String)>)> = if method == "HEAD" {
            if candidates.iter().any(|(route, _)| route.method == "HEAD") {
                candidates
                    .into_iter()
                    .filter(|(route, _)| route.method == "HEAD")
                    .collect()
            } else {
                candidates
                    .into_iter()
                    .filter(|(route, _)| route.method == "GET")
                    .collect()
            }
        } else {
            candidates
                .into_iter()
                .filter(|(route, _)| route.method == method)
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed: Vec<String> = self
                .routes
                .iter()
                .filter(|route| {
                    route_matches_path(route, &request_segments, request_trailing_slash, self.policy)
                })
                .map(|route| route.method.clone())
                .collect();
            if allowed.iter().any(|item| item == "GET") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let (winner, params) = eligible
            .into_iter()
            .max_by(|(left, _), (right, _)| {
                left.rank
                    .cmp(&right.rank)
                    .then_with(|| right.id.cmp(&left.id))
            })
            .expect("eligible is nonempty");

        let redirect = if self.policy == SlashPolicy::Redirect
            && !head.is_empty()
            && winner.trailing_slash != request_trailing_slash
        {
            if winner.trailing_slash {
                Some(format!("{head}/"))
            } else {
                Some(head.to_owned())
            }
        } else {
            None
        };

        Ok(Match {
            route: winner.id,
            params,
            redirect,
        })
    }
}

fn validate_method(method: &str) -> Result<(), Error> {
    if method.is_empty() || !method.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(Error::InvalidMethod);
    }
    Ok(())
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), Error> {
    parse_segments(pattern, true).map_err(|at| Error::InvalidPattern { at })
}

fn parse_path(path: &str) -> Result<(Vec<String>, bool), Error> {
    let (segments, trailing_slash) =
        parse_segments(path, false).map_err(|at| Error::InvalidPath { at })?;
    let decoded = segments
        .into_iter()
        .map(|segment| match segment {
            Segment::Static(value) => value,
            _ => unreachable!("request paths contain no capture segments"),
        })
        .collect();
    Ok((decoded, trailing_slash))
}

fn parse_segments(input: &str, is_pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    if !input.starts_with('/') {
        return Err(0);
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = input.as_bytes();
    let trailing_slash = bytes.last() == Some(&b'/');
    let content_end = if trailing_slash {
        bytes.len() - 1
    } else {
        bytes.len()
    };
    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    while start < content_end {
        let end = bytes[start..content_end]
            .iter()
            .position(|byte| *byte == b'/')
            .map(|offset| start + offset)
            .unwrap_or(content_end);

        if end == start {
            return Err(start);
        }

        let raw = &input[start..end];
        if is_pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(start);
            }
            if marker == b'*' && end != content_end {
                return Err(start);
            }
            segments.push(if marker == b':' {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            });
        } else {
            let decoded = decode_static(raw, start)?;
            segments.push(Segment::Static(decoded));
        }

        if end == content_end {
            break;
        }
        start = end + 1;
    }

    Ok((segments, trailing_slash))
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

fn decode_static(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= bytes.len() {
                return Err(base + index);
            }
            let high = hex_upper(bytes[index + 1]).ok_or(base + index)?;
            let low = hex_upper(bytes[index + 2]).ok_or(base + index)?;
            let value = (high << 4) | low;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(base + index);
            }
            decoded.push(value);
            index += 3;
        } else {
            if !is_unreserved(byte) {
                return Err(base + index);
            }
            decoded.push(byte);
            index += 1;
        }
    }

    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn hex_upper(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn match_route(pattern: &[Segment], request: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut request_index = 0;

    for segment in pattern {
        match segment {
            Segment::Static(expected) => {
                if request.get(request_index)? != expected {
                    return None;
                }
                request_index += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), request.get(request_index)?.clone()));
                request_index += 1;
            }
            Segment::Wildcard(name) => {
                let value = request[request_index..].join("/");
                params.push((name.clone(), value));
                request_index = request.len();
            }
        }
    }

    if request_index != request.len() {
        return None;
    }
    Some(params)
}

fn route_matches_path(
    route: &Route,
    request: &[String],
    request_trailing_slash: bool,
    policy: SlashPolicy,
) -> bool {
    if policy == SlashPolicy::Strict && route.trailing_slash != request_trailing_slash {
        return false;
    }
    match_route(&route.segments, request).is_some()
}