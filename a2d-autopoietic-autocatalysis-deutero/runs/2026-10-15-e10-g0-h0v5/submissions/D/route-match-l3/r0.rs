use std::collections::BTreeSet;

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
enum PatternSegment {
    Static(String),
    Param(String),
    Wildcard(String),
}

#[derive(Debug, Clone)]
struct Route {
    id: usize,
    method: String,
    segments: Vec<PatternSegment>,
    trailing_slash: bool,
    rank: Vec<u8>,
    has_wildcard: bool,
}

#[derive(Debug, Clone)]
struct DecodedPath {
    segments: Vec<String>,
    trailing_slash: bool,
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

        let (segments, trailing_slash) = parse_pattern(pattern)?;
        let has_wildcard = segments
            .iter()
            .any(|segment| matches!(segment, PatternSegment::Wildcard(_)));
        let mut rank = Vec::with_capacity(segments.len() + 1);
        for segment in &segments {
            rank.push(match segment {
                PatternSegment::Static(_) => 2,
                PatternSegment::Param(_) => 1,
                PatternSegment::Wildcard(_) => 0,
            });
        }
        if !has_wildcard {
            rank.push(3);
        }

        let id = self.routes.len();
        self.routes.push(Route {
            id,
            method: method.to_owned(),
            segments,
            trailing_slash,
            rank,
            has_wildcard,
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
        let decoded = parse_request_path(head)?;

        let mut candidates = Vec::new();
        for route in &self.routes {
            if self.path_matches(route, &decoded).is_some() {
                candidates.push(route);
            }
        }
        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible: Vec<&Route> = if method == "HEAD" {
            let head_routes: Vec<&Route> = candidates
                .iter()
                .copied()
                .filter(|route| route.method == "HEAD")
                .collect();
            if head_routes.is_empty() {
                candidates
                    .iter()
                    .copied()
                    .filter(|route| route.method == "GET")
                    .collect()
            } else {
                head_routes
            }
        } else {
            candidates
                .iter()
                .copied()
                .filter(|route| route.method == method)
                .collect()
        };

        if eligible.is_empty() {
            let mut allowed = BTreeSet::new();
            for route in candidates {
                allowed.insert(route.method.as_str());
            }
            if allowed.contains("GET") {
                allowed.insert("HEAD");
            }
            return Err(Error::MethodNotAllowed {
                allowed: allowed.into_iter().map(str::to_owned).collect(),
            });
        }

        let mut best: Option<(&Route, Vec<(String, String)>)> = None;
        for route in eligible {
            let Some(params) = self.path_matches(route, &decoded) else {
                continue;
            };
            let replace = match best.as_ref() {
                None => true,
                Some((best_route, _)) => route.rank > best_route.rank,
            };
            if replace {
                best = Some((route, params));
            }
        }

        let (route, params) = best.expect("eligible routes are path candidates");
        let redirect = if self.policy == SlashPolicy::Redirect
            && !head.is_empty()
            && head != "/"
            && route.trailing_slash != decoded.trailing_slash
        {
            if route.trailing_slash {
                Some(format!("{head}/"))
            } else {
                Some(head.to_owned())
            }
        } else {
            None
        };

        Ok(Match {
            route: route.id,
            params,
            redirect,
        })
    }

    fn path_matches(
        &self,
        route: &Route,
        path: &DecodedPath,
    ) -> Option<Vec<(String, String)>> {
        if self.policy == SlashPolicy::Strict
            && route.trailing_slash != path.trailing_slash
        {
            return None;
        }

        let mut params = Vec::new();
        let wildcard_index = route
            .segments
            .iter()
            .position(|segment| matches!(segment, PatternSegment::Wildcard(_)));

        if let Some(index) = wildcard_index {
            if path.segments.len() < index {
                return None;
            }
            for i in 0..index {
                if !match_segment(&route.segments[i], &path.segments[i], &mut params) {
                    return None;
                }
            }
            let PatternSegment::Wildcard(name) = &route.segments[index] else {
                unreachable!()
            };
            params.push((name.clone(), path.segments[index..].join("/")));
        } else {
            if path.segments.len() != route.segments.len() {
                return None;
            }
            for (pattern_segment, path_segment) in
                route.segments.iter().zip(path.segments.iter())
            {
                if !match_segment(pattern_segment, path_segment, &mut params) {
                    return None;
                }
            }
        }

        Some(params)
    }
}

fn validate_method(method: &str) -> Result<(), Error> {
    if !method.is_empty()
        && method
            .bytes()
            .all(|byte| byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(Error::InvalidMethod)
    }
}

fn parse_pattern(pattern: &str) -> Result<(Vec<PatternSegment>, bool), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if pattern == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = pattern.as_bytes();
    let trailing_slash = bytes.last() == Some(&b'/');
    let end = if trailing_slash {
        bytes.len() - 1
    } else {
        bytes.len()
    };
    let mut result = Vec::new();
    let mut names = BTreeSet::new();
    let mut start = 1;

    while start < end {
        let next = bytes[start..end]
            .iter()
            .position(|&byte| byte == b'/')
            .map(|offset| start + offset)
            .unwrap_or(end);

        if next == start {
            return Err(Error::InvalidPattern { at: start });
        }

        let segment = &pattern[start..next];
        let first = bytes[start];
        if first == b':' || first == b'*' {
            let name = &segment[1..];
            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(Error::InvalidPattern { at: start });
            }
            if first == b'*' && next != end {
                return Err(Error::InvalidPattern { at: start });
            }
            result.push(if first == b':' {
                PatternSegment::Param(name.to_owned())
            } else {
                PatternSegment::Wildcard(name.to_owned())
            });
        } else {
            let decoded = decode_static_segment(segment, start, true)
                .map_err(|at| Error::InvalidPattern { at })?;
            result.push(PatternSegment::Static(decoded));
        }

        if next == end {
            break;
        }
        start = next + 1;
    }

    Ok((result, trailing_slash))
}

fn parse_request_path(path: &str) -> Result<DecodedPath, Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if path == "/" {
        return Ok(DecodedPath {
            segments: Vec::new(),
            trailing_slash: false,
        });
    }

    let bytes = path.as_bytes();
    let trailing_slash = bytes.last() == Some(&b'/');
    let end = if trailing_slash {
        bytes.len() - 1
    } else {
        bytes.len()
    };

    let mut segments = Vec::new();
    let mut start = 1;
    while start < end {
        let next = bytes[start..end]
            .iter()
            .position(|&byte| byte == b'/')
            .map(|offset| start + offset)
            .unwrap_or(end);

        if next == start {
            return Err(Error::InvalidPath { at: start });
        }

        let segment = &path[start..next];
        let decoded = decode_static_segment(segment, start, false)
            .map_err(|at| Error::InvalidPath { at })?;
        segments.push(decoded);

        if next == end {
            break;
        }
        start = next + 1;
    }

    Ok(DecodedPath {
        segments,
        trailing_slash,
    })
}

fn decode_static_segment(
    segment: &str,
    absolute_start: usize,
    pattern_static: bool,
) -> Result<String, usize> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err(absolute_start + i);
                }
                let Some(high) = uppercase_hex(bytes[i + 1]) else {
                    return Err(absolute_start + i);
                };
                let Some(low) = uppercase_hex(bytes[i + 2]) else {
                    return Err(absolute_start + i);
                };
                let value = (high << 4) | low;
                if !(0x20..=0x7e).contains(&value)
                    || value == b'/'
                    || is_unreserved(value)
                {
                    return Err(absolute_start + i);
                }
                decoded.push(value);
                i += 3;
            }
            byte if is_unreserved(byte) => {
                decoded.push(byte);
                i += 1;
            }
            _ => return Err(absolute_start + i),
        }
    }

    if pattern_static {
        // This branch exists to keep the shared validator explicit at call sites.
    }
    Ok(String::from_utf8(decoded).expect("decoded bytes are ASCII"))
}

fn uppercase_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

fn match_segment(
    pattern: &PatternSegment,
    path: &str,
    params: &mut Vec<(String, String)>,
) -> bool {
    match pattern {
        PatternSegment::Static(value) => value == path,
        PatternSegment::Param(name) => {
            params.push((name.clone(), path.to_owned()));
            true
        }
        PatternSegment::Wildcard(_) => false,
    }
}