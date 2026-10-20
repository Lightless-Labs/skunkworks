use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPattern { at: usize },
    InvalidPath { at: usize },
    NotFound,
    InvalidMethod,
    MethodNotAllowed { allowed: Vec<String> },
    InvalidName,
    DuplicateName,
    Conflict { with: usize },
    UnknownName,
    DuplicateParam { name: String },
    MissingParam { name: String },
    ExtraParam { name: String },
    InvalidValue { name: String },
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Debug, Clone)]
struct Route {
    id: usize,
    name: String,
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
}

#[derive(Debug, Clone)]
struct ParsedPath {
    segments: Vec<String>,
    trailing: bool,
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

    pub fn add(&mut self, name: &str, method: &str, pattern: &str) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        if !valid_route_name(name) {
            return Err(Error::InvalidName);
        }
        let (segments, trailing) = parse_pattern(pattern)?;
        if self.routes.iter().any(|route| route.name == name) {
            return Err(Error::DuplicateName);
        }
        for route in &self.routes {
            if route.method == method && shapes_conflict(route, &segments, trailing, self.policy) {
                return Err(Error::Conflict { with: route.id });
            }
        }
        let id = self.routes.len();
        self.routes.push(Route {
            id,
            name: name.to_owned(),
            method: method.to_owned(),
            segments,
            trailing,
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
        let parsed = parse_request(head)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>, Vec<u8>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != parsed.trailing {
                continue;
            }
            if let Some(params) = match_segments(&route.segments, &parsed.segments) {
                candidates.push((route, params, rank(&route.segments)));
            }
        }
        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible_method = if method == "HEAD" {
            if candidates.iter().any(|(r, _, _)| r.method == "HEAD") {
                "HEAD"
            } else {
                "GET"
            }
        } else {
            method
        };
        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(r, _, _)| r.method == eligible_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed: Vec<String> = candidates
                .iter()
                .map(|(r, _, _)| r.method.clone())
                .collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let winner = eligible
            .into_iter()
            .max_by(|a, b| {
                a.2.cmp(&b.2)
                    .then_with(|| b.0.id.cmp(&a.0.id))
            })
            .unwrap();
        let redirect = if self.policy == SlashPolicy::Redirect
            && !parsed.segments.is_empty()
            && winner.0.trailing != parsed.trailing
        {
            let mut target = head.to_owned();
            if winner.0.trailing {
                target.push('/');
            } else {
                target.pop();
            }
            Some(target)
        } else {
            None
        };
        Ok(Match {
            route: winner.0.id,
            params: winner.1.clone(),
            redirect,
        })
    }

    pub fn build(&self, name: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let route = self.routes.iter().find(|r| r.name == name).ok_or(Error::UnknownName)?;

        let mut supplied = Vec::new();
        let mut seen = HashSet::new();
        for &(key, value) in params {
            if !seen.insert(key) {
                return Err(Error::DuplicateParam {
                    name: key.to_owned(),
                });
            }
            supplied.push((key, value));
        }

        let captures: Vec<&str> = route
            .segments
            .iter()
            .filter_map(|s| match s {
                Segment::Param(n) | Segment::Wildcard(n) => Some(n.as_str()),
                Segment::Static(_) => None,
            })
            .collect();

        for key in &captures {
            if !supplied.iter().any(|(k, _)| k == key) {
                return Err(Error::MissingParam {
                    name: (*key).to_owned(),
                });
            }
        }
        let mut extras: Vec<&str> = supplied
            .iter()
            .map(|(k, _)| *k)
            .filter(|k| !captures.contains(k))
            .collect();
        extras.sort();
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam {
                name: (*key).to_string(),
            });
        }

        for segment in &route.segments {
            let (name, wildcard) = match segment {
                Segment::Param(n) => (n, false),
                Segment::Wildcard(n) => (n, true),
                Segment::Static(_) => continue,
            };
            let value = supplied.iter().find(|(k, _)| *k == name).unwrap().1;
            if !valid_build_value(value, wildcard) {
                return Err(Error::InvalidValue { name: name.clone() });
            }
        }

        let mut output = String::from("/");
        let mut emitted = false;
        for segment in &route.segments {
            match segment {
                Segment::Static(s) => {
                    if emitted {
                        output.push('/');
                    }
                    output.push_str(&encode_segment(s));
                    emitted = true;
                }
                Segment::Param(name) => {
                    if emitted {
                        output.push('/');
                    }
                    let value = supplied.iter().find(|(k, _)| *k == name).unwrap().1;
                    output.push_str(&encode_segment(value));
                    emitted = true;
                }
                Segment::Wildcard(name) => {
                    let value = supplied.iter().find(|(k, _)| *k == name).unwrap().1;
                    if !value.is_empty() {
                        if emitted {
                            output.push('/');
                        }
                        output.push_str(
                            &value
                                .split('/')
                                .map(encode_segment)
                                .collect::<Vec<_>>()
                                .join("/"),
                        );
                        emitted = true;
                    }
                }
            }
        }
        if route.trailing && output != "/" {
            output.push('/');
        }
        Ok(output)
    }
}

fn valid_method(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_route_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-')
}

fn valid_capture_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn hex_upper(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_static_segment(s: &str, base: usize, pattern: bool) -> Result<String, Error> {
    let bytes = s.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(parse_error(pattern, base + i));
            }
            let Some(h1) = hex_upper(bytes[i + 1]) else {
                return Err(parse_error(pattern, base + i));
            };
            let Some(h2) = hex_upper(bytes[i + 2]) else {
                return Err(parse_error(pattern, base + i));
            };
            let value = h1 * 16 + h2;
            if !(0x20..=0x7e).contains(&value) || value == b'/' || is_unreserved(value) {
                return Err(parse_error(pattern, base + i));
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(parse_error(pattern, base + i));
        }
    }
    Ok(String::from_utf8(decoded).unwrap())
}

fn parse_error(pattern: bool, at: usize) -> Error {
    if pattern {
        Error::InvalidPattern { at }
    } else {
        Error::InvalidPath { at }
    }
}

fn parse_pattern(input: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }
    let (parts, offsets, trailing) = split_nonroot(input);
    let mut segments = Vec::with_capacity(parts.len());
    let mut names = HashSet::new();

    for (i, part) in parts.iter().enumerate() {
        let at = offsets[i];
        if part.is_empty() {
            return Err(Error::InvalidPattern { at });
        }
        if let Some(name) = part.strip_prefix(':') {
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at });
            }
            segments.push(Segment::Param(name.to_owned()));
        } else if let Some(name) = part.strip_prefix('*') {
            if i + 1 != parts.len() {
                return Err(Error::InvalidPattern { at });
            }
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at });
            }
            segments.push(Segment::Wildcard(name.to_owned()));
        } else {
            segments.push(Segment::Static(decode_static_segment(part, at, true)?));
        }
    }
    Ok((segments, trailing))
}

fn parse_request(input: &str) -> Result<ParsedPath, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if input == "/" {
        return Ok(ParsedPath {
            segments: Vec::new(),
            trailing: false,
        });
    }
    let (parts, offsets, trailing) = split_nonroot(input);
    let mut segments = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return Err(Error::InvalidPath { at: offsets[i] });
        }
        segments.push(decode_static_segment(part, offsets[i], false)?);
    }
    Ok(ParsedPath { segments, trailing })
}

fn split_nonroot(input: &str) -> (Vec<&str>, Vec<usize>, bool) {
    let trailing = input.ends_with('/');
    let end = if trailing { input.len() - 1 } else { input.len() };
    let body = &input[1..end];
    let mut parts = Vec::new();
    let mut offsets = Vec::new();
    let mut start = 0;
    for (i, b) in body.bytes().enumerate() {
        if b == b'/' {
            parts.push(&body[start..i]);
            offsets.push(1 + start);
            start = i + 1;
        }
    }
    parts.push(&body[start..]);
    offsets.push(1 + start);
    (parts, offsets, trailing)
}

fn shapes_conflict(route: &Route, new: &[Segment], trailing: bool, policy: SlashPolicy) -> bool {
    if route.segments.len() != new.len() {
        return false;
    }
    if policy == SlashPolicy::Strict && route.trailing != trailing {
        return false;
    }
    route
        .segments
        .iter()
        .zip(new)
        .all(|(a, b)| match (a, b) {
            (Segment::Static(x), Segment::Static(y)) => x == y,
            (Segment::Param(_), Segment::Param(_)) => true,
            (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
            _ => false,
        })
}

fn match_segments(pattern: &[Segment], path: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut pi = 0;
    for segment in pattern {
        match segment {
            Segment::Static(s) => {
                if path.get(pi)? != s {
                    return None;
                }
                pi += 1;
            }
            Segment::Param(name) => {
                let value = path.get(pi)?;
                params.push((name.clone(), value.clone()));
                pi += 1;
            }
            Segment::Wildcard(name) => {
                let value = path[pi..].join("/");
                params.push((name.clone(), value));
                return Some(params);
            }
        }
    }
    if pi == path.len() {
        Some(params)
    } else {
        None
    }
}

fn rank(pattern: &[Segment]) -> Vec<u8> {
    let mut rank = Vec::new();
    for segment in pattern {
        match segment {
            Segment::Static(_) => rank.push(2),
            Segment::Param(_) => rank.push(1),
            Segment::Wildcard(_) => {
                rank.push(0);
                return rank;
            }
        }
    }
    rank.push(3);
    rank
}

fn valid_build_value(value: &str, wildcard: bool) -> bool {
    if !value.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return false;
    }
    if wildcard {
        value.is_empty()
            || value
                .split('/')
                .all(|part| !part.is_empty())
    } else {
        !value.is_empty() && !value.contains('/')
    }
}

fn encode_segment(value: &str) -> String {
    let mut output = String::new();
    for b in value.bytes() {
        if is_unreserved(b) {
            output.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(HEX[(b >> 4) as usize] as char);
            output.push(HEX[(b & 15) as usize] as char);
        }
    }
    output
}