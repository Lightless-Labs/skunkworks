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
    id: usize,
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
    rank: Vec<u8>,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn decode_segment(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = hex_value(bytes[i + 1]).ok_or(base + i)?;
            let lo = hex_value(bytes[i + 2]).ok_or(base + i)?;
            let decoded = hi * 16 + lo;
            if !(0x20..=0x7e).contains(&decoded) || decoded == b'/' || unreserved(decoded) {
                return Err(base + i);
            }
            out.push(decoded);
            i += 3;
        } else {
            if !b.is_ascii() || !unreserved(b) {
                return Err(base + i);
            }
            out.push(b);
            i += 1;
        }
    }
    Ok(String::from_utf8(out).expect("decoded bytes are ASCII"))
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool, Vec<u8>), usize> {
    let bytes = pattern.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(0);
    }
    if pattern == "/" {
        return Ok((Vec::new(), false, vec![3]));
    }

    let trailing = pattern.ends_with('/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut rank = Vec::new();
    let mut start = 1;

    while start < end {
        let rel_end = bytes[start..end]
            .iter()
            .position(|b| *b == b'/')
            .map(|n| start + n)
            .unwrap_or(end);

        if rel_end == start {
            return Err(start);
        }

        let raw = &pattern[start..rel_end];
        let first = raw.as_bytes()[0];

        if first == b':' || first == b'*' {
            let name = &raw[1..];
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(start);
            }

            let wildcard = first == b'*';
            if wildcard && rel_end != end {
                return Err(start);
            }

            if wildcard {
                segments.push(Segment::Wildcard(name.to_owned()));
                rank.push(0);
            } else {
                segments.push(Segment::Param(name.to_owned()));
                rank.push(1);
            }
        } else {
            let decoded = decode_segment(raw, start)?;
            segments.push(Segment::Static(decoded));
            rank.push(2);
        }

        start = rel_end + 1;
    }

    if trailing && (end <= 1 || bytes[end - 1] == b'/') {
        return Err(end);
    }

    if !matches!(segments.last(), Some(Segment::Wildcard(_))) {
        rank.push(3);
    }

    Ok((segments, trailing, rank))
}

fn parse_path(path: &str) -> Result<(Vec<String>, bool), usize> {
    let bytes = path.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(0);
    }
    if path == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = path.ends_with('/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut start = 1;

    while start < end {
        let rel_end = bytes[start..end]
            .iter()
            .position(|b| *b == b'/')
            .map(|n| start + n)
            .unwrap_or(end);

        if rel_end == start {
            return Err(start);
        }

        segments.push(decode_segment(&path[start..rel_end], start)?);
        start = rel_end + 1;
    }

    if trailing && (end <= 1 || bytes[end - 1] == b'/') {
        return Err(end);
    }

    Ok((segments, trailing))
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self {
            policy,
            routes: Vec::new(),
        }
    }

    pub fn add(&mut self, method: &str, pattern: &str) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }

        let (segments, trailing, rank) =
            parse_pattern(pattern).map_err(|at| Error::InvalidPattern { at })?;
        let id = self.routes.len();

        self.routes.push(Route {
            id,
            method: method.to_owned(),
            segments,
            trailing,
            rank,
        });

        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }

        let head_end = path
            .char_indices()
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];

        let (request_segments, request_trailing) =
            parse_path(head).map_err(|at| Error::InvalidPath { at })?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();

        for route in &self.routes {
            let trailing_ok =
                self.policy != SlashPolicy::Strict || route.trailing == request_trailing;
            if !trailing_ok {
                continue;
            }

            if let Some(params) = match_route(route, &request_segments) {
                candidates.push((route, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<(&Route, Vec<(String, String)>)> = if method == "HEAD" {
            let has_head = candidates.iter().any(|(route, _)| route.method == "HEAD");
            let chosen_method = if has_head { "HEAD" } else { "GET" };
            candidates
                .into_iter()
                .filter(|(route, _)| route.method == chosen_method)
                .collect()
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
                    let trailing_ok =
                        self.policy != SlashPolicy::Strict || route.trailing == request_trailing;
                    trailing_ok && match_route(route, &request_segments).is_some()
                })
                .map(|route| route.method.clone())
                .collect();

            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();

            return Err(Error::MethodNotAllowed { allowed });
        }

        let mut eligible = eligible;
        eligible.sort_by(|(a, _), (b, _)| {
            b.rank
                .cmp(&a.rank)
                .then_with(|| a.id.cmp(&b.id))
        });

        let (route, params) = eligible.remove(0);
        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing != request_trailing
            && !request_segments.is_empty()
        {
            if route.trailing {
                Some(format!("{head}/"))
            } else {
                Some(head[..head.len() - 1].to_owned())
            }
        } else {
            None
        };

        Ok(Match {
            route: route.id,
            params,
            redirect,
        })
    }
}

fn match_route(route: &Route, request: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut i = 0;

    for segment in &route.segments {
        match segment {
            Segment::Static(value) => {
                if request.get(i)? != value {
                    return None;
                }
                i += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), request.get(i)?.clone()));
                i += 1;
            }
            Segment::Wildcard(name) => {
                let value = request[i..].join("/");
                params.push((name.clone(), value));
                i = request.len();
                break;
            }
        }
    }

    if i == request.len() {
        Some(params)
    } else {
        None
    }
}