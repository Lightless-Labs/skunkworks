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
}

pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn is_method(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_name(s: &str, allow_dash: bool) -> bool {
    let bytes = s.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || (allow_dash && *b == b'-'))
}

fn decode_static_segment(s: &str, base: usize) -> Result<String, usize> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let h1 = bytes[i + 1];
            let h2 = bytes[i + 2];
            if !(h1.is_ascii_digit() || (b'A'..=b'F').contains(&h1))
                || !(h2.is_ascii_digit() || (b'A'..=b'F').contains(&h2))
            {
                return Err(base + i);
            }
            let value = (hex(h1) << 4) | hex(h2);
            if !(0x20..=0x7e).contains(&value) || value == b'/' || is_unreserved(value) {
                return Err(base + i);
            }
            out.push(value);
            i += 3;
        } else {
            if !b.is_ascii() || !is_unreserved(b) {
                return Err(base + i);
            }
            out.push(b);
            i += 1;
        }
    }
    Ok(String::from_utf8(out).unwrap())
}

fn hex(b: u8) -> u8 {
    if b.is_ascii_digit() {
        b - b'0'
    } else {
        b - b'A' + 10
    }
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

    let trailing = input.ends_with('/');
    let end = if trailing { input.len() - 1 } else { input.len() };
    let bytes = input.as_bytes();
    let mut segments = Vec::new();
    let mut start = 1;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(invalid(start));
        }
        let text = &input[start..stop];
        if pattern && (text.starts_with(':') || text.starts_with('*')) {
            let wildcard = text.starts_with('*');
            let name = &text[1..];
            if !valid_name(name, false) {
                return Err(invalid(start));
            }
            if wildcard && stop < end {
                return Err(invalid(start));
            }
            segments.push(if wildcard {
                Segment::Wildcard(name.to_string())
            } else {
                Segment::Param(name.to_string())
            });
        } else {
            let decoded = decode_static_segment(text, start).map_err(invalid)?;
            segments.push(Segment::Static(decoded));
        }
        start = stop + 1;
    }

    if trailing && (end == 0 || bytes[end - 1] == b'/') {
        return Err(invalid(end));
    }
    Ok((segments, trailing))
}

fn rank(route: &Route) -> Vec<u8> {
    let mut v = Vec::new();
    for seg in &route.segments {
        match seg {
            Segment::Static(_) => v.push(2),
            Segment::Param(_) => v.push(1),
            Segment::Wildcard(_) => {
                v.push(0);
                return v;
            }
        }
    }
    v.push(3);
    v
}

fn matches(route: &Route, path: &[Segment]) -> Option<Vec<(String, String)>> {
    let wildcard = matches!(route.segments.last(), Some(Segment::Wildcard(_)));
    let fixed = if wildcard {
        route.segments.len() - 1
    } else {
        route.segments.len()
    };
    if (!wildcard && path.len() != fixed) || (wildcard && path.len() < fixed) {
        return None;
    }

    let mut params = Vec::new();
    for i in 0..fixed {
        match (&route.segments[i], &path[i]) {
            (Segment::Static(a), Segment::Static(b)) if a == b => {}
            (Segment::Param(name), Segment::Static(value)) => {
                params.push((name.clone(), value.clone()));
            }
            _ => return None,
        }
    }
    if wildcard {
        if let Segment::Wildcard(name) = &route.segments[fixed] {
            let tail = path[fixed..]
                .iter()
                .map(|s| match s {
                    Segment::Static(v) => v.as_str(),
                    _ => "",
                })
                .collect::<Vec<_>>()
                .join("/");
            params.push((name.clone(), tail));
        }
    }
    Some(params)
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
        if !is_method(method) {
            return Err(Error::InvalidMethod);
        }
        if !valid_name(name, true) {
            return Err(Error::InvalidName);
        }
        let (segments, trailing) = parse_path(pattern, true)?;
        if self.routes.iter().any(|r| r.name == name) {
            return Err(Error::DuplicateName);
        }

        for route in &self.routes {
            if route.method != method {
                continue;
            }
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            if route.segments.len() != segments.len() {
                continue;
            }
            let same = route.segments.iter().zip(&segments).all(|(a, b)| match (a, b) {
                (Segment::Static(x), Segment::Static(y)) => x == y,
                (Segment::Param(_), Segment::Param(_)) => true,
                (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
                _ => false,
            });
            if same {
                return Err(Error::Conflict { with: route.id });
            }
        }

        let id = self.routes.len();
        self.routes.push(Route {
            id,
            name: name.to_string(),
            method: method.to_string(),
            segments,
            trailing,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !is_method(method) {
            return Err(Error::InvalidMethod);
        }
        let head_end = path
            .char_indices()
            .find(|(_, c)| *c == '?' || *c == '#')
            .map(|(i, _)| i)
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (parsed, trailing) = parse_path(head, false)?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }
            if let Some(params) = matches(route, &parsed) {
                candidates.push((route, params));
            }
        }
        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<usize> = if method == "HEAD" {
            if candidates.iter().any(|(r, _)| r.method == "HEAD") {
                candidates.iter().enumerate().filter_map(|(i, (r, _))| (r.method == "HEAD").then_some(i)).collect()
            } else {
                candidates.iter().enumerate().filter_map(|(i, (r, _))| (r.method == "GET").then_some(i)).collect()
            }
        } else {
            candidates.iter().enumerate().filter_map(|(i, (r, _))| (r.method == method).then_some(i)).collect()
        };

        if eligible.is_empty() {
            let mut allowed: Vec<String> = candidates.iter().map(|(r, _)| r.method.clone()).collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_string());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let mut best = eligible[0];
        for &i in eligible.iter().skip(1) {
            if rank(candidates[i].0) > rank(candidates[best].0) {
                best = i;
            }
        }
        let (route, params) = &candidates[best];
        let redirect = if self.policy == SlashPolicy::Redirect
            && !parsed.is_empty()
            && route.trailing != trailing
        {
            let mut s = head.to_string();
            if route.trailing {
                s.push('/');
            } else if s.ends_with('/') {
                s.pop();
            }
            Some(s)
        } else {
            None
        };
        Ok(Match {
            route: route.id,
            params: params.clone(),
            redirect,
        })
    }

    pub fn build(&self, name: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let route = self.routes.iter().find(|r| r.name == name).ok_or(Error::UnknownName)?;

        let mut seen = Vec::<&str>::new();
        for (key, _) in params {
            if seen.contains(key) {
                return Err(Error::DuplicateParam {
                    name: (*key).to_string(),
                });
            }
            seen.push(key);
        }

        let mut required = Vec::new();
        for seg in &route.segments {
            match seg {
                Segment::Param(n) | Segment::Wildcard(n) => required.push(n.as_str()),
                Segment::Static(_) => {}
            }
        }
        for key in &required {
            if !params.iter().any(|(k, _)| k == key) {
                return Err(Error::MissingParam {
                    name: (*key).to_string(),
                });
            }
        }

        let mut extras: Vec<&str> = params
            .iter()
            .map(|(k, _)| *k)
            .filter(|k| !required.contains(k))
            .collect();
        extras.sort();
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam {
                name: (*key).to_string(),
            });
        }

        for key in &required {
            let value = params.iter().find(|(k, _)| k == key).unwrap().1;
            let wildcard = route
                .segments
                .iter()
                .any(|s| matches!(s, Segment::Wildcard(n) if n == key));
            let valid = value.bytes().all(|b| (0x20..=0x7e).contains(&b))
                && if wildcard {
                    value.is_empty()
                        || value
                            .split('/')
                            .all(|part| !part.is_empty())
                } else {
                    !value.is_empty() && !value.contains('/')
                };
            if !valid {
                return Err(Error::InvalidValue {
                    name: (*key).to_string(),
                });
            }
        }

        let mut out = String::new();
        for seg in &route.segments {
            match seg {
                Segment::Static(value) => {
                    out.push('/');
                    out.push_str(&encode_component(value));
                }
                Segment::Param(key) => {
                    let value = params.iter().find(|(k, _)| k == key).unwrap().1;
                    out.push('/');
                    out.push_str(&encode_component(value));
                }
                Segment::Wildcard(key) => {
                    let value = params.iter().find(|(k, _)| k == key).unwrap().1;
                    if !value.is_empty() {
                        out.push('/');
                        out.push_str(
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
        if out.is_empty() {
            out.push('/');
        } else if route.trailing {
            out.push('/');
        }
        Ok(out)
    }
}