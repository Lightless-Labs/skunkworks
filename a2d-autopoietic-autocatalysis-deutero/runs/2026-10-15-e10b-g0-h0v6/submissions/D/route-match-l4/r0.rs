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

fn valid_method(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn valid_route_name(s: &str) -> bool {
    let mut it = s.bytes();
    matches!(it.next(), Some(b'a'..=b'z'))
        && it.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn valid_capture_name(s: &str) -> bool {
    let mut it = s.bytes();
    matches!(it.next(), Some(b'a'..=b'z'))
        && it.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn hex_upper(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_static(bytes: &[u8], offset: usize) -> Result<String, usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(offset + i);
            }
            let hi = hex_upper(bytes[i + 1]).ok_or(offset + i)?;
            let lo = hex_upper(bytes[i + 2]).ok_or(offset + i)?;
            let decoded = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || is_unreserved(decoded)
            {
                return Err(offset + i);
            }
            out.push(decoded);
            i += 3;
        } else if is_unreserved(b) {
            out.push(b);
            i += 1;
        } else {
            return Err(offset + i);
        }
    }
    Ok(String::from_utf8(out).unwrap())
}

fn parse_path(
    input: &str,
    pattern: bool,
    base: usize,
) -> Result<(Vec<Segment>, bool), usize> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(base);
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let trailing = bytes.last() == Some(&b'/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = HashSet::new();
    let mut start = 1;

    while start < end {
        let rel_end = bytes[start..end]
            .iter()
            .position(|&b| b == b'/')
            .map(|n| start + n)
            .unwrap_or(end);

        if rel_end == start {
            return Err(base + start);
        }

        let raw = &bytes[start..rel_end];
        if pattern && (raw[0] == b':' || raw[0] == b'*') {
            let is_wild = raw[0] == b'*';
            let name = std::str::from_utf8(&raw[1..]).unwrap_or("");
            if !valid_capture_name(name) || !names.insert(name.to_string()) {
                return Err(base + start);
            }
            if is_wild && rel_end != end {
                return Err(base + start);
            }
            segments.push(if is_wild {
                Segment::Wildcard(name.to_string())
            } else {
                Segment::Param(name.to_string())
            });
        } else {
            let decoded = decode_static(raw, base + start)?;
            segments.push(Segment::Static(decoded));
        }

        if rel_end == end {
            break;
        }
        start = rel_end + 1;
        if start == end {
            return Err(base + start);
        }
    }

    Ok((segments, trailing))
}

fn shape_conflicts(a: &Route, b: &Route) -> bool {
    if a.segments.len() != b.segments.len() {
        return false;
    }
    a.segments.iter().zip(&b.segments).all(|(x, y)| match (x, y) {
        (Segment::Static(a), Segment::Static(b)) => a == b,
        (Segment::Param(_), Segment::Param(_)) => true,
        (Segment::Wildcard(_), Segment::Wildcard(_)) => true,
        _ => false,
    })
}

fn rank(route: &Route) -> Vec<u8> {
    let mut out = Vec::new();
    for segment in &route.segments {
        match segment {
            Segment::Static(_) => out.push(2),
            Segment::Param(_) => out.push(1),
            Segment::Wildcard(_) => {
                out.push(0);
                return out;
            }
        }
    }
    out.push(3);
    out
}

fn encode_segment(s: &str) -> String {
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
        let (segments, trailing) =
            parse_path(pattern, true, 0).map_err(|at| Error::InvalidPattern { at })?;

        if self.routes.iter().any(|r| r.name == name) {
            return Err(Error::DuplicateName);
        }

        let id = self.routes.len();
        let candidate = Route {
            id,
            name: name.to_string(),
            method: method.to_string(),
            segments,
            trailing,
        };

        if let Some(existing) = self.routes.iter().find(|r| {
            r.method == candidate.method
                && shape_conflicts(r, &candidate)
                && (self.policy != SlashPolicy::Strict || r.trailing == candidate.trailing)
        }) {
            return Err(Error::Conflict { with: existing.id });
        }

        self.routes.push(candidate);
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }

        let head_end = path
            .bytes()
            .position(|b| b == b'?' || b == b'#')
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (request_segments, request_trailing) =
            parse_path(head, false, 0).map_err(|at| Error::InvalidPath { at })?;

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();
        for route in &self.routes {
            let mut captures = Vec::new();
            let mut matched = true;
            let mut i = 0;

            for segment in &route.segments {
                match segment {
                    Segment::Static(s) => {
                        if request_segments.get(i).map(|x| match x {
                            Segment::Static(v) => v == s,
                            _ => false,
                        }) != Some(true)
                        {
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
                        let tail = request_segments[i..]
                            .iter()
                            .map(|s| match s {
                                Segment::Static(v) => v.as_str(),
                                _ => "",
                            })
                            .collect::<Vec<_>>()
                            .join("/");
                        captures.push((name.clone(), tail));
                        i = request_segments.len();
                    }
                }
            }

            if matched && i == request_segments.len() {
                if self.policy == SlashPolicy::Strict && route.trailing != request_trailing {
                    continue;
                }
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

        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(r, _)| r.method == eligible_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed = Vec::<String>::new();
            for (route, _) in &candidates {
                if !allowed.contains(&route.method) {
                    allowed.push(route.method.clone());
                }
                if route.method == "GET" && !allowed.iter().any(|m| m == "HEAD") {
                    allowed.push("HEAD".to_string());
                }
            }
            allowed.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let (winner, captures) = eligible
            .into_iter()
            .max_by(|(a, _), (b, _)| rank(a).cmp(&rank(b)).then_with(|| b.id.cmp(&a.id)))
            .unwrap();

        let redirect = if self.policy == SlashPolicy::Redirect
            && winner.trailing != request_trailing
            && !head.is_empty()
            && head != "/"
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
            params: captures.clone(),
            redirect,
        })
    }

    pub fn build(&self, name: &str, params: &[(&str, &str)]) -> Result<String, Error> {
        let route = self.routes.iter().find(|r| r.name == name).ok_or(Error::UnknownName)?;

        let mut seen = HashSet::new();
        for (key, _) in params {
            if !seen.insert(*key) {
                return Err(Error::DuplicateParam {
                    name: (*key).to_string(),
                });
            }
        }

        let captures: Vec<&str> = route
            .segments
            .iter()
            .filter_map(|s| match s {
                Segment::Param(n) | Segment::Wildcard(n) => Some(n.as_str()),
                _ => None,
            })
            .collect();

        for key in &captures {
            if !seen.contains(key) {
                return Err(Error::MissingParam {
                    name: (*key).to_string(),
                });
            }
        }

        let mut extras: Vec<&str> = params
            .iter()
            .map(|(k, _)| *k)
            .filter(|k| !captures.contains(k))
            .collect();
        extras.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        if let Some(key) = extras.first() {
            return Err(Error::ExtraParam {
                name: (*key).to_string(),
            });
        }

        for segment in &route.segments {
            if let Segment::Param(key) | Segment::Wildcard(key) = segment {
                let value = params.iter().find(|(k, _)| *k == key).unwrap().1;
                let valid_ascii = value.bytes().all(|b| (0x20..=0x7e).contains(&b));
                let valid = match segment {
                    Segment::Param(_) => {
                        !value.is_empty() && !value.contains('/') && valid_ascii
                    }
                    Segment::Wildcard(_) => {
                        valid_ascii
                            && (value.is_empty()
                                || value
                                    .split('/')
                                    .all(|part| !part.is_empty()))
                    }
                    _ => true,
                };
                if !valid {
                    return Err(Error::InvalidValue { name: key.clone() });
                }
            }
        }

        let mut out = String::from("/");
        let mut first = true;
        for segment in &route.segments {
            if !first {
                out.push('/');
            }
            first = false;
            match segment {
                Segment::Static(s) => out.push_str(&encode_segment(s)),
                Segment::Param(key) => {
                    let value = params.iter().find(|(k, _)| *k == key).unwrap().1;
                    out.push_str(&encode_segment(value));
                }
                Segment::Wildcard(key) => {
                    let value = params.iter().find(|(k, _)| *k == key).unwrap().1;
                    if !value.is_empty() {
                        out.push_str(
                            &value
                                .split('/')
                                .map(encode_segment)
                                .collect::<Vec<_>>()
                                .join("/"),
                        );
                    }
                }
            }
        }

        if route.trailing && out != "/" {
            out.push('/');
        }
        Ok(out)
    }
}