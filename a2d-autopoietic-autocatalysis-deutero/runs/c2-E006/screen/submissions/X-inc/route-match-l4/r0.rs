use std::collections::{HashMap, HashSet};

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

#[derive(Clone)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

struct Route {
    id: usize,
    name: String,
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
    rank: Vec<u8>,
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn uppercase_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_segment(segment: &str) -> Result<String, usize> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(i);
            }
            let (Some(hi), Some(lo)) =
                (uppercase_hex(bytes[i + 1]), uppercase_hex(bytes[i + 2]))
            else {
                return Err(i);
            };
            let value = hi * 16 + lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(i);
            }
            decoded.push(value);
            i += 3;
        } else if is_unreserved(b) {
            decoded.push(b);
            i += 1;
        } else {
            return Err(i);
        }
    }

    Ok(String::from_utf8(decoded).unwrap())
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn valid_route_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..].iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-'
        })
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.as_bytes().iter().all(u8::is_ascii_uppercase)
}

fn split_segments(input: &str) -> Result<(Vec<(&str, usize)>, bool), usize> {
    if !input.starts_with('/') {
        return Err(0);
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = input.ends_with('/');
    let end = if trailing {
        input.len() - 1
    } else {
        input.len()
    };

    let mut segments = Vec::new();
    let mut start = 1;
    let bytes = input.as_bytes();

    for i in 1..end {
        if bytes[i] == b'/' {
            if i == start {
                return Err(start);
            }
            segments.push((&input[start..i], start));
            start = i + 1;
        }
    }

    if start == end {
        return Err(start);
    }

    segments.push((&input[start..end], start));
    Ok((segments, trailing))
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool, Vec<u8>), Error> {
    let (parts, trailing) =
        split_segments(pattern).map_err(|at| Error::InvalidPattern { at })?;
    let mut segments = Vec::with_capacity(parts.len());
    let mut names = HashSet::new();
    let mut rank = Vec::with_capacity(parts.len() + 1);

    for (index, (raw, at)) in parts.iter().enumerate() {
        if raw.starts_with(':') || raw.starts_with('*') {
            let wildcard = raw.as_bytes()[0] == b'*';
            let name = &raw[1..];

            if !valid_capture_name(name) {
                return Err(Error::InvalidPattern { at: *at });
            }
            if !names.insert(name.to_string()) {
                return Err(Error::InvalidPattern { at: *at });
            }
            if wildcard && index + 1 != parts.len() {
                return Err(Error::InvalidPattern { at: *at });
            }

            if wildcard {
                segments.push(Segment::Wildcard(name.to_string()));
                rank.push(0);
            } else {
                segments.push(Segment::Param(name.to_string()));
                rank.push(1);
            }
        } else {
            let decoded = decode_segment(raw)
                .map_err(|offset| Error::InvalidPattern { at: *at + offset })?;
            segments.push(Segment::Static(decoded));
            rank.push(2);
        }
    }

    if !segments
        .iter()
        .any(|segment| matches!(segment, Segment::Wildcard(_)))
    {
        rank.push(3);
    }

    Ok((segments, trailing, rank))
}

fn parse_path_head(path: &str) -> Result<(Vec<String>, bool), Error> {
    let end = path
        .char_indices()
        .find(|(_, ch)| *ch == '?' || *ch == '#')
        .map(|(index, _)| index)
        .unwrap_or(path.len());
    let head = &path[..end];

    let (parts, trailing) =
        split_segments(head).map_err(|at| Error::InvalidPath { at })?;
    let mut decoded = Vec::with_capacity(parts.len());

    for (raw, at) in parts {
        decoded.push(
            decode_segment(raw)
                .map_err(|offset| Error::InvalidPath { at: at + offset })?,
        );
    }

    Ok((decoded, trailing))
}

fn same_shape(
    existing: &Route,
    segments: &[Segment],
    trailing: bool,
    policy: SlashPolicy,
) -> bool {
    if existing.segments.len() != segments.len() {
        return false;
    }
    if policy == SlashPolicy::Strict && existing.trailing != trailing {
        return false;
    }

    existing
        .segments
        .iter()
        .zip(segments)
        .all(|(left, right)| match (left, right) {
            (Segment::Static(a), Segment::Static(b)) => a == b,
            (Segment::Param(_), Segment::Param(_)) => true,
            (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
            _ => false,
        })
}

fn path_candidate(
    route: &Route,
    path: &[String],
    trailing: bool,
    policy: SlashPolicy,
) -> Option<Vec<(String, String)>> {
    if policy == SlashPolicy::Strict && route.trailing != trailing {
        return None;
    }

    let wildcard = route.segments.last().and_then(|segment| match segment {
        Segment::Wildcard(name) => Some(name),
        _ => None,
    });
    let fixed_len = if wildcard.is_some() {
        route.segments.len() - 1
    } else {
        route.segments.len()
    };

    if wildcard.is_none() && path.len() != fixed_len {
        return None;
    }
    if wildcard.is_some() && path.len() < fixed_len {
        return None;
    }

    let mut params = Vec::new();

    for i in 0..fixed_len {
        match &route.segments[i] {
            Segment::Static(value) if value == &path[i] => {}
            Segment::Static(_) => return None,
            Segment::Param(name) => params.push((name.clone(), path[i].clone())),
            Segment::Wildcard(_) => return None,
        }
    }

    if let Some(name) = wildcard {
        params.push((name.clone(), path[fixed_len..].join("/")));
    }

    Some(params)
}

fn encode_component(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::new();

    for &byte in bytes {
        if is_unreserved(byte) {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }

    encoded
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

        let (segments, trailing, rank) = parse_pattern(pattern)?;

        if self.routes.iter().any(|route| route.name == name) {
            return Err(Error::DuplicateName);
        }

        if let Some(route) = self
            .routes
            .iter()
            .filter(|route| route.method == method)
            .find(|route| same_shape(route, &segments, trailing, self.policy))
        {
            return Err(Error::Conflict { with: route.id });
        }

        let id = self.routes.len();
        self.routes.push(Route {
            id,
            name: name.to_string(),
            method: method.to_string(),
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

        let (decoded, trailing) = parse_path_head(path)?;
        let candidates: Vec<(&Route, Vec<(String, String)>)> = self
            .routes
            .iter()
            .filter_map(|route| {
                path_candidate(route, &decoded, trailing, self.policy)
                    .map(|params| (route, params))
            })
            .collect();

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let selected_method = if method == "HEAD"
            && candidates.iter().any(|(route, _)| route.method == "HEAD")
        {
            "HEAD"
        } else if method == "HEAD" {
            "GET"
        } else {
            method
        };

        let mut eligible: Vec<(&Route, Vec<(String, String)>)> = candidates
            .iter()
            .filter(|(route, _)| route.method == selected_method)
            .map(|(route, params)| (*route, params.clone()))
            .collect();

        if eligible.is_empty() {
            let mut allowed: Vec<String> = candidates
                .iter()
                .map(|(route, _)| route.method.clone())
                .collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_string());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        eligible.sort_by(|(a, _), (b, _)| {
            b.rank.cmp(&a.rank).then_with(|| a.id.cmp(&b.id))
        });

        let (route, params) = eligible.remove(0);
        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing != trailing
            && !decoded.is_empty()
        {
            let head_end = path
                .char_indices()
                .find(|(_, ch)| *ch == '?' || *ch == '#')
                .map(|(index, _)| index)
                .unwrap_or(path.len());
            let mut target = path[..head_end].to_string();

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
        for &(key, value) in params {
            if supplied.contains_key(key) {
                return Err(Error::DuplicateParam {
                    name: key.to_string(),
                });
            }
            supplied.insert(key, value);
        }

        let captures: Vec<&str> = route
            .segments
            .iter()
            .filter_map(|segment| match segment {
                Segment::Param(name) | Segment::Wildcard(name) => Some(name.as_str()),
                Segment::Static(_) => None,
            })
            .collect();

        for &key in &captures {
            if !supplied.contains_key(key) {
                return Err(Error::MissingParam {
                    name: key.to_string(),
                });
            }
        }

        let mut extras: Vec<&str> = supplied
            .keys()
            .copied()
            .filter(|key| !captures.contains(key))
            .collect();
        extras.sort();
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam {
                name: (*key).to_string(),
            });
        }

        for &key in &captures {
            let value = supplied[key];
            let ascii = value
                .as_bytes()
                .iter()
                .all(|byte| (0x20..=0x7e).contains(byte));
            let wildcard = route
                .segments
                .iter()
                .any(|segment| matches!(segment, Segment::Wildcard(name) if name == key));
            let shape_valid = if wildcard {
                value.is_empty() || value.split('/').all(|part| !part.is_empty())
            } else {
                !value.is_empty() && !value.contains('/')
            };

            if !ascii || !shape_valid {
                return Err(Error::InvalidValue {
                    name: key.to_string(),
                });
            }
        }

        let mut output = String::new();

        for segment in &route.segments {
            match segment {
                Segment::Static(value) => {
                    output.push('/');
                    output.push_str(&encode_component(value.as_bytes()));
                }
                Segment::Param(name) => {
                    output.push('/');
                    output.push_str(&encode_component(supplied[name.as_str()].as_bytes()));
                }
                Segment::Wildcard(name) => {
                    let value = supplied[name.as_str()];
                    if !value.is_empty() {
                        for part in value.split('/') {
                            output.push('/');
                            output.push_str(&encode_component(part.as_bytes()));
                        }
                    }
                }
            }
        }

        if output.is_empty() {
            output.push('/');
        }
        if route.trailing && output != "/" && !output.ends_with('/') {
            output.push('/');
        }

        Ok(output)
    }
}