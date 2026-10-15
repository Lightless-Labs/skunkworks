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

#[derive(Clone)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

struct Route {
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
    id: usize,
    rank: Vec<u8>,
    wildcard: bool,
}

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
        let (segments, trailing) = parse_pattern(pattern)?;
        let wildcard = matches!(segments.last(), Some(Segment::Wildcard(_)));
        let mut rank: Vec<u8> = segments
            .iter()
            .map(|s| match s {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            })
            .collect();
        if !wildcard {
            rank.push(3);
        }
        let id = self.routes.len();
        self.routes.push(Route {
            method: method.to_owned(),
            segments,
            trailing,
            id,
            rank,
            wildcard,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        validate_method(method)?;
        let head_end = path
            .char_indices()
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (request, trailing) = parse_path(head)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            if let Some(params) = match_route(route, &request) {
                candidates.push((route, params));
            }
        }
        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<usize> = if method == "HEAD"
            && candidates.iter().any(|(route, _)| route.method == "HEAD")
        {
            candidates
                .iter()
                .enumerate()
                .filter_map(|(i, (route, _))| (route.method == "HEAD").then_some(i))
                .collect()
        } else {
            candidates
                .iter()
                .enumerate()
                .filter_map(|(i, (route, _))| (route.method == method).then_some(i))
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed: Vec<String> =
                candidates.iter().map(|(route, _)| route.method.clone()).collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let winner_index = *eligible
            .iter()
            .max_by(|&&a, &&b| {
                candidates[a]
                    .0
                    .rank
                    .cmp(&candidates[b].0.rank)
                    .then_with(|| candidates[b].0.id.cmp(&candidates[a].0.id))
            })
            .unwrap();
        let (route, params) = &candidates[winner_index];

        let redirect = if self.policy == SlashPolicy::Redirect
            && !request.is_empty()
            && route.trailing != trailing
        {
            let mut target = head.to_owned();
            if route.trailing {
                target.push('/');
            } else if target.ends_with('/') {
                target.pop();
            }
            Some(target)
        } else {
            None
        };

        Ok(Match {
            route: route.id,
            params: params.clone(),
            redirect,
        })
    }
}

fn validate_method(method: &str) -> Result<(), Error> {
    if !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(Error::InvalidMethod)
    }
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    parse_segments(pattern, true).map_err(|at| Error::InvalidPattern { at })
}

fn parse_path(path: &str) -> Result<(Vec<String>, bool), Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    let (segments, trailing) =
        parse_segments(path, false).map_err(|at| Error::InvalidPath { at })?;
    let decoded = segments
        .into_iter()
        .map(|s| match s {
            Segment::Static(value) => value,
            _ => unreachable!(),
        })
        .collect();
    Ok((decoded, trailing))
}

fn parse_segments(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = input.as_bytes();
    let trailing = input.ends_with('/');
    let content_end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = Vec::<String>::new();
    let mut start = 1;

    while start < content_end {
        let end = bytes[start..content_end]
            .iter()
            .position(|&b| b == b'/')
            .map(|i| start + i)
            .unwrap_or(content_end);

        if end == start {
            return Err(start);
        }

        let raw = &input[start..end];
        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_name(name) || names.iter().any(|n| n == name) {
                return Err(start);
            }
            if marker == b'*' && end != content_end {
                return Err(start);
            }
            names.push(name.to_owned());
            segments.push(if marker == b':' {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            });
        } else {
            segments.push(Segment::Static(decode_segment(raw, start)?));
        }

        if end == content_end {
            break;
        }
        start = end + 1;
    }

    if trailing && content_end == 1 {
        return Err(1);
    }
    Ok((segments, trailing))
}

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn decode_segment(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len()
                || !is_upper_hex(bytes[i + 1])
                || !is_upper_hex(bytes[i + 2])
            {
                return Err(base + i);
            }
            let decoded = (hex_value(bytes[i + 1]) << 4) | hex_value(bytes[i + 2]);
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || is_unreserved(decoded)
            {
                return Err(base + i);
            }
            out.push(decoded);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(base + i);
        }
    }

    Ok(String::from_utf8(out).unwrap())
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn is_upper_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'A'..=b'F').contains(&b)
}

fn hex_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

fn match_route(route: &Route, request: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut index = 0;

    for segment in &route.segments {
        match segment {
            Segment::Static(value) => {
                if request.get(index)? != value {
                    return None;
                }
                index += 1;
            }
            Segment::Param(name) => {
                let value = request.get(index)?;
                params.push((name.clone(), value.clone()));
                index += 1;
            }
            Segment::Wildcard(name) => {
                let value = request[index..].join("/");
                params.push((name.clone(), value));
                index = request.len();
                break;
            }
        }
    }

    if index == request.len() {
        Some(params)
    } else {
        None
    }
}

fn main() {}