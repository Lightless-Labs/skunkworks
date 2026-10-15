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

fn valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(|b| b.is_ascii_uppercase())
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn decode_segment(input: &str, base: usize) -> Result<String, usize> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(base + i);
            }
            let hi = hex_value(bytes[i + 1]).ok_or(base + i)?;
            let lo = hex_value(bytes[i + 2]).ok_or(base + i)?;
            let decoded = (hi << 4) | lo;

            if !(0x20..=0x7e).contains(&decoded)
                || decoded == b'/'
                || unreserved(decoded)
            {
                return Err(base + i);
            }

            output.push(decoded);
            i += 3;
        } else if unreserved(b) {
            output.push(b);
            i += 1;
        } else {
            return Err(base + i);
        }
    }

    Ok(String::from_utf8(output).expect("decoded bytes are ASCII"))
}

fn valid_capture_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn parse_path(input: &str, is_pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'/') {
        return Err(0);
    }
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let mut pieces: Vec<(&str, usize)> = Vec::new();
    let mut start = 1;
    let mut trailing = false;

    for (idx, &byte) in bytes.iter().enumerate().skip(1) {
        if byte == b'/' {
            if idx == bytes.len() - 1 {
                if idx == start {
                    return Err(start);
                }
                pieces.push((&input[start..idx], start));
                trailing = true;
                break;
            }
            if idx == start {
                return Err(start);
            }
            pieces.push((&input[start..idx], start));
            start = idx + 1;
        }
    }

    if !trailing {
        if start == bytes.len() {
            return Err(start);
        }
        pieces.push((&input[start..], start));
    }

    let mut segments = Vec::with_capacity(pieces.len());
    let mut names = std::collections::HashSet::new();
    let mut wildcard_at = None;

    for (index, (text, at)) in pieces.iter().enumerate() {
        if is_pattern && (text.starts_with(':') || text.starts_with('*')) {
            let marker = text.as_bytes()[0];
            let name = &text[1..];

            if !valid_capture_name(name) || !names.insert(name.to_owned()) {
                return Err(*at);
            }
            if marker == b'*' {
                wildcard_at = Some(*at);
                if index + 1 != pieces.len() {
                    return Err(*at);
                }
                segments.push(Segment::Wildcard(name.to_owned()));
            } else {
                segments.push(Segment::Param(name.to_owned()));
            }
        } else {
            segments.push(Segment::Static(decode_segment(text, *at)?));
        }
    }

    let _ = wildcard_at;
    Ok((segments, trailing))
}

fn rank_for(segments: &[Segment]) -> Vec<u8> {
    let mut rank = Vec::with_capacity(segments.len() + 1);
    for segment in segments {
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

fn match_segments(pattern: &[Segment], path: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut path_index = 0;

    for segment in pattern {
        match segment {
            Segment::Static(value) => {
                if path.get(path_index)? != value {
                    return None;
                }
                path_index += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), path.get(path_index)?.clone()));
                path_index += 1;
            }
            Segment::Wildcard(name) => {
                let value = path[path_index..].join("/");
                params.push((name.clone(), value));
                path_index = path.len();
                break;
            }
        }
    }

    if path_index == path.len() {
        Some(params)
    } else {
        None
    }
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

        let (segments, trailing) =
            parse_path(pattern, true).map_err(|at| Error::InvalidPattern { at })?;
        let route_id = self.routes.len();
        let rank = rank_for(&segments);

        self.routes.push(Route {
            method: method.to_owned(),
            segments,
            trailing,
            rank,
        });

        Ok(route_id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        if !valid_method(method) {
            return Err(Error::InvalidMethod);
        }

        let head_end = path
            .char_indices()
            .find_map(|(i, c)| if c == '?' || c == '#' { Some(i) } else { None })
            .unwrap_or(path.len());
        let head = &path[..head_end];

        let (path_segments, path_trailing) =
            parse_path(head, false).map_err(|at| Error::InvalidPath { at })?;
        let decoded_path: Vec<String> = path_segments
            .into_iter()
            .map(|segment| match segment {
                Segment::Static(value) => value,
                _ => unreachable!(),
            })
            .collect();

        let mut candidates = Vec::new();
        for (index, route) in self.routes.iter().enumerate() {
            let slash_matches = self.policy != SlashPolicy::Strict || route.trailing == path_trailing;
            if slash_matches {
                if let Some(params) = match_segments(&route.segments, &decoded_path) {
                    candidates.push((index, params));
                }
            }
        }

        if candidates.is_empty() {
            return Err(Error::NotFound);
        }

        let eligible_method = if method == "HEAD"
            && candidates.iter().any(|(index, _)| self.routes[*index].method == "HEAD")
        {
            "HEAD"
        } else if method == "HEAD" {
            "GET"
        } else {
            method
        };

        let mut eligible: Vec<(usize, Vec<(String, String)>)> = candidates
            .iter()
            .filter(|(index, _)| self.routes[*index].method == eligible_method)
            .cloned()
            .collect();

        if eligible.is_empty() {
            let mut allowed = Vec::new();
            for (index, _) in &candidates {
                let candidate_method = &self.routes[*index].method;
                if !allowed.contains(candidate_method) {
                    allowed.push(candidate_method.clone());
                }
            }
            if allowed.iter().any(|m| m == "GET") && !allowed.iter().any(|m| m == "HEAD") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            return Err(Error::MethodNotAllowed { allowed });
        }

        eligible.sort_by(|(left, _), (right, _)| {
            self.routes[*right]
                .rank
                .cmp(&self.routes[*left].rank)
                .then_with(|| left.cmp(right))
        });

        let (route_index, params) = eligible.remove(0);
        let route = &self.routes[route_index];

        let redirect = if self.policy == SlashPolicy::Redirect && route.trailing != path_trailing {
            if head == "/" {
                None
            } else if route.trailing {
                Some(format!("{head}/"))
            } else {
                Some(head.to_owned())
            }
        } else {
            None
        };

        Ok(Match {
            route: route_index,
            params,
            redirect,
        })
    }
}