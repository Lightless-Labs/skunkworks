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

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_route_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn valid_capture_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
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

fn parse_encoded_segment(s: &str, base: usize, pattern: bool) -> Result<String, Error> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(if pattern { Error::InvalidPattern { at: base + i } } else { Error::InvalidPath { at: base + i } });
            }
            let (Some(h), Some(l)) = (hex_upper(bytes[i + 1]), hex_upper(bytes[i + 2])) else {
                return Err(if pattern { Error::InvalidPattern { at: base + i } } else { Error::InvalidPath { at: base + i } });
            };
            let decoded = h * 16 + l;
            if !(0x20..=0x7e).contains(&decoded) || decoded == b'/' || is_unreserved(decoded) {
                return Err(if pattern { Error::InvalidPattern { at: base + i } } else { Error::InvalidPath { at: base + i } });
            }
            out.push(decoded);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(if pattern { Error::InvalidPattern { at: base + i } } else { Error::InvalidPath { at: base + i } });
        }
    }
    Ok(String::from_utf8(out).unwrap())
}

fn split_segments<'a>(input: &'a str, pattern: bool) -> Result<(Vec<(&'a str, usize)>, bool), Error> {
    let invalid = |at| if pattern { Error::InvalidPattern { at } } else { Error::InvalidPath { at } };
    if !input.starts_with('/') {
        return Err(invalid(0));
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }
    let mut parts = Vec::new();
    let mut start = 1;
    let bytes = input.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i] == b'/' {
            if i == start {
                return Err(invalid(i));
            }
            parts.push((&input[start..i], start));
            start = i + 1;
        }
    }
    let trailing = bytes.last() == Some(&b'/');
    if trailing {
        if start == bytes.len() {
            if parts.is_empty() {
                return Err(invalid(1));
            }
        } else {
            parts.push((&input[start..], start));
        }
    } else {
        if start == bytes.len() {
            return Err(invalid(start));
        }
        parts.push((&input[start..], start));
    }
    Ok((parts, trailing))
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool, Vec<u8>), Error> {
    let (parts, trailing) = split_segments(pattern, true)?;
    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    for (idx, (raw, at)) in parts.iter().enumerate() {
        if raw.starts_with(':') || raw.starts_with('*') {
            let wildcard = raw.as_bytes()[0] == b'*';
            let name = &raw[1..];
            if !valid_capture_name(name) {
                return Err(Error::InvalidPattern { at: *at });
            }
            if !names.insert(name.to_string()) {
                return Err(Error::InvalidPattern { at: *at });
            }
            if wildcard && idx + 1 != parts.len() {
                return Err(Error::InvalidPattern { at: *at });
            }
            segments.push(if wildcard {
                Segment::Wildcard(name.to_string())
            } else {
                Segment::Param(name.to_string())
            });
        } else {
            segments.push(Segment::Static(parse_encoded_segment(raw, *at, true)?));
        }
    }
    let mut rank = Vec::new();
    for segment in &segments {
        match segment {
            Segment::Static(_) => rank.push(2),
            Segment::Param(_) => rank.push(1),
            Segment::Wildcard(_) => {
                rank.push(0);
                break;
            }
        }
    }
    if !segments.iter().any(|s| matches!(s, Segment::Wildcard(_))) {
        rank.push(3);
    }
    Ok((segments, trailing, rank))
}

fn same_shape(a: &Route, b: &[Segment], trailing: bool, policy: SlashPolicy) -> bool {
    if a.segments.len() != b.len() {
        return false;
    }
    if policy == SlashPolicy::Strict && a.trailing != trailing {
        return false;
    }
    a.segments.iter().zip(b).all(|(x, y)| match (x, y) {
        (Segment::Static(a), Segment::Static(b)) => a == b,
        (Segment::Param(_), Segment::Param(_)) => true,
        (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
        _ => false,
    })
}

fn decode_request(path: &str) -> Result<(Vec<String>, bool), Error> {
    let head_end = path
        .char_indices()
        .find(|(_, c)| *c == '?' || *c == '#')
        .map(|(i, _)| i)
        .unwrap_or(path.len());
    let head = &path[..head_end];
    let (parts, trailing) = split_segments(head, false)?;
    let decoded = parts
        .iter()
        .map(|(s, at)| parse_encoded_segment(s, *at, false))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((decoded, trailing))
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self { policy, routes: Vec::new() }
    }

    pub fn add(&mut self, name: &str, method: &str, pattern: &str) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        if !valid_route_name(name) {
            return Err(Error::InvalidName);
        }
        let (segments, trailing, rank) = parse_pattern(pattern)?;
        if self.routes.iter().any(|r| r.name == name) {
            return Err(Error::DuplicateName);
        }
        if let Some(route) = self.routes.iter().filter(|r| r.method == method).find(|r| same_shape(r, &segments, trailing, self.policy)) {
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
        let head_end = path
            .char_indices()
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (parts, trailing) = split_segments(head, false)?;
        let decoded = parts
            .iter()
            .map(|(s, at)| parse_encoded_segment(s, *at, false))
            .collect::<Result<Vec<_>, _>>()?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            let mut params = Vec::new();
            let mut matched = true;
            let mut pos = 0;
            for segment in &route.segments {
                match segment {
                    Segment::Static(value) => {
                        if decoded.get(pos) != Some(value) {
                            matched = false;
                            break;
                        }
                        pos += 1;
                    }
                    Segment::Param(name) => {
                        if let Some(value) = decoded.get(pos) {
                            params.push((name.clone(), value.clone()));
                            pos += 1;
                        } else {
                            matched = false;
                            break;
                        }
                    }
                    Segment::Wildcard(name) => {
                        let value = decoded[pos..].join("/");
                        params.push((name.clone(), value));
                        pos = decoded.len();
                    }
                }
            }
            if matched && pos == decoded.len()
                && (self.policy != SlashPolicy::Strict || route.trailing == trailing)
            {
                candidates.push((route, params));
            }
        }
        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let has_head = candidates.iter().any(|(r, _)| r.method == "HEAD");
        let chosen_method = if method == "HEAD" && !has_head { "GET" } else { method };
        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(r, _)| r.method == chosen_method)
            .collect();
        if eligible.is_empty() {
            let mut allowed = std::collections::BTreeSet::new();
            for (route, _) in &candidates {
                allowed.insert(route.method.clone());
                if route.method == "GET" {
                    allowed.insert("HEAD".to_string());
                }
            }
            return Err(Error::MethodNotAllowed { allowed: allowed.into_iter().collect() });
        }

        let mut best = eligible[0];
        for candidate in eligible.iter().skip(1) {
            if candidate.0.rank > best.0.rank {
                best = candidate;
            }
        }
        let redirect = if self.policy == SlashPolicy::Redirect
            && !head.is_empty()
            && head != "/"
            && best.0.trailing != trailing
        {
            let mut target = head.to_string();
            if best.0.trailing {
                target.push('/');
            } else {
                target.pop();
            }
            Some(target)
        } else {
            None
        };
        Ok(Match {
            route: best.0.id,
            params: best.1.clone(),
            redirect,
        })
    }

    pub fn build(&self, name: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let route = self.routes.iter().find(|r| r.name == name).ok_or(Error::UnknownName)?;
        let mut supplied = std::collections::HashMap::new();
        for (key, value) in params {
            if supplied.contains_key(*key) {
                return Err(Error::DuplicateParam { name: (*key).to_string() });
            }
            supplied.insert(*key, *value);
        }

        let mut required = Vec::new();
        for segment in &route.segments {
            match segment {
                Segment::Param(n) | Segment::Wildcard(n) => required.push(n.as_str()),
                Segment::Static(_) => {}
            }
        }
        for key in &required {
            if !supplied.contains_key(key) {
                return Err(Error::MissingParam { name: (*key).to_string() });
            }
        }
        let required_set: std::collections::HashSet<&str> = required.iter().copied().collect();
        let mut extras: Vec<&str> = supplied.keys().copied().filter(|k| !required_set.contains(k)).collect();
        extras.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam { name: (*key).to_string() });
        }

        for segment in &route.segments {
            match segment {
                Segment::Param(n) => {
                    let value = supplied[n.as_str()];
                    if value.is_empty() || !value.bytes().all(|b| (0x20..=0x7e).contains(&b) && b != b'/') {
                        return Err(Error::InvalidValue { name: n.clone() });
                    }
                }
                Segment::Wildcard(n) => {
                    let value = supplied[n.as_str()];
                    if !value.bytes().all(|b| (0x20..=0x7e).contains(&b))
                        || (!value.is_empty() && value.split('/').any(|part| part.is_empty()))
                    {
                        return Err(Error::InvalidValue { name: n.clone() });
                    }
                }
                Segment::Static(_) => {}
            }
        }

        fn encode_component(value: &str) -> String {
            let mut out = String::new();
            for b in value.bytes() {
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

        let mut output = String::from("/");
        let mut first = true;
        for segment in &route.segments {
            let emitted = match segment {
                Segment::Static(value) => Some(encode_component(value)),
                Segment::Param(n) => Some(encode_component(supplied[n.as_str()])),
                Segment::Wildcard(n) => {
                    let value = supplied[n.as_str()];
                    if value.is_empty() {
                        None
                    } else {
                        Some(value.split('/').map(encode_component).collect::<Vec<_>>().join("/"))
                    }
                }
            };
            if let Some(value) = emitted {
                if !first {
                    output.push('/');
                }
                output.push_str(&value);
                first = false;
            }
        }
        if first {
            output = "/".to_string();
        } else if route.trailing {
            output.push('/');
        }
        Ok(output)
    }
}