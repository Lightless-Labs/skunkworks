use std::collections::{BTreeSet, HashMap, HashSet};

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

#[derive(Clone, Debug, PartialEq, Eq)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Clone)]
struct Route {
    id: usize,
    name: String,
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
    rank: Vec<u8>,
}

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

        let candidate = Route {
            id: self.routes.len(),
            name: name.to_string(),
            method: method.to_string(),
            rank: make_rank(&segments),
            segments,
            trailing,
        };

        if let Some(existing) = self
            .routes
            .iter()
            .filter(|route| {
                route.method == candidate.method
                    && (self.policy != SlashPolicy::Strict
                        || route.trailing == candidate.trailing)
                    && same_shape(&route.segments, &candidate.segments)
            })
            .map(|route| route.id)
            .min()
        {
            return Err(Error::Conflict { with: existing });
        }

        let id = candidate.id;
        self.routes.push(candidate);
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }

        let head_end = path
            .char_indices()
            .find(|(_, ch)| *ch == '?' || *ch == '#')
            .map(|(index, _)| index)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let parsed = parse_request_path(head)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = self
            .routes
            .iter()
            .filter_map(|route| {
                if self.policy == SlashPolicy::Strict && route.trailing != parsed.trailing {
                    return None;
                }
                match_route(route, &parsed.segments).map(|params| (route, params))
            })
            .collect();

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        if method == "HEAD" {
            if candidates.iter().any(|(route, _)| route.method == "HEAD") {
                candidates.retain(|(route, _)| route.method == "HEAD");
            } else {
                candidates.retain(|(route, _)| route.method == "GET");
            }
        } else {
            candidates.retain(|(route, _)| route.method == method);
        }

        if candidates.is_empty() {
            let mut allowed = BTreeSet::new();
            for route in &self.routes {
                let is_candidate = (self.policy != SlashPolicy::Strict
                    || route.trailing == parsed.trailing)
                    && match_route(route, &parsed.segments).is_some();
                if is_candidate {
                    allowed.insert(route.method.clone());
                    if route.method == "GET" {
                        allowed.insert("HEAD".to_string());
                    }
                }
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        let (winner, params) = candidates
            .into_iter()
            .max_by(|(a, _), (b, _)| a.rank.cmp(&b.rank).then_with(|| b.id.cmp(&a.id)))
            .unwrap();

        let redirect = if self.policy == SlashPolicy::Redirect
            && winner.trailing != parsed.trailing
            && !parsed.segments.is_empty()
        {
            let mut target = head.to_string();
            if winner.trailing {
                target.push('/');
            } else if target.ends_with('/') {
                target.pop();
            }
            Some(target)
        } else {
            None
        };

        Ok(Match {
            route: winner.id,
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

        let mut supplied: HashMap<&str, &str> = HashMap::new();
        for (key, value) in params {
            if supplied.contains_key(key) {
                return Err(Error::DuplicateParam {
                    name: (*key).to_string(),
                });
            }
            supplied.insert(*key, *value);
        }

        let mut required = Vec::new();
        for segment in &route.segments {
            if let Segment::Param(key) | Segment::Wildcard(key) = segment {
                required.push(key.as_str());
                if !supplied.contains_key(key.as_str()) {
                    return Err(Error::MissingParam { name: key.clone() });
                }
            }
        }

        let required_set: HashSet<&str> = required.into_iter().collect();
        if let Some(key) = supplied
            .keys()
            .filter(|key| !required_set.contains(**key))
            .min_by(|a, b| a.as_bytes().cmp(b.as_bytes()))
        {
            return Err(Error::ExtraParam {
                name: (*key).to_string(),
            });
        }

        for segment in &route.segments {
            match segment {
                Segment::Param(key) => {
                    let value = supplied[key.as_str()];
                    if !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
                        || value.is_empty()
                        || value.contains('/')
                    {
                        return Err(Error::InvalidValue { name: key.clone() });
                    }
                }
                Segment::Wildcard(key) => {
                    let value = supplied[key.as_str()];
                    if !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
                        || (!value.is_empty()
                            && value.split('/').any(|part| part.is_empty()))
                    {
                        return Err(Error::InvalidValue { name: key.clone() });
                    }
                }
                Segment::Static(_) => {}
            }
        }

        let mut output = String::from("/");
        for (index, segment) in route.segments.iter().enumerate() {
            if index > 0 {
                output.push('/');
            }
            match segment {
                Segment::Static(text) => output.push_str(&encode_component(text)),
                Segment::Param(key) => {
                    output.push_str(&encode_component(supplied[key.as_str()]));
                }
                Segment::Wildcard(key) => {
                    let value = supplied[key.as_str()];
                    if !value.is_empty() {
                        output.push_str(
                            &value
                                .split('/')
                                .map(encode_component)
                                .collect::<Vec<_>>()
                                .join("/"),
                        );
                    }
                }
            }
        }

        if route.trailing && !route.segments.is_empty() {
            output.push('/');
        }
        Ok(output)
    }
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_route_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..].iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_' || *byte == b'-'
        })
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), Error> {
    let bytes = pattern.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = pattern.ends_with('/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = HashSet::new();
    let mut start = 1usize;

    while start < end {
        let stop = bytes[start..end]
            .iter()
            .position(|byte| *byte == b'/')
            .map(|offset| start + offset)
            .unwrap_or(end);

        if stop == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &pattern[start..stop];
        if raw.starts_with(':') || raw.starts_with('*') {
            let marker = bytes[start];
            let capture_name = &raw[1..];
            if !valid_capture_name(capture_name) || !names.insert(capture_name.to_string()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if marker == b'*' && stop != end {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(if marker == b':' {
                Segment::Param(capture_name.to_string())
            } else {
                Segment::Wildcard(capture_name.to_string())
            });
        } else {
            let decoded =
                decode_static(raw, start).map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }

        start = stop + 1;
    }

    if segments.is_empty() {
        return Err(Error::InvalidPattern { at: 1 });
    }
    Ok((segments, trailing))
}

fn parse_request_path(path: &str) -> Result<ParsedPath, Error> {
    let bytes = path.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(ParsedPath {
            segments: Vec::new(),
            trailing: false,
        });
    }

    let trailing = path.ends_with('/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut start = 1usize;

    while start < end {
        let stop = bytes[start..end]
            .iter()
            .position(|byte| *byte == b'/')
            .map(|offset| start + offset)
            .unwrap_or(end);

        if stop == start {
            return Err(Error::InvalidPath { at: start });
        }

        let decoded =
            decode_static(&path[start..stop], start).map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);
        start = stop + 1;
    }

    if segments.is_empty() {
        return Err(Error::InvalidPath { at: 1 });
    }
    Ok(ParsedPath { segments, trailing })
}

fn decode_static(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= bytes.len() {
                return Err(base + index);
            }
            let high = upper_hex(bytes[index + 1]).ok_or(base + index)?;
            let low = upper_hex(bytes[index + 2]).ok_or(base + index)?;
            let decoded = high * 16 + low;
            if !(0x20..=0x7e).contains(&decoded) || decoded == b'/' || is_unreserved(decoded) {
                return Err(base + index);
            }
            output.push(decoded);
            index += 3;
        } else {
            if !is_unreserved(byte) {
                return Err(base + index);
            }
            output.push(byte);
            index += 1;
        }
    }

    Ok(String::from_utf8(output).unwrap())
}

fn upper_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn make_rank(segments: &[Segment]) -> Vec<u8> {
    let mut rank = Vec::new();
    for segment in segments {
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

fn same_shape(a: &[Segment], b: &[Segment]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(left, right)| match (left, right) {
            (Segment::Static(a), Segment::Static(b)) => a == b,
            (Segment::Param(_), Segment::Param(_))
            | (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
            _ => false,
        })
}

fn match_route(route: &Route, path: &[String]) -> Option<Vec<(String, String)>> {
    let wildcard_index = route
        .segments
        .iter()
        .position(|segment| matches!(segment, Segment::Wildcard(_)));

    let fixed_count = wildcard_index.unwrap_or(route.segments.len());
    if wildcard_index.is_none() && path.len() != fixed_count {
        return None;
    }
    if wildcard_index.is_some() && path.len() < fixed_count {
        return None;
    }

    let mut params = Vec::new();
    for (index, segment) in route.segments.iter().take(fixed_count).enumerate() {
        match segment {
            Segment::Static(text) if text != &path[index] => return None,
            Segment::Static(_) => {}
            Segment::Param(name) => params.push((name.clone(), path[index].clone())),
            Segment::Wildcard(_) => unreachable!(),
        }
    }

    if let Some(index) = wildcard_index {
        if let Segment::Wildcard(name) = &route.segments[index] {
            params.push((name.clone(), path[index..].join("/")));
        }
    }
    Some(params)
}

fn encode_component(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if is_unreserved(byte) {
            result.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            result.push('%');
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    result
}

fn main() {}