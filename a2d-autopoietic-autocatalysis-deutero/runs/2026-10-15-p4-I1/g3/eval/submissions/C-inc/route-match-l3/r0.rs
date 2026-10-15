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

#[derive(Debug, Clone)]
enum Segment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Debug, Clone)]
struct Route {
    id: usize,
    method: String,
    segments: Vec<Segment>,
    trailing: bool,
}

#[derive(Debug)]
pub struct Router {
    policy: SlashPolicy,
    routes: Vec<Route>,
}

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
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

fn decode_segment(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let Some(hi) = hex_value(bytes[i + 1]) else {
                return Err(base + i);
            };
            let Some(lo) = hex_value(bytes[i + 2]) else {
                return Err(base + i);
            };
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

    // Decoded bytes are guaranteed ASCII.
    Ok(String::from_utf8(out).expect("decoded segment is ASCII"))
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = pattern.as_bytes();
    let trailing = pattern.ends_with('/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut start = 1;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let raw = &pattern[start..stop];
        let first = raw.as_bytes()[0];

        if first == b':' || first == b'*' {
            let name = &raw[1..];
            let is_wildcard = first == b'*';
            if is_wildcard && stop < end {
                return Err(Error::InvalidPattern { at: start });
            }
            if !valid_capture_name(name) || !names.insert(name.to_string()) {
                return Err(Error::InvalidPattern { at: start });
            }
            segments.push(if is_wildcard {
                Segment::Wildcard(name.to_string())
            } else {
                Segment::Param(name.to_string())
            });
        } else {
            let decoded = decode_segment(raw, start)
                .map_err(|at| Error::InvalidPattern { at })?;
            segments.push(Segment::Static(decoded));
        }

        if stop == end {
            break;
        }
        start = stop + 1;
        if start == end {
            break;
        }
    }

    Ok((segments, trailing))
}

fn parse_path(path: &str) -> Result<(Vec<String>, bool), Error> {
    let head_end = path
        .char_indices()
        .find_map(|(i, c)| (c == '?' || c == '#').then_some(i))
        .unwrap_or(path.len());
    let head = &path[..head_end];

    if !head.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if head == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = head.as_bytes();
    let trailing = head.ends_with('/');
    let end = if trailing { bytes.len() - 1 } else { bytes.len() };
    let mut segments = Vec::new();
    let mut start = 1;

    while start < end {
        let mut stop = start;
        while stop < end && bytes[stop] != b'/' {
            stop += 1;
        }
        if stop == start {
            return Err(Error::InvalidPath { at: start });
        }
        let decoded = decode_segment(&head[start..stop], start)
            .map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);

        if stop == end {
            break;
        }
        start = stop + 1;
        if start == end {
            break;
        }
    }

    Ok((segments, trailing))
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
        let (segments, trailing) = parse_pattern(pattern)?;
        let id = self.routes.len();
        self.routes.push(Route {
            id,
            method: method.to_string(),
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

        let mut candidates: Vec<(&Route, Vec<(String, String)>, Vec<u8>)> = Vec::new();
        for route in &self.routes {
            if self.policy == SlashPolicy::Strict && route.trailing != path_trailing {
                continue;
            }

            let wildcard_index = route
                .segments
                .iter()
                .position(|s| matches!(s, Segment::Wildcard(_)));
            let fixed_len = wildcard_index.unwrap_or(route.segments.len());
            if (wildcard_index.is_none() && path_segments.len() != fixed_len)
                || (wildcard_index.is_some() && path_segments.len() < fixed_len)
            {
                continue;
            }

            let mut params = Vec::new();
            let mut rank = Vec::new();
            let mut matched = true;

            for (i, segment) in route.segments.iter().enumerate() {
                match segment {
                    Segment::Static(value) => {
                        if path_segments.get(i) != Some(value) {
                            matched = false;
                            break;
                        }
                        rank.push(2);
                    }
                    Segment::Param(name) => {
                        let Some(value) = path_segments.get(i) else {
                            matched = false;
                            break;
                        };
                        params.push((name.clone(), value.clone()));
                        rank.push(1);
                    }
                    Segment::Wildcard(name) => {
                        let value = path_segments[i..].join("/");
                        params.push((name.clone(), value));
                        rank.push(0);
                        break;
                    }
                }
            }

            if !matched {
                continue;
            }
            if wildcard_index.is_none() {
                rank.push(3);
            }
            candidates.push((route, params, rank));
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let has_head = candidates.iter().any(|(route, _, _)| route.method == "HEAD");
        let effective_method = if method == "HEAD" && !has_head {
            "GET"
        } else {
            method
        };

        let eligible: Vec<_> = candidates
            .iter()
            .filter(|(route, _, _)| route.method == effective_method)
            .collect();

        if eligible.is_empty() {
            let mut allowed = std::collections::BTreeSet::new();
            for (route, _, _) in &candidates {
                allowed.insert(route.method.clone());
                if route.method == "GET" {
                    allowed.insert("HEAD".to_string());
                }
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().collect(),
            });
        }

        let (route, params, _) = eligible
            .into_iter()
            .max_by(|a, b| {
                a.2.cmp(&b.2)
                    .then_with(|| b.0.id.cmp(&a.0.id))
            })
            .expect("eligible routes are nonempty");

        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing != path_trailing
            && !path_segments.is_empty()
        {
            let head_end = path
                .char_indices()
                .find_map(|(i, c)| (c == '?' || c == '#').then_some(i))
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
            params: params.clone(),
            redirect,
        })
    }
}

fn main() {}