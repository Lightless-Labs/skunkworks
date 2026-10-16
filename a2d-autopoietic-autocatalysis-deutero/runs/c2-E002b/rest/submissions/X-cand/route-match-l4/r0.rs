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

fn valid_method(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_route_name(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn valid_capture_name(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn hex_upper(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_static(raw: &str, base: usize, pattern: bool) -> Result<String, Error> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            let fail = || {
                if pattern {
                    Error::InvalidPattern { at: base + i }
                } else {
                    Error::InvalidPath { at: base + i }
                }
            };
            if i + 2 >= bytes.len() {
                return Err(fail());
            }
            let hi = hex_upper(bytes[i + 1]).ok_or_else(fail)?;
            let lo = hex_upper(bytes[i + 2]).ok_or_else(fail)?;
            let decoded = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&decoded) || decoded == b'/' || unreserved(decoded) {
                return Err(fail());
            }
            out.push(decoded);
            i += 3;
        } else if unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(if pattern {
                Error::InvalidPattern { at: base + i }
            } else {
                Error::InvalidPath { at: base + i }
            });
        }
    }
    Ok(String::from_utf8(out).unwrap())
}

fn parse_path(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), Error> {
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
    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();
    let mut start = 1usize;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(invalid(start));
        }
        let raw = &input[start..stop];

        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let wildcard = raw.as_bytes()[0] == b'*';
            let name = &raw[1..];
            if !valid_capture_name(name) || !seen.insert(name.to_owned()) {
                return Err(invalid(start));
            }
            if wildcard && stop != end {
                return Err(invalid(start));
            }
            segments.push(if wildcard {
                Segment::Wildcard(name.to_owned())
            } else {
                Segment::Param(name.to_owned())
            });
        } else {
            segments.push(Segment::Static(decode_static(raw, start, pattern)?));
        }
        if stop == end {
            break;
        }
        start = stop + 1;
    }

    Ok((segments, trailing))
}

fn shape_conflicts(a: &Route, b: &Route, policy: SlashPolicy) -> bool {
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
        Self { policy, routes: Vec::new() }
    }

    pub fn add(&mut self, name: &str, method: &str, pattern: &str) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }
        if !valid_route_name(name) {
            return Err(Error::InvalidName);
        }
        let (segments, trailing) = parse_path(pattern, true)?;
        if self.routes.iter().any(|r| r.name == name) {
            return Err(Error::DuplicateName);
        }

        let mut rank = Vec::new();
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
        let route = Route {
            id,
            name: name.to_owned(),
            method: method.to_owned(),
            segments,
            trailing,
            rank,
        };
        if let Some(existing) = self.routes.iter().find(|r| shape_conflicts(&route, r, self.policy)) {
            return Err(Error::Conflict { with: existing.id });
        }
        self.routes.push(route);
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
            if self.policy == SlashPolicy::Strict && route.trailing != request_trailing {
                continue;
            }
            let mut captures = Vec::new();
            let mut matched = true;
            let mut i = 0usize;
            for segment in &route.segments {
                match segment {
                    Segment::Static(expected) => {
                        if request_segments.get(i).map(|s| match s {
                            Segment::Static(v) => v == expected,
                            _ => false,
                        }) != Some(true) {
                            matched = false;
                            break;
                        }
                        i += 1;
                    }
                    Segment::Param(name) => {
                        match request_segments.get(i) {
                            Some(Segment::Static(value)) => {
                                captures.push((name.clone(), value.clone()));
                                i += 1;
                            }
                            _ => {
                                matched = false;
                                break;
                            }
                        }
                    }
                    Segment::Wildcard(name) => {
                        let mut tail = Vec::new();
                        while i < request_segments.len() {
                            match &request_segments[i] {
                                Segment::Static(v) => tail.push(v.clone()),
                                _ => {
                                    matched = false;
                                    break;
                                }
                            }
                            i += 1;
                        }
                        if !matched {
                            break;
                        }
                        captures.push((name.clone(), tail.join("/")));
                    }
                }
            }
            if matched && i == request_segments.len() {
                candidates.push((route, captures));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible_method = if method == "HEAD" && candidates.iter().any(|(r, _)| r.method == "HEAD") {
            "HEAD"
        } else if method == "HEAD" {
            "GET"
        } else {
            method
        };
        let mut eligible: Vec<_> = candidates
            .iter()
            .filter(|(r, _)| r.method == eligible_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed = std::collections::BTreeSet::new();
            for (route, _) in &candidates {
                allowed.insert(route.method.clone());
            }
            if allowed.contains("GET") {
                allowed.insert("HEAD".to_owned());
            }
            return Err(Error::MethodNotAllowed { allowed: allowed.into_iter().collect() });
        }

        eligible.sort_by(|(a, _), (b, _)| b.rank.cmp(&a.rank).then_with(|| a.id.cmp(&b.id)));
        let (winner, params) = eligible[0];
        let redirect = if self.policy == SlashPolicy::Redirect
            && !head.is_empty()
            && head != "/"
            && winner.trailing != request_trailing
        {
            if winner.trailing {
                Some(format!("{head}/"))
            } else {
                Some(head.strip_suffix('/').unwrap_or(head).to_owned())
            }
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
        let route = self.routes.iter().find(|r| r.name == name).ok_or(Error::UnknownName)?;

        let mut supplied = std::collections::HashMap::<&str, &str>::new();
        for (key, value) in params {
            if supplied.insert(*key, *value).is_some() {
                return Err(Error::DuplicateParam { name: (*key).to_owned() });
            }
        }

        let mut capture_names = Vec::new();
        for segment in &route.segments {
            match segment {
                Segment::Param(n) | Segment::Wildcard(n) => capture_names.push(n.as_str()),
                Segment::Static(_) => {}
            }
        }
        for key in &capture_names {
            if !supplied.contains_key(key) {
                return Err(Error::MissingParam { name: (*key).to_owned() });
            }
        }
        let extra = supplied.keys().filter(|k| !capture_names.contains(k)).min();
        if let Some(key) = extra {
            return Err(Error::ExtraParam { name: (*key).to_owned() });
        }

        for segment in &route.segments {
            match segment {
                Segment::Param(n) => {
                    let value = supplied[n.as_str()];
                    if value.is_empty()
                        || !value.bytes().all(|b| (0x20..=0x7e).contains(&b))
                        || value.contains('/')
                    {
                        return Err(Error::InvalidValue { name: n.clone() });
                    }
                }
                Segment::Wildcard(n) => {
                    let value = supplied[n.as_str()];
                    if !value.bytes().all(|b| (0x20..=0x7e).contains(&b))
                        || (!value.is_empty()
                            && value.split('/').any(|part| part.is_empty()))
                    {
                        return Err(Error::InvalidValue { name: n.clone() });
                    }
                }
                Segment::Static(_) => {}
            }
        }

        let mut output = String::from("/");
        let mut first = true;
        for segment in &route.segments {
            let mut pieces: Vec<String> = Vec::new();
            match segment {
                Segment::Static(value) => pieces.push(encode_component(value)),
                Segment::Param(name) => pieces.push(encode_component(supplied[name.as_str()])),
                Segment::Wildcard(name) => {
                    let value = supplied[name.as_str()];
                    if !value.is_empty() {
                        pieces.extend(value.split('/').map(encode_component));
                    }
                }
            }
            for piece in pieces {
                if !first {
                    output.push('/');
                }
                output.push_str(&piece);
                first = false;
            }
        }
        if route.trailing && output != "/" {
            output.push('/');
        }
        Ok(output)
    }
}

fn encode_component(value: &str) -> String {
    let mut out = String::new();
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in value.bytes() {
        if unreserved(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}