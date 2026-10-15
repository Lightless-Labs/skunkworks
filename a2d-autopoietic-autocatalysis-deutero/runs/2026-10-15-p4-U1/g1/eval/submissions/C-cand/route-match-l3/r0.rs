use std::collections::HashSet;

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
    id: usize,
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

fn decode_segment(s: &str, base: usize, pattern: bool) -> Result<String, Error> {
    let error = |at| {
        if pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };

    let bytes = s.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == b'%' {
            if i + 2 >= bytes.len() {
                return Err(error(base + i));
            }

            let (Some(high), Some(low)) = (hex_upper(bytes[i + 1]), hex_upper(bytes[i + 2]))
            else {
                return Err(error(base + i));
            };

            let decoded = (high << 4) | low;
            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || unreserved(decoded)
            {
                return Err(error(base + i));
            }

            output.push(decoded);
            i += 3;
        } else if unreserved(byte) {
            output.push(byte);
            i += 1;
        } else {
            return Err(error(base + i));
        }
    }

    Ok(String::from_utf8(output).unwrap())
}

fn parse_path(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), Error> {
    let error = |at| {
        if pattern {
            Error::InvalidPattern { at }
        } else {
            Error::InvalidPath { at }
        }
    };

    if !input.starts_with('/') {
        return Err(error(0));
    }

    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let mut parts: Vec<(usize, &str)> = Vec::new();
    let mut start = 1;

    for (i, byte) in input.bytes().enumerate().skip(1) {
        if byte == b'/' {
            parts.push((start, &input[start..i]));
            start = i + 1;
        }
    }

    let trailing = input.ends_with('/');
    if trailing {
        if start < input.len() {
            parts.push((start, &input[start..]));
        }
    } else {
        parts.push((start, &input[start..]));
    }

    let mut segments = Vec::new();
    let mut names = HashSet::new();

    for (index, (at, raw)) in parts.iter().enumerate() {
        if raw.is_empty() {
            return Err(error(*at));
        }

        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let wildcard = raw.starts_with('*');
            let name = &raw[1..];
            let bytes = name.as_bytes();

            let valid_name = !bytes.is_empty()
                && bytes[0].is_ascii_lowercase()
                && bytes[1..]
                    .iter()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_');

            if !valid_name || !names.insert(name.to_string()) {
                return Err(error(*at));
            }

            if wildcard && index + 1 != parts.len() {
                return Err(error(*at));
            }

            if wildcard {
                segments.push(Segment::Wildcard(name.to_string()));
            } else {
                segments.push(Segment::Param(name.to_string()));
            }
        } else {
            segments.push(Segment::Static(decode_segment(raw, *at, pattern)?));
        }
    }

    Ok((segments, trailing))
}

fn path_head(path: &str) -> &str {
    let end = path
        .char_indices()
        .find(|(_, c)| *c == '?' || *c == '#')
        .map(|(i, _)| i)
        .unwrap_or(path.len());

    &path[..end]
}

impl Router {
    pub fn new(policy: SlashPolicy) -> Self {
        Self {
            policy,
            routes: Vec::new(),
        }
    }

    pub fn add(&mut self, method: &str, pattern: &str) -> Result<usize, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }

        let (segments, trailing) = parse_path(pattern, true)?;
        let has_wildcard = matches!(segments.last(), Some(Segment::Wildcard(_)));

        let mut rank = Vec::with_capacity(segments.len() + 1);
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
            id,
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

        let head = path_head(path);
        let (path_segments, trailing) = parse_path(head, false)?;
        let decoded: Vec<String> = path_segments
            .into_iter()
            .map(|segment| match segment {
                Segment::Static(value) => value,
                _ => unreachable!(),
            })
            .collect();

        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();

        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != trailing {
                continue;
            }

            let mut params = Vec::new();
            let mut path_index = 0;
            let mut matches = true;

            for segment in &route.segments {
                match segment {
                    Segment::Static(expected) => {
                        if decoded.get(path_index) != Some(expected) {
                            matches = false;
                            break;
                        }
                        path_index += 1;
                    }
                    Segment::Param(name) => {
                        if let Some(value) = decoded.get(path_index) {
                            params.push((name.clone(), value.clone()));
                            path_index += 1;
                        } else {
                            matches = false;
                            break;
                        }
                    }
                    Segment::Wildcard(name) => {
                        params.push((name.clone(), decoded[path_index..].join("/")));
                        path_index = decoded.len();
                    }
                }
            }

            if matches && path_index == decoded.len() {
                candidates.push((route, params));
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let selected_method = if method == "HEAD" {
            if candidates.iter().any(|(route, _)| route.method == "HEAD") {
                "HEAD"
            } else {
                "GET"
            }
        } else {
            method
        };

        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(route, _)| route.method == selected_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed: Vec<String> = candidates
                .iter()
                .map(|(route, _)| route.method.clone())
                .collect();

            if allowed.iter().any(|method| method == "GET") {
                allowed.push("HEAD".to_string());
            }

            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let (route, params) = eligible
            .into_iter()
            .max_by(|(a, _), (b, _)| {
                a.rank
                    .cmp(&b.rank)
                    .then_with(|| b.id.cmp(&a.id))
            })
            .unwrap();

        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing != trailing
            && head != "/"
        {
            if route.trailing {
                Some(format!("{head}/"))
            } else {
                Some(head.strip_suffix('/').unwrap_or(head).to_string())
            }
        } else {
            None
        };

        Ok(Match {
            route: route.id,
            params: params.clone(),
            redirect,
        })
    }
}

fn main() {}