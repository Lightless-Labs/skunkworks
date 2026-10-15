use std::collections::HashSet;

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
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
    rank: Vec<u8>,
}

#[derive(Debug, Clone)]
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
            method: method.to_string(),
            segments,
            trailing,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        validate_method(method)?;
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (segments, trailing) = parse_path(head)?;

        let mut candidates = Vec::new();
        for (id, route) in self.routes.iter().enumerate() {
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            if let Some(params) = match_route(route, &segments) {
                candidates.push((id, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<(usize, Vec<(String, String)>)> = if method == "HEAD" {
            if candidates
                .iter()
                .any(|(id, _)| self.routes[*id].method == "HEAD")
            {
                candidates
                    .into_iter()
                    .filter(|(id, _)| self.routes[*id].method == "HEAD")
                    .collect()
            } else {
                candidates
                    .into_iter()
                    .filter(|(id, _)| self.routes[*id].method == "GET")
                    .collect()
            }
        } else {
            candidates
                .into_iter()
                .filter(|(id, _)| self.routes[*id].method == method)
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed = HashSet::new();
            for (id, _) in &eligible_candidates_placeholder() {
                let _ = (id,);
            }
            // Recompute all path candidates; method filtering is deliberately separate.
            for (id, route) in self.routes.iter().enumerate() {
                if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                    continue;
                }
                if match_route(route, &segments).is_some() {
                    allowed.insert(route.method.clone());
                    let _ = id;
                }
            }
            if allowed.contains("GET") {
                allowed.insert("HEAD".to_string());
            }
            let mut allowed: Vec<String> = allowed.into_iter().collect();
            allowed.sort();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let (id, params) = eligible
            .into_iter()
            .max_by(|(a, _), (b, _)| {
                self.routes[*a]
                    .rank
                    .cmp(&self.routes[*b].rank)
                    .then_with(|| b.cmp(a))
            })
            .unwrap();

        let route = &self.routes[id];
        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing != trailing
            && !segments.is_empty()
        {
            let mut target = head.to_string();
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
            route: id,
            params,
            redirect,
        })
    }
}

fn eligible_candidates_placeholder() -> Vec<(usize, ())> {
    Vec::new()
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

fn parse_path(path: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    parse_segments(path, false).map_err(|at| Error::InvalidPath { at })
}

fn parse_segments(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = input.as_bytes();
    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = HashSet::new();
    let mut start = 1;

    while start < end {
        let stop = bytes[start..end]
            .iter()
            .position(|&b| b == b'/')
            .map(|p| start + p)
            .unwrap_or(end);

        if stop == start {
            return Err(start);
        }

        let raw = &input[start..stop];
        let first = bytes[start];

        if pattern && (first == b':' || first == b'*') {
            let wildcard = first == b'*';
            let name = &raw[1..];
            if !valid_name(name) || !names.insert(name.to_string()) {
                return Err(start);
            }
            if wildcard && stop != end {
                return Err(start);
            }
            segments.push(if wildcard {
                Segment::Wildcard(name.to_string())
            } else {
                Segment::Param(name.to_string())
            });
        } else {
            let decoded = decode_static(raw, start)?;
            segments.push(Segment::Static(decoded));
        }

        start = stop + 1;
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

fn decode_static(raw: &str, offset: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
                || bytes[i + 1].is_ascii_lowercase()
                || bytes[i + 2].is_ascii_lowercase()
            {
                return Err(offset + i);
            }
            let value = (hex(bytes[i + 1]).unwrap() << 4) | hex(bytes[i + 2]).unwrap();
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(offset + i);
            }
            out.push(value);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(offset + i);
        }
    }
    Ok(String::from_utf8(out).unwrap())
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_route(route: &Route, path: &[Segment]) -> Option<Vec<(String, String)>> {
    let wildcard_index = route
        .segments
        .iter()
        .position(|s| matches!(s, Segment::Wildcard(_)));

    let fixed_len = wildcard_index.unwrap_or(route.segments.len());
    if (wildcard_index.is_none() && path.len() != fixed_len)
        || (wildcard_index.is_some() && path.len() < fixed_len)
    {
        return None;
    }

    let mut params = Vec::new();
    for i in 0..fixed_len {
        match (&route.segments[i], &path[i]) {
            (Segment::Static(expected), Segment::Static(actual)) if expected == actual => {}
            (Segment::Param(name), Segment::Static(value)) => {
                params.push((name.clone(), value.clone()));
            }
            _ => return None,
        }
    }

    if let Some(index) = wildcard_index {
        let Segment::Wildcard(name) = &route.segments[index] else {
            unreachable!()
        };
        let value = path[index..]
            .iter()
            .map(|s| match s {
                Segment::Static(v) => v.as_str(),
                _ => "",
            })
            .collect::<Vec<_>>()
            .join("/");
        params.push((name.clone(), value));
    }

    Some(params)
}

fn main() {}