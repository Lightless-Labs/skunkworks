use std::collections::BTreeSet;

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
    rank: Vec<u8>,
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
        let mut rank = Vec::with_capacity(segments.len() + 1);
        let has_wildcard = segments.iter().any(|s| matches!(s, Segment::Wildcard(_)));
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
            method: method.to_owned(),
            segments,
            trailing,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        validate_method(method)?;
        let (head, segments, trailing) = parse_path(path)?;

        let mut candidates = Vec::new();
        for (index, route) in self.routes.iter().enumerate() {
            let slash_matches = self.policy != SlashPolicy::Strict || route.trailing == trailing;
            if slash_matches {
                if let Some(params) = match_route(&route.segments, &segments) {
                    candidates.push((index, params));
                }
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<(usize, Vec<(String, String)>)> = if method == "HEAD" {
            let has_head = candidates
                .iter()
                .any(|(i, _)| self.routes[*i].method == "HEAD");
            let selected_method = if has_head { "HEAD" } else { "GET" };
            candidates
                .into_iter()
                .filter(|(i, _)| self.routes[*i].method == selected_method)
                .collect()
        } else {
            candidates
                .into_iter()
                .filter(|(i, _)| self.routes[*i].method == method)
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed = BTreeSet::new();
            for (i, _) in candidates_for_allowed(&self.routes, &segments, trailing, self.policy) {
                allowed.insert(self.routes[i].method.clone());
            }
            if allowed.contains("GET") {
                allowed.insert("HEAD".to_owned());
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        let mut best = eligible[0].clone();
        for candidate in eligible.into_iter().skip(1) {
            let a = &self.routes[candidate.0].rank;
            let b = &self.routes[best.0].rank;
            if a > b {
                best = candidate;
            }
        }

        let route = &self.routes[best.0];
        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing != trailing
            && !segments.is_empty()
        {
            let mut target = head.to_owned();
            if route.trailing {
                target.push('/');
            } else {
                target.pop();
            }
            Some(target)
        } else {
            None
        };

        Ok(Match {
            route: best.0,
            params: best.1,
            redirect,
        })
    }
}

fn validate_method(method: &str) -> Result<(), Error> {
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        Err(Error::InvalidMethod)
    } else {
        Ok(())
    }
}

fn parse_pattern(input: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    parse_segments(input, true).map_err(|at| Error::InvalidPattern { at })
}

fn parse_path(input: &str) -> Result<(&str, Vec<String>, bool), Error> {
    let end = input
        .char_indices()
        .find(|(_, c)| *c == '?' || *c == '#')
        .map(|(i, _)| i)
        .unwrap_or(input.len());
    let head = &input[..end];
    if !head.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    let (segments, trailing) =
        parse_segments(head, false).map_err(|at| Error::InvalidPath { at })?;
    let decoded = segments
        .into_iter()
        .map(|s| match s {
            Segment::Static(v) => v,
            _ => unreachable!(),
        })
        .collect();
    Ok((head, decoded, trailing))
}

fn parse_segments(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    let bytes = input.as_bytes();
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = bytes.last() == Some(&b'/');
    let content_end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = BTreeSet::new();

    let mut start = 1;
    while start < content_end {
        let end = bytes[start..content_end]
            .iter()
            .position(|b| *b == b'/')
            .map(|p| start + p)
            .unwrap_or(content_end);

        if end == start {
            return Err(start);
        }

        let raw = &input[start..end];
        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_name(name) || !names.insert(name.to_owned()) {
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

        start = end + 1;
    }

    if pattern {
        // A trailing slash is already separated; interior empty segments were
        // rejected while scanning.
    }
    Ok((segments, trailing))
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_static(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = upper_hex(bytes[i + 1]).ok_or(base + i)?;
            let lo = upper_hex(bytes[i + 2]).ok_or(base + i)?;
            let decoded = hi * 16 + lo;
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

fn match_route(pattern: &[Segment], path: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut pi = 0;
    for segment in pattern {
        match segment {
            Segment::Static(value) => {
                if path.get(pi)? != value {
                    return None;
                }
                pi += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), path.get(pi)?.clone()));
                pi += 1;
            }
            Segment::Wildcard(name) => {
                params.push((name.clone(), path[pi..].join("/")));
                pi = path.len();
                break;
            }
        }
    }
    if pi == path.len() {
        Some(params)
    } else {
        None
    }
}

fn candidates_for_allowed(
    routes: &[Route],
    path: &[String],
    trailing: bool,
    policy: SlashPolicy,
) -> Vec<(usize, Vec<(String, String)>)> {
    routes
        .iter()
        .enumerate()
        .filter_map(|(i, route)| {
            let slash_matches = policy != SlashPolicy::Strict || route.trailing == trailing;
            if slash_matches {
                match_route(&route.segments, path).map(|params| (i, params))
            } else {
                None
            }
        })
        .collect()
}

fn main() {}