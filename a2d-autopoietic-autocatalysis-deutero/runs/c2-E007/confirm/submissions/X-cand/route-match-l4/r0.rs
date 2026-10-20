use std::collections::{BTreeSet, HashMap};

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

#[derive(Clone, PartialEq, Eq)]
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

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
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

fn decode_segment(raw: &[u8], base: usize) -> Result<String, usize> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        if b == b'%' {
            if i + 2 >= raw.len() {
                return Err(base + i);
            }
            let hi = hex_upper(raw[i + 1]).ok_or(base + i)?;
            let lo = hex_upper(raw[i + 2]).ok_or(base + i)?;
            let decoded = (hi << 4) | lo;
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
    Ok(String::from_utf8(out).expect("decoded segments are ASCII"))
}

fn valid_capture_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn valid_route_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..].iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-')
        })
}

fn valid_method(s: &str) -> bool {
    !s.is_empty() && s.as_bytes().iter().all(u8::is_ascii_uppercase)
}

fn parse_pattern(input: &str) -> Result<(Vec<Segment>, bool, Vec<u8>), Error> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if input == "/" {
        return Ok((Vec::new(), false, vec![3]));
    }

    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = BTreeSet::new();
    let mut rank = Vec::new();
    let mut start = 1;

    while start < end {
        let stop = bytes[start..end]
            .iter()
            .position(|b| *b == b'/')
            .map(|n| start + n)
            .unwrap_or(end);

        if stop == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &bytes[start..stop];
        if raw[0] == b':' || raw[0] == b'*' {
            let wildcard = raw[0] == b'*';
            let name_bytes = &raw[1..];
            let name = std::str::from_utf8(name_bytes).unwrap_or("");
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if wildcard && stop != end {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(if wildcard {
                Segment::Wildcard(name.to_owned())
            } else {
                Segment::Param(name.to_owned())
            });
            rank.push(if wildcard { 0 } else { 1 });
        } else {
            let value = decode_segment(raw, start)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(value));
            rank.push(2);
        }

        if stop == end {
            break;
        }
        start = stop + 1;
    }

    // A trailing slash is permitted only after a nonempty final segment.
    // For example, `//` and `/a//` contain an empty segment before it.
    if trailing && start == end {
        return Err(Error::InvalidPattern { at: end });
    }

    if !matches!(segments.last(), Some(Segment::Wildcard(_))) {
        rank.push(3);
    }
    Ok((segments, trailing, rank))
}

fn parse_path(input: &str) -> Result<(Vec<String>, bool), Error> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut start = 1;

    while start < end {
        let stop = bytes[start..end]
            .iter()
            .position(|b| *b == b'/')
            .map(|n| start + n)
            .unwrap_or(end);

        if stop == start {
            return Err(Error::InvalidPath { at: start });
        }
        let value = decode_segment(&bytes[start..stop], start)
            .map_err(|at| Error::InvalidPath { at })?;
        segments.push(value);

        if stop == end {
            break;
        }
        start = stop + 1;
    }

    // A trailing slash is permitted only after a nonempty final segment.
    if trailing && start == end {
        return Err(Error::InvalidPath { at: end });
    }

    Ok((segments, trailing))
}

fn shape_conflicts(a: &[Segment], b: &[Segment]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| match (x, y) {
            (Segment::Static(s), Segment::Static(t)) => s == t,
            (Segment::Param(_), Segment::Param(_))
            | (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
            _ => false,
        })
}

fn encode_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
        }
    }
    out
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

        if let Some(route) = self.routes.iter().find(|route| {
            route.method == method
                && shape_conflicts(&route.segments, &segments)
                && (self.policy != SlashPolicy::Strict || route.trailing == trailing)
        }) {
            return Err(Error::Conflict { with: route.id });
        }

        let id = self.routes.len();
        self.routes.push(Route {
            id,
            name: name.to_owned(),
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
        let (request_segments, request_trailing) = parse_path(head)?;

        let path_candidates: Vec<&Route> = self
            .routes
            .iter()
            .filter(|route| {
                let slash_ok = self.policy != SlashPolicy::Strict
                    || route.trailing == request_trailing;
                slash_ok && route_matches(route, &request_segments)
            })
            .collect();

        if path_candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<&Route> = if method == "HEAD" {
            let head_candidates: Vec<&Route> = path_candidates
                .iter()
                .copied()
                .filter(|r| r.method == "HEAD")
                .collect();
            if !head_candidates.is_empty() {
                head_candidates
            } else {
                path_candidates
                    .iter()
                    .copied()
                    .filter(|r| r.method == "GET")
                    .collect()
            }
        } else {
            path_candidates
                .iter()
                .copied()
                .filter(|r| r.method == method)
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed = BTreeSet::new();
            for route in &path_candidates {
                allowed.insert(route.method.clone());
                if route.method == "GET" {
                    allowed.insert("HEAD".to_owned());
                }
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        let mut best = eligible[0];
        for route in eligible.iter().skip(1) {
            if route.rank > best.rank {
                best = route;
            }
        }

        let params = capture_params(best, &request_segments);
        let redirect = if self.policy == SlashPolicy::Redirect
            && !request_segments.is_empty()
            && best.trailing != request_trailing
        {
            let mut target = head.to_owned();
            if best.trailing {
                target.push('/');
            } else {
                target.pop();
            }
            Some(target)
        } else {
            None
        };

        Ok(Match {
            route: best.id,
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
            if supplied.insert(*key, *value).is_some() {
                return Err(Error::DuplicateParam {
                    name: (*key).to_owned(),
                });
            }
        }

        let mut required = Vec::new();
        for segment in &route.segments {
            match segment {
                Segment::Param(name) | Segment::Wildcard(name) => required.push(name.as_str()),
                Segment::Static(_) => {}
            }
        }
        for key in &required {
            if !supplied.contains_key(key) {
                return Err(Error::MissingParam {
                    name: (*key).to_owned(),
                });
            }
        }

        let required_set: BTreeSet<&str> = required.iter().copied().collect();
        if let Some(key) = supplied
            .keys()
            .copied()
            .filter(|key| !required_set.contains(key))
            .min()
        {
            return Err(Error::ExtraParam {
                name: key.to_owned(),
            });
        }

        for segment in &route.segments {
            let (name, wildcard) = match segment {
                Segment::Param(name) => (name, false),
                Segment::Wildcard(name) => (name, true),
                Segment::Static(_) => continue,
            };
            let value = supplied[name.as_str()];
            let bytes = value.as_bytes();
            let invalid = !bytes.iter().all(|b| (0x20..=0x7e).contains(b))
                || if wildcard {
                    !value.is_empty()
                        && (value.starts_with('/')
                            || value.ends_with('/')
                            || value.contains("//"))
                } else {
                    value.is_empty() || value.contains('/')
                };
            if invalid {
                return Err(Error::InvalidValue {
                    name: name.clone(),
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
                    output.push_str(&encode_component(value));
                    emitted += 1;
                }
                Segment::Param(name) => {
                    if emitted > 0 {
                        output.push('/');
                    }
                    output.push_str(&encode_component(supplied[name.as_str()]));
                    emitted += 1;
                }
                Segment::Wildcard(name) => {
                    let value = supplied[name.as_str()];
                    if !value.is_empty() {
                        if emitted > 0 {
                            output.push('/');
                        }
                        for (i, component) in value.split('/').enumerate() {
                            if i > 0 {
                                output.push('/');
                            }
                            output.push_str(&encode_component(component));
                        }
                        emitted += value.split('/').count();
                    }
                }
            }
        }
        if emitted == 0 {
            output = "/".to_owned();
        } else if route.trailing {
            output.push('/');
        }
        Ok(output)
    }
}

fn route_matches(route: &Route, request: &[String]) -> bool {
    let wildcard = matches!(route.segments.last(), Some(Segment::Wildcard(_)));
    if (!wildcard && route.segments.len() != request.len())
        || (wildcard && request.len() + 1 < route.segments.len())
    {
        return false;
    }

    for (i, segment) in route.segments.iter().enumerate() {
        if let Segment::Wildcard(_) = segment {
            return true;
        }
        match segment {
            Segment::Static(value) if request.get(i) != Some(value) => return false,
            Segment::Param(_) => {}
            _ => {}
        }
    }
    route.segments.len() == request.len()
}

fn capture_params(route: &Route, request: &[String]) -> Vec<(String, String)> {
    let mut params = Vec::new();
    for (i, segment) in route.segments.iter().enumerate() {
        match segment {
            Segment::Param(name) => {
                if let Some(value) = request.get(i) {
                    params.push((name.clone(), value.clone()));
                }
            }
            Segment::Wildcard(name) => {
                let value = request[i..].join("/");
                params.push((name.clone(), value));
            }
            Segment::Static(_) => {}
        }
    }
    params
}