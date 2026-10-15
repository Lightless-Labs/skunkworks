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
    trailing_slash: bool,
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
        let parsed = parse_pattern(pattern)?;
        let id = self.routes.len();
        let mut rank = Vec::with_capacity(parsed.segments.len() + 1);
        let has_wildcard = parsed
            .segments
            .iter()
            .any(|segment| matches!(segment, Segment::Wildcard(_)));
        for segment in &parsed.segments {
            rank.push(match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            });
        }
        if !has_wildcard {
            rank.push(3);
        }

        self.routes.push(Route {
            id,
            method: method.to_owned(),
            segments: parsed.segments,
            trailing_slash: parsed.trailing_slash,
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
        let parsed = parse_path(head)?;
        let mut candidates: Vec<(&Route, Vec<(String, String)>)> = Vec::new();

        for route in &self.routes {
            if self.policy == SlashPolicy::Strict
                && route.trailing_slash != parsed.trailing_slash
            {
                continue;
            }
            if let Some(params) = match_segments(&route.segments, &parsed.segments) {
                candidates.push((route, params));
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
            let mut allowed: Vec<String> = candidates
                .iter()
                .map(|(route, _)| route.method.clone())
                .collect();
            if allowed.iter().any(|m| m == "GET") {
                allowed.push("HEAD".to_owned());
            }
            allowed.sort();
            allowed.dedup();
            return Err(Error::MethodNotAllowed { allowed });
        }

        let (winner, params) = eligible
            .into_iter()
            .max_by(|(a, _), (b, _)| a.rank.cmp(&b.rank).then_with(|| b.id.cmp(&a.id)))
            .unwrap();

        let redirect = if self.policy == SlashPolicy::Redirect
            && winner.trailing_slash != parsed.trailing_slash
            && !parsed.segments.is_empty()
        {
            if winner.trailing_slash {
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
}

struct ParsedPath {
    segments: Vec<String>,
    trailing_slash: bool,
}

struct ParsedPattern {
    segments: Vec<Segment>,
    trailing_slash: bool,
}

fn validate_method(method: &str) -> Result<(), Error> {
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        Err(Error::InvalidMethod)
    } else {
        Ok(())
    }
}

fn parse_pattern(input: &str) -> Result<ParsedPattern, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    if input == "/" {
        return Ok(ParsedPattern {
            segments: Vec::new(),
            trailing_slash: false,
        });
    }

    let trailing_slash = input.ends_with('/');
    let body_end = if trailing_slash {
        input.len() - 1
    } else {
        input.len()
    };
    let body = &input[1..body_end];
    let mut segments = Vec::new();
    let mut names = Vec::<String>::new();
    let mut start = 1usize;

    for (idx, raw) in body.split('/').enumerate() {
        if raw.is_empty() {
            return Err(Error::InvalidPattern { at: start });
        }

        let first = raw.as_bytes()[0];
        if first == b':' || first == b'*' {
            let name = &raw[1..];
            if !valid_name(name) {
                return Err(Error::InvalidPattern { at: start });
            }
            if names.iter().any(|existing| existing == name) {
                return Err(Error::InvalidPattern { at: start });
            }
            if first == b'*' && idx + 1 != body.split('/').count() {
                return Err(Error::InvalidPattern { at: start });
            }
            names.push(name.to_owned());
            segments.push(if first == b':' {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            });
        } else {
            segments.push(Segment::Static(decode_segment(raw, start, true)?));
        }
        start += raw.len() + 1;
    }

    Ok(ParsedPattern {
        segments,
        trailing_slash,
    })
}

fn parse_path(input: &str) -> Result<ParsedPath, Error> {
    if !input.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    if input == "/" {
        return Ok(ParsedPath {
            segments: Vec::new(),
            trailing_slash: false,
        });
    }

    let trailing_slash = input.ends_with('/');
    let body_end = if trailing_slash {
        input.len() - 1
    } else {
        input.len()
    };
    let body = &input[1..body_end];
    let mut segments = Vec::new();
    let mut start = 1usize;

    for raw in body.split('/') {
        if raw.is_empty() {
            return Err(Error::InvalidPath { at: start });
        }
        segments.push(decode_segment(raw, start, false)?);
        start += raw.len() + 1;
    }

    Ok(ParsedPath {
        segments,
        trailing_slash,
    })
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_segment(raw: &str, base: usize, pattern: bool) -> Result<String, Error> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
                || bytes[i + 1].is_ascii_lowercase()
                || bytes[i + 2].is_ascii_lowercase()
            {
                return Err(parse_error(pattern, base + i));
            }
            let value = (hex_value(bytes[i + 1]) << 4) | hex_value(bytes[i + 2]);
            if !(0x20..=0x7e).contains(&value)
                || value == b'/'
                || is_unreserved(value)
            {
                return Err(parse_error(pattern, base + i));
            }
            decoded.push(value);
            i += 3;
        } else {
            if !is_unreserved(b) {
                return Err(parse_error(pattern, base + i));
            }
            decoded.push(b);
            i += 1;
        }
    }

    Ok(String::from_utf8(decoded).unwrap())
}

fn parse_error(pattern: bool, at: usize) -> Error {
    if pattern {
        Error::InvalidPattern { at }
    } else {
        Error::InvalidPath { at }
    }
}

fn hex_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_segments(pattern: &[Segment], path: &[String]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut p = 0usize;

    for segment in pattern {
        match segment {
            Segment::Static(value) => {
                if path.get(p)? != value {
                    return None;
                }
                p += 1;
            }
            Segment::Param(name) => {
                params.push((name.clone(), path.get(p)?.clone()));
                p += 1;
            }
            Segment::Wildcard(name) => {
                params.push((name.clone(), path[p..].join("/")));
                p = path.len();
                break;
            }
        }
    }

    if p == path.len() {
        Some(params)
    } else {
        None
    }
}

fn main() {}