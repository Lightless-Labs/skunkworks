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
    rank: Vec<u8>,
}

#[derive(Debug)]
pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_route_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
}

fn valid_capture_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

fn hex_upper(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    matches!(
        b,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'~' | b'-'
    )
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
            let hi = hex_upper(bytes[i + 1]).ok_or(offset + i)?;
            let lo = hex_upper(bytes[i + 2]).ok_or(offset + i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value) || value == b'/' || is_unreserved(value) {
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

    Ok(String::from_utf8(decoded).expect("decoded segment is ASCII"))
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = pattern.as_bytes();
    let mut raw_segments: Vec<(usize, &str)> = Vec::new();
    let mut start = 1;

    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            if i == start {
                return Err(Error::InvalidPattern { at: start });
            }
            raw_segments.push((start, &pattern[start..i]));
            start = i + 1;
        }
    }

    let trailing = start == bytes.len();
    if !trailing {
        raw_segments.push((start, &pattern[start..]));
    }

    let mut segments = Vec::with_capacity(raw_segments.len());
    let mut names = HashSet::new();

    for (index, (at, raw)) in raw_segments.iter().enumerate() {
        if raw.starts_with(':') || raw.starts_with('*') {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: *at });
            }
            if marker == b'*' && index + 1 != raw_segments.len() {
                return Err(Error::InvalidPattern { at: *at });
            }
            segments.push(if marker == b':' {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            });
        } else {
            let decoded =
                decode_segment(raw, *at).map_err(|bad| Error::InvalidPattern { at: bad })?;
            segments.push(Segment::Static(decoded));
        }
    }

    Ok((segments, trailing))
}

fn path_head(path: &str) -> &str {
    let end = path
        .char_indices()
        .find(|(_, ch)| *ch == '?' || *ch == '#')
        .map(|(i, _)| i)
        .unwrap_or(path.len());
    &path[..end]
}

fn parse_path(path: &str) -> Result<(Vec<String>, bool), Error> {
    let head = path_head(path);
    if !head.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if head == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = head.as_bytes();
    let mut segments = Vec::new();
    let mut start = 1;

    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            if i == start {
                return Err(Error::InvalidPath { at: start });
            }
            let decoded = decode_segment(&head[start..i], start)
                .map_err(|bad| Error::InvalidPath { at: bad })?;
            segments.push(decoded);
            start = i + 1;
        }
    }

    let trailing = start == bytes.len();
    if !trailing {
        let decoded = decode_segment(&head[start..], start)
            .map_err(|bad| Error::InvalidPath { at: bad })?;
        segments.push(decoded);
    }

    Ok((segments, trailing))
}

fn shape_conflicts(
    a: &Route,
    b_segments: &[Segment],
    b_trailing: bool,
    policy: SlashPolicy,
) -> bool {
    if a.segments.len() != b_segments.len() {
        return false;
    }
    if policy == SlashPolicy::Strict && a.trailing != b_trailing {
        return false;
    }
    a.segments
        .iter()
        .zip(b_segments)
        .all(|(left, right)| match (left, right) {
            (Segment::Static(a), Segment::Static(b)) => a == b,
            (Segment::Param(_), Segment::Param(_)) => true,
            (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
            _ => false,
        })
}

fn make_rank(segments: &[Segment]) -> Vec<u8> {
    let mut rank = Vec::new();
    let mut wildcard = false;
    for segment in segments {
        match segment {
            Segment::Static(_) => rank.push(2),
            Segment::Param(_) => rank.push(1),
            Segment::Wildcard(_) => {
                rank.push(0);
                wildcard = true;
                break;
            }
        }
    }
    if !wildcard {
        rank.push(3);
    }
    rank
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

        if let Some(route) = self.routes.iter().find(|route| {
            route.method == method
                && shape_conflicts(route, &segments, trailing, self.policy)
        }) {
            return Err(Error::Conflict { with: route.id });
        }

        let id = self.routes.len();
        self.routes.push(Route {
            id,
            name: name.to_owned(),
            method: method.to_owned(),
            rank: make_rank(&segments),
            segments,
            trailing,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }

        let (path_segments, path_trailing) = parse_path(path)?;
        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();

        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != path_trailing {
                continue;
            }

            let mut captures = Vec::new();
            let mut matched = true;
            let mut path_index = 0;

            for segment in &route.segments {
                match segment {
                    Segment::Static(value) => {
                        if path_segments.get(path_index) != Some(value) {
                            matched = false;
                            break;
                        }
                        path_index += 1;
                    }
                    Segment::Param(name) => {
                        let Some(value) = path_segments.get(path_index) else {
                            matched = false;
                            break;
                        };
                        captures.push((name.clone(), value.clone()));
                        path_index += 1;
                    }
                    Segment::Wildcard(name) => {
                        let value = path_segments[path_index..].join("/");
                        captures.push((name.clone(), value));
                        path_index = path_segments.len();
                        break;
                    }
                }
            }

            if matched && path_index == path_segments.len() {
                candidates.push((route, captures));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let head_has_candidate =
            method == "HEAD" && candidates.iter().any(|(route, _)| route.method == "HEAD");
        let effective_method = if method == "HEAD" && !head_has_candidate {
            "GET"
        } else {
            method
        };

        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(route, _)| route.method == effective_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed = BTreeSet::new();
            for (route, _) in &candidates {
                allowed.insert(route.method.clone());
            }
            if allowed.contains("GET") {
                allowed.insert("HEAD".to_owned());
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        let (winner, params) = eligible
            .into_iter()
            .max_by(|(a, _), (b, _)| a.rank.cmp(&b.rank).then_with(|| b.id.cmp(&a.id)))
            .expect("eligible candidates are nonempty");

        let redirect = if self.policy == SlashPolicy::Redirect
            && winner.trailing != path_trailing
            && !path_segments.is_empty()
        {
            let mut head = path_head(path).to_owned();
            if winner.trailing {
                head.push('/');
            } else {
                head.pop();
            }
            Some(head)
        } else {
            None
        };

        Ok(Match {
            route: winner.id,
            params: params.clone(),
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
            if supplied.contains_key(*key) {
                return Err(Error::DuplicateParam {
                    name: (*key).to_owned(),
                });
            }
            supplied.insert(*key, *value);
        }

        let capture_names: Vec<&str> = route
            .segments
            .iter()
            .filter_map(|segment| match segment {
                Segment::Param(name) | Segment::Wildcard(name) => Some(name.as_str()),
                Segment::Static(_) => None,
            })
            .collect();

        for key in &capture_names {
            if !supplied.contains_key(*key) {
                return Err(Error::MissingParam {
                    name: (*key).to_owned(),
                });
            }
        }

        if let Some(extra) = supplied
            .keys()
            .filter(|key| !capture_names.contains(key))
            .min_by(|a, b| a.as_bytes().cmp(b.as_bytes()))
        {
            return Err(Error::ExtraParam {
                name: (*extra).to_owned(),
            });
        }

        for segment in &route.segments {
            let name = match segment {
                Segment::Param(name) | Segment::Wildcard(name) => name,
                Segment::Static(_) => continue,
            };
            let value = supplied[name.as_str()];
            let valid_ascii = value.bytes().all(|b| (0x20..=0x7e).contains(&b));
            let valid = match segment {
                Segment::Param(_) => valid_ascii && !value.is_empty() && !value.contains('/'),
                Segment::Wildcard(_) => {
                    valid_ascii && (value.is_empty() || value.split('/').all(|part| !part.is_empty()))
                }
                Segment::Static(_) => true,
            };
            if !valid {
                return Err(Error::InvalidValue {
                    name: name.clone(),
                });
            }
        }

        let mut output = String::from("/");
        let mut emitted = false;

        for segment in &route.segments {
            match segment {
                Segment::Static(value) => {
                    if emitted {
                        output.push('/');
                    }
                    encode_component(value, &mut output);
                    emitted = true;
                }
                Segment::Param(name) => {
                    if emitted {
                        output.push('/');
                    }
                    encode_component(supplied[name.as_str()], &mut output);
                    emitted = true;
                }
                Segment::Wildcard(name) => {
                    let value = supplied[name.as_str()];
                    if !value.is_empty() {
                        if emitted {
                            output.push('/');
                        }
                        for (index, part) in value.split('/').enumerate() {
                            if index > 0 {
                                output.push('/');
                            }
                            encode_component(part, &mut output);
                        }
                        emitted = true;
                    }
                }
            }
        }

        if !emitted {
            return Ok("/".to_owned());
        }
        if route.trailing {
            output.push('/');
        }
        Ok(output)
    }
}

fn encode_component(value: &str, output: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if is_unreserved(byte) {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
}