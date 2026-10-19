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

        for (id, route) in self.routes.iter().enumerate() {
            if route.method == method
                && same_shape(&route.segments, &segments)
                && (self.policy != SlashPolicy::Strict || route.trailing == trailing)
            {
                return Err(Error::Conflict { with: id });
            }
        }

        let id = self.routes.len();
        self.routes.push(Route {
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
        let parsed = parse_request_path(head)?;

        let mut candidates = Vec::new();
        for (id, route) in self.routes.iter().enumerate() {
            if self.policy == SlashPolicy::Strict && route.trailing != parsed.trailing {
                continue;
            }
            if let Some(params) = match_route(route, &parsed.segments) {
                candidates.push((id, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible_method = if method == "HEAD" {
            if candidates
                .iter()
                .any(|(id, _)| self.routes[*id].method == "HEAD")
            {
                "HEAD"
            } else {
                "GET"
            }
        } else {
            method
        };

        let eligible: Vec<_> = candidates
            .into_iter()
            .filter(|(id, _)| self.routes[*id].method == eligible_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed: Vec<String> = self
                .routes
                .iter()
                .enumerate()
                .filter(|(id, _)| {
                    // The route must have matched the path; recompute because candidates
                    // were consumed only after the method decision.
                    let route = &self.routes[*id];
                    (self.policy != SlashPolicy::Strict || route.trailing == parsed.trailing)
                        && match_route(route, &parsed.segments).is_some()
                })
                .map(|(_, route)| route.method.clone())
                .collect();

            if allowed.iter().any(|m| m == "GET") && !allowed.iter().any(|m| m == "HEAD") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let (id, params) = eligible
            .into_iter()
            .max_by(|(ida, _), (idb, _)| {
                rank(&self.routes[*ida].segments)
                    .cmp(&rank(&self.routes[*idb].segments))
                    .then_with(|| idb.cmp(ida))
            })
            .expect("eligible candidates are nonempty");

        let route = &self.routes[id];
        let redirect = if self.policy == SlashPolicy::Redirect
            && !parsed.segments.is_empty()
            && route.trailing != parsed.trailing
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
            route: id,
            params,
            redirect,
        })
    }

    pub fn build(&self, name: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let route = self
            .routes
            .iter()
            .find(|route| route.name == name)
            .ok_or(Error::UnknownName)?;

        let mut supplied = Vec::with_capacity(params.len());
        let mut seen = HashSet::new();
        for (key, value) in params {
            if !seen.insert(*key) {
                return Err(Error::DuplicateParam {
                    name: (*key).to_owned(),
                });
            }
            supplied.push((*key, *value));
        }

        let required: Vec<&str> = route
            .segments
            .iter()
            .filter_map(|segment| match segment {
                Segment::Param(name) | Segment::Wildcard(name) => Some(name.as_str()),
                Segment::Static(_) => None,
            })
            .collect();

        for key in &required {
            if !supplied.iter().any(|(given, _)| given == key) {
                return Err(Error::MissingParam {
                    name: (*key).to_owned(),
                });
            }
        }

        let mut extras: Vec<&str> = supplied
            .iter()
            .map(|(key, _)| *key)
            .filter(|key| !required.iter().any(|required| required == key))
            .collect();
        extras.sort();
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam {
                name: (*key).to_string(),
            });
        }

        for key in &required {
            let value = supplied
                .iter()
                .find(|(given, _)| given == key)
                .map(|(_, value)| *value)
                .expect("required keys have already been checked");
            let wildcard = route
                .segments
                .iter()
                .any(|segment| matches!(segment, Segment::Wildcard(n) if n == key));
            if !valid_build_value(value, wildcard) {
                return Err(Error::InvalidValue {
                    name: (*key).to_owned(),
                });
            }
        }

        let mut output = String::from("/");
        let mut emitted = 0usize;
        for segment in &route.segments {
            match segment {
                Segment::Static(value) => {
                    if emitted > 0 {
                        output.push('/');
                    }
                    encode_segment(value, &mut output);
                    emitted += 1;
                }
                Segment::Param(key) => {
                    if emitted > 0 {
                        output.push('/');
                    }
                    let value = supplied
                        .iter()
                        .find(|(given, _)| given == key)
                        .map(|(_, value)| *value)
                        .expect("required parameter exists");
                    encode_segment(value, &mut output);
                    emitted += 1;
                }
                Segment::Wildcard(key) => {
                    let value = supplied
                        .iter()
                        .find(|(given, _)| given == key)
                        .map(|(_, value)| *value)
                        .expect("required wildcard exists");
                    if !value.is_empty() {
                        if emitted > 0 {
                            output.push('/');
                        }
                        for (index, component) in value.split('/').enumerate() {
                            if index > 0 {
                                output.push('/');
                            }
                            encode_segment(component, &mut output);
                        }
                        emitted += value.split('/').count();
                    }
                }
            }
        }

        if route.trailing && emitted > 0 {
            output.push('/');
        }
        Ok(output)
    }
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_route_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-')
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = pattern.as_bytes();
    let mut trailing = false;
    let end = if bytes.last() == Some(&b'/') {
        trailing = true;
        bytes.len() - 1
    } else {
        bytes.len()
    };

    let mut segments = Vec::new();
    let mut names = HashSet::new();
    let mut start = 1usize;

    while start < end {
        let slash = bytes[start..end]
            .iter()
            .position(|b| *b == b'/')
            .map(|p| start + p)
            .unwrap_or(end);
        if slash == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &pattern[start..slash];
        let first = raw.as_bytes()[0];
        if first == b':' || first == b'*' {
            let name = &raw[1..];
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if first == b'*' && slash != end {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(if first == b':' {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            });
        } else {
            let decoded = decode_segment(raw, start).map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }
        start = slash + 1;
    }

    if segments.is_empty() {
        return Err(Error::InvalidPattern { at: 1 });
    }
    Ok((segments, trailing))
}

fn parse_request_path(path: &str) -> Result<ParsedPath, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(ParsedPath {
            segments: Vec::new(),
            trailing: false,
        });
    }

    let bytes = path.as_bytes();
    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut start = 1usize;

    while start < end {
        let slash = bytes[start..end]
            .iter()
            .position(|b| *b == b'/')
            .map(|p| start + p)
            .unwrap_or(end);
        if slash == start {
            return Err(Error::InvalidPath { at: start });
        }
        let raw = &path[start..slash];
        let decoded = decode_segment(raw, start).map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);
        start = slash + 1;
    }

    if segments.is_empty() {
        return Err(Error::InvalidPath { at: 1 });
    }
    Ok(ParsedPath { segments, trailing })
}

fn decode_segment(raw: &str, offset: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(offset + i);
            }
            let hi = upper_hex(bytes[i + 1]).ok_or(offset + i)?;
            let lo = upper_hex(bytes[i + 2]).ok_or(offset + i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(offset + i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(offset + i);
        }
    }
    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
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

fn same_shape(a: &[Segment], b: &[Segment]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(left, right)| match (left, right) {
            (Segment::Static(x), Segment::Static(y)) => x == y,
            (Segment::Param(_), Segment::Param(_)) => true,
            (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
            _ => false,
        })
}

fn match_route(route: &Route, path: &[String]) -> Option<Vec<(String, String)>> {
    let wildcard = matches!(route.segments.last(), Some(Segment::Wildcard(_)));
    let fixed_len = if wildcard {
        route.segments.len() - 1
    } else {
        route.segments.len()
    };

    if (!wildcard && path.len() != fixed_len) || (wildcard && path.len() < fixed_len) {
        return None;
    }

    let mut params = Vec::new();
    for (segment, value) in route.segments[..fixed_len].iter().zip(path.iter()) {
        match segment {
            Segment::Static(expected) if expected != value => return None,
            Segment::Static(_) => {}
            Segment::Param(name) => params.push((name.clone(), value.clone())),
            Segment::Wildcard(_) => return None,
        }
    }

    if wildcard {
        let Segment::Wildcard(name) = route.segments.last().expect("wildcard exists") else {
            unreachable!()
        };
        params.push((name.clone(), path[fixed_len..].join("/")));
    }
    Some(params)
}

fn rank(segments: &[Segment]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut has_wildcard = false;
    for segment in segments {
        match segment {
            Segment::Static(_) => result.push(2),
            Segment::Param(_) => result.push(1),
            Segment::Wildcard(_) => {
                result.push(0);
                has_wildcard = true;
                break;
            }
        }
    }
    if !has_wildcard {
        result.push(3);
    }
    result
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

fn encode_segment(value: &str, output: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in value.bytes() {
        if is_unreserved(b) {
            output.push(b as char);
        } else {
            output.push('%');
            output.push(HEX[(b >> 4) as usize] as char);
            output.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
}