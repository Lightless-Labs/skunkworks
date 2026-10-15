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
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
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

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_static_segment(raw: &str, offset: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(offset + i);
            }
            let hi = hex_value(bytes[i + 1]).ok_or(offset + i)?;
            let lo = hex_value(bytes[i + 2]).ok_or(offset + i)?;
            let value = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(offset + i);
            }
            decoded.push(value);
            i += 3;
        } else {
            if !is_unreserved(b) {
                return Err(offset + i);
            }
            decoded.push(b);
            i += 1;
        }
    }
    Ok(String::from_utf8(decoded).unwrap())
}

fn parse_path(
    input: &str,
    pattern: bool,
) -> Result<(Vec<Segment>, bool), Error> {
    let invalid = |at| {
        if pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };

    if !input.starts_with('/') {
        return Err(invalid(0));
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = input.as_bytes();
    let trailing = input.ends_with('/');
    let end = if trailing { input.len() - 1 } else { input.len() };
    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    while start < end {
        let stop = input[start..end]
            .find('/')
            .map(|n| start + n)
            .unwrap_or(end);

        if stop == start {
            return Err(invalid(start));
        }

        let raw = &input[start..stop];

        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_capture_name(name) {
                return Err(invalid(start));
            }
            if !names.insert(name.to_string()) {
                return Err(invalid(start));
            }
            if marker == b'*' && stop != end {
                return Err(invalid(start));
            }
            if marker == b':' {
                segments.push(Segment::Param(name.to_string()));
            } else {
                segments.push(Segment::Wildcard(name.to_string()));
            }
        } else {
            let text = decode_static_segment(raw, start).map_err(invalid)?;
            segments.push(Segment::Static(text));
        }

        if stop == end {
            break;
        }
        start = stop + 1;
        if start == end {
            break;
        }
    }

    // The trailing slash is permitted only as the single final slash.
    if trailing && bytes.len() >= 2 && bytes[bytes.len() - 2] == b'/' {
        return Err(invalid(input.len() - 1));
    }

    Ok((segments, trailing))
}

fn encode_segment(value: &str) -> String {
    let mut result = String::new();
    for b in value.bytes() {
        if is_unreserved(b) {
            result.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            result.push('%');
            result.push(HEX[(b >> 4) as usize] as char);
            result.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    result
}

fn route_rank(route: &Route) -> Vec<u8> {
    let mut rank = Vec::new();
    for segment in &route.segments {
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

fn structural_conflict(a: &Route, b: &Route, policy: SlashPolicy) -> bool {
    if a.method != b.method || a.segments.len() != b.segments.len() {
        return false;
    }
    if policy == SlashPolicy::Strict && a.trailing != b.trailing {
        return false;
    }
    a.segments.iter().zip(&b.segments).all(|(x, y)| match (x, y) {
        (Segment::Static(a), Segment::Static(b)) => a == b,
        (Segment::Param(_), Segment::Param(_)) => true,
        (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
        _ => false,
    })
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self {
            policy,
            routes: Vec::new(),
        }
    }

    pub fn add(
        &mut self,
        name: &str,
        method: &str,
        pattern: &str,
    ) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        if !valid_route_name(name) {
            return Err(Error::InvalidName);
        }

        let (segments, trailing) = parse_path(pattern, true)?;
        if self.routes.iter().any(|route| route.name == name) {
            return Err(Error::DuplicateName);
        }

        let candidate = Route {
            id: self.routes.len(),
            name: name.to_string(),
            method: method.to_string(),
            segments,
            trailing,
        };

        if let Some(existing) = self
            .routes
            .iter()
            .find(|route| structural_conflict(route, &candidate, self.policy))
        {
            return Err(Error::Conflict { with: existing.id });
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
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (request_segments, request_trailing) = parse_path(head, false)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if let Some(params) = match_route(route, &request_segments) {
                let slash_ok = self.policy != SlashPolicy::Strict
                    || route.trailing == request_trailing;
                if slash_ok {
                    candidates.push((route, params));
                }
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible_method = if method == "HEAD"
            && candidates.iter().any(|(route, _)| route.method == "HEAD")
        {
            "HEAD"
        } else if method == "HEAD" {
            "GET"
        } else {
            method
        };

        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(route, _)| route.method == eligible_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed = std::collections::BTreeSet::new();
            for (route, _) in &candidates {
                allowed.insert(route.method.clone());
                if route.method == "GET" {
                    allowed.insert("HEAD".to_string());
                }
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        let (winner, params) = eligible
            .into_iter()
            .max_by(|(a, _), (b, _)| {
                route_rank(a)
                    .cmp(&route_rank(b))
                    .then_with(|| b.id.cmp(&a.id))
            })
            .unwrap();

        let redirect = if self.policy == SlashPolicy::Redirect
            && winner.segments.len() > 0
            && winner.trailing != request_trailing
        {
            let mut value = head.to_string();
            if winner.trailing {
                value.push('/');
            } else if value.ends_with('/') {
                value.pop();
            }
            Some(value)
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

        let mut supplied = std::collections::HashMap::new();
        for (key, value) in params {
            if supplied.contains_key(*key) {
                return Err(Error::DuplicateParam {
                    name: (*key).to_string(),
                });
            }
            supplied.insert(*key, *value);
        }

        let mut required = Vec::new();
        for segment in &route.segments {
            match segment {
                Segment::Param(name) | Segment::Wildcard(name) => required.push(name.as_str()),
                Segment::Static(_) => {}
            }
        }

        for key in &required {
            if !supplied.contains_key(*key) {
                return Err(Error::MissingParam {
                    name: (*key).to_string(),
                });
            }
        }

        let mut extras: Vec<&str> = supplied
            .keys()
            .copied()
            .filter(|key| !required.contains(key))
            .collect();
        extras.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam {
                name: (*key).to_string(),
            });
        }

        for segment in &route.segments {
            let (key, wildcard) = match segment {
                Segment::Param(name) => (Some(name.as_str()), false),
                Segment::Wildcard(name) => (Some(name.as_str()), true),
                Segment::Static(_) => (None, false),
            };
            if let Some(key) = key {
                let value = supplied[key];
                let bytes = value.as_bytes();
                let valid_ascii = bytes.iter().all(|b| (0x20..=0x7e).contains(b));
                let valid_value = if wildcard {
                    valid_ascii
                        && (value.is_empty()
                            || bytes[0] != b'/'
                                && bytes[bytes.len() - 1] != b'/'
                                && !value.contains("//"))
                } else {
                    valid_ascii && !value.is_empty() && !value.contains('/')
                };
                if !valid_value {
                    return Err(Error::InvalidValue {
                        name: key.to_string(),
                    });
                }
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
                    output.push_str(&encode_segment(value));
                    emitted += 1;
                }
                Segment::Param(name) => {
                    if emitted > 0 {
                        output.push('/');
                    }
                    output.push_str(&encode_segment(supplied[name.as_str()]));
                    emitted += 1;
                }
                Segment::Wildcard(name) => {
                    let value = supplied[name.as_str()];
                    if !value.is_empty() {
                        if emitted > 0 {
                            output.push('/');
                        }
                        for (index, component) in value.split('/').enumerate() {
                            if index > 0 {
                                output.push('/');
                            }
                            output.push_str(&encode_segment(component));
                        }
                    }
                }
            }
        }

        if output != "/" && route.trailing {
            output.push('/');
        }
        Ok(output)
    }
}

fn match_route(
    route: &Route,
    request: &[Segment],
) -> Option<Vec<(String, String)>> {
    let wildcard_index = route
        .segments
        .iter()
        .position(|segment| matches!(segment, Segment::Wildcard(_)));

    let fixed_len = wildcard_index.unwrap_or(route.segments.len());
    if wildcard_index.is_none() && request.len() != fixed_len {
        return None;
    }
    if wildcard_index.is_some() && request.len() < fixed_len {
        return None;
    }

    let mut captures = Vec::new();
    for index in 0..fixed_len {
        match (&route.segments[index], &request[index]) {
            (Segment::Static(expected), Segment::Static(actual)) if expected == actual => {}
            (Segment::Param(name), Segment::Static(actual)) => {
                captures.push((name.clone(), actual.clone()));
            }
            _ => return None,
        }
    }

    if let Some(index) = wildcard_index {
        let name = match &route.segments[index] {
            Segment::Wildcard(name) => name.clone(),
            _ => unreachable!(),
        };
        let tail = request[index..]
            .iter()
            .map(|segment| match segment {
                Segment::Static(value) => Some(value.as_str()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?
            .join("/");
        captures.push((name, tail));
    }

    Some(captures)
}

fn main() {}