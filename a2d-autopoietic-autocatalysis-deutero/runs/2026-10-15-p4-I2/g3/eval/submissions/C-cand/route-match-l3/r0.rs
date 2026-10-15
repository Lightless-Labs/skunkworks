#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidPattern { at: usize },
    InvalidPath { at: usize },
    NotFound,
    InvalidMethod,
    MethodNotAllowed { allowed: Vec<String> },
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
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
    rank: Vec<u8>,
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

    pub fn add(&mut self, method: &str, pattern: &str) -> Result<usize, Error> {
        validate_method(method)?;
        let (segments, trailing) = parse_pattern(pattern)?;

        let mut rank = Vec::with_capacity(segments.len() + 1);
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
        self.routes.push(Route {
            method: method.to_owned(),
            segments,
            trailing,
            rank,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        validate_method(method)?;
        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (request_segments, request_trailing) = parse_path(head)?;

        let mut candidates = Vec::new();
        for (index, route) in self.routes.iter().enumerate() {
            if let Some(params) = match_route(route, &request_segments) {
                let slash_ok = self.policy != SlashPolicy::Strict
                    || route.trailing == request_trailing;
                if slash_ok {
                    candidates.push((index, params));
                }
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible_method = if method == "HEAD"
            && candidates
                .iter()
                .any(|(i, _)| self.routes[*i].method == "HEAD")
        {
            "HEAD"
        } else if method == "HEAD" {
            "GET"
        } else {
            method
        };

        let eligible: Vec<(usize, Vec<(String, String)>)> = candidates
            .iter()
            .filter(|(i, _)| self.routes[*i].method == eligible_method)
            .cloned()
            .collect();

        if eligible.is_empty() {
            let mut allowed: Vec<String> = candidates
                .iter()
                .map(|(i, _)| self.routes[*i].method.clone())
                .collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let mut winner = &eligible[0];
        for candidate in eligible.iter().skip(1) {
            if self.routes[candidate.0].rank > self.routes[winner.0].rank {
                winner = candidate;
            }
        }

        let route = &self.routes[winner.0];
        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing != request_trailing
            && !head.is_empty()
            && head != "/"
        {
            if route.trailing {
                Some(format!("{head}/"))
            } else {
                Some(head.strip_suffix('/').unwrap_or(head).to_owned())
            }
        } else {
            None
        };

        Ok(Match {
            route: winner.0,
            params: winner.1.clone(),
            redirect,
        })
    }
}

fn validate_method(method: &str) -> Result<(), Error> {
    if !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(Error::InvalidMethod)
    }
}

fn parse_pattern(input: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let (parts, trailing) = split_body(input);
    let mut segments = Vec::with_capacity(parts.len());
    let mut names = Vec::<String>::new();

    for (start, raw) in parts {
        if raw.is_empty() {
            return Err(Error::InvalidPattern { at: start });
        }

        let first = raw.as_bytes()[0];
        if first == b':' || first == b'*' {
            let wildcard = first == b'*';
            let name = &raw[1..];
            if !valid_name(name) || names.iter().any(|n| n == name) {
                return Err(Error::InvalidPattern { at: start });
            }
            if wildcard && start + raw.len() != input.len() - usize::from(trailing) {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.to_owned());
            segments.push(if wildcard {
                Segment::Wildcard(name.to_owned())
            } else {
                Segment::Param(name.to_owned())
            });
        } else {
            let decoded = decode_segment(raw, start).map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }
    }
    Ok((segments, trailing))
}

fn parse_path(input: &str) -> Result<(Vec<String>, bool), Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let (parts, trailing) = split_body(input);
    let mut segments = Vec::with_capacity(parts.len());
    for (start, raw) in parts {
        if raw.is_empty() {
            return Err(Error::InvalidPath { at: start });
        }
        segments.push(decode_segment(raw, start).map_err(|at| Error::InvalidPath { at })?);
    }
    Ok((segments, trailing))
}

fn split_body(input: &str) -> (Vec<(usize, &str)>, bool) {
    let trailing = input.ends_with('/');
    let end = if trailing { input.len() - 1 } else { input.len() };
    let mut parts = Vec::new();
    let mut start = 1;
    let bytes = input.as_bytes();
    let mut i = 1;
    while i < end {
        if bytes[i] == b'/' {
            parts.push((start, &input[start..i]));
            start = i + 1;
        }
        i += 1;
    }
    parts.push((start, &input[start..end]));
    (parts, trailing)
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_segment(raw: &str, offset: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(offset + i);
            }
            let hi = upper_hex(bytes[i + 1]).ok_or(offset + i)?;
            let lo = upper_hex(bytes[i + 2]).ok_or(offset + i)?;
            let decoded = (hi << 4) | lo;
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || is_unreserved(decoded)
            {
                return Err(offset + i);
            }
            output.push(decoded);
            i += 3;
        } else if is_unreserved(b) {
            output.push(b);
            i += 1;
        } else {
            return Err(offset + i);
        }
    }
    Ok(String::from_utf8(output).expect("decoded segment is ASCII"))
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

fn match_route(route: &Route, request: &[String]) -> Option<Vec<(String, String)>> {
    let wildcard_at = route
        .segments
        .iter()
        .position(|s| matches!(s, Segment::Wildcard(_)));
    let fixed_len = wildcard_at.unwrap_or(route.segments.len());
    if wildcard_at.is_none() && request.len() != route.segments.len() {
        return None;
    }
    if wildcard_at.is_some() && request.len() < fixed_len {
        return None;
    }

    let mut params = Vec::new();
    for i in 0..fixed_len {
        match &route.segments[i] {
            Segment::Static(value) if value != &request[i] => return None,
            Segment::Static(_) => {}
            Segment::Param(name) => params.push((name.clone(), request[i].clone())),
            Segment::Wildcard(_) => unreachable!(),
        }
    }
    if let Some(index) = wildcard_at {
        let Segment::Wildcard(name) = &route.segments[index] else {
            unreachable!()
        };
        params.push((name.clone(), request[index..].join("/")));
    }
    Some(params)
}

fn main() {}