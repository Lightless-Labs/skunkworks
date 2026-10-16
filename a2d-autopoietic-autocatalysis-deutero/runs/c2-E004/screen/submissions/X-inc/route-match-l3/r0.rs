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
    trailing_slash: bool,
    rank: Vec<u8>,
    wildcard: bool,
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
        let wildcard = matches!(segments.last(), Some(Segment::Wildcard(_)));
        let mut rank = Vec::with_capacity(segments.len() + 1);
        for segment in &segments {
            rank.push(match segment {
                Segment::Static(_) => 2,
                Segment::Param(_) => 1,
                Segment::Wildcard(_) => 0,
            });
        }
        if !wildcard {
            rank.push(3);
        }

        let id = self.routes.len();
        self.routes.push(Route {
            method: method.to_owned(),
            segments,
            trailing_slash,
            rank,
            wildcard,
        });
        Ok(id)
    }

    pub fn resolve(&self, method: &str, path: &str) -> Result<Match, Error> {
        validate_method(method)?;

        let head_end = path
            .char_indices()
            .find_map(|(i, c)| (c == '?' || c == '#').then_some(i))
            .unwrap_or(path.len());
        let head = &path[..head_end];
        let (request_segments, trailing_slash) = parse_path(head)?;

        let mut candidates = Vec::new();
        for (index, route) in self.routes.iter().enumerate() {
            if let Some(params) = match_route(route, &request_segments) {
                let slash_ok = match self.policy {
                    SlashPolicy::Strict => route.trailing_slash == trailing_slash,
                    SlashPolicy::Ignore | SlashPolicy::Redirect => true,
                };
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

        let mut eligible: Vec<(usize, Vec<(String, String)>)> = candidates
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

        eligible.sort_by(|(a, _), (b, _)| {
            self.routes[*b]
                .rank
                .cmp(&self.routes[*a].rank)
                .then_with(|| a.cmp(b))
        });
        let (winner, params) = eligible.remove(0);
        let route = &self.routes[winner];

        let redirect = if self.policy == SlashPolicy::Redirect
            && route.trailing_slash != trailing_slash
            && !head.eq("/")
        {
            let mut target = head.to_owned();
            if route.trailing_slash {
                target.push('/');
            } else if target.ends_with('/') {
                target.pop();
            }
            Some(target)
        } else {
            None
        };

        Ok(Match {
            route: winner,
            params,
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

fn parse_pattern(pattern: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !pattern.starts_with('/') {
        return Err(Error::InvalidPattern { at: 0 });
    }
    parse_segments(pattern, true).map_err(|at| Error::InvalidPattern { at })
}

fn parse_path(path: &str) -> Result<(Vec<Segment>, bool), Error> {
    if !path.starts_with('/') {
        return Err(Error::InvalidPath { at: 0 });
    }
    parse_segments(path, false).map_err(|at| Error::InvalidPath { at })
}

fn parse_segments(input: &str, pattern: bool) -> Result<(Vec<Segment>, bool), usize> {
    if input == "/" {
        return Ok((Vec::new(), false));
    }

    let bytes = input.as_bytes();
    let trailing_slash = bytes.last() == Some(&b'/');
    let end = if trailing_slash {
        bytes.len() - 1
    } else {
        bytes.len()
    };

    let mut segments = Vec::new();
    let mut names = Vec::<String>::new();
    let mut start = 1usize;

    while start < end {
        let segment_end = input[start..end]
            .find('/')
            .map(|i| start + i)
            .unwrap_or(end);

        if segment_end == start {
            return Err(start);
        }

        let raw = &input[start..segment_end];
        if pattern && (raw.starts_with(':') || raw.starts_with('*')) {
            let marker = raw.as_bytes()[0];
            let name = &raw[1..];
            if !valid_capture_name(name) || names.iter().any(|n| n == name) {
                return Err(start);
            }
            if marker == b'*' && segment_end != end {
                return Err(start);
            }
            names.push(name.to_owned());
            segments.push(if marker == b':' {
                Segment::Param(name.to_owned())
            } else {
                Segment::Wildcard(name.to_owned())
            });
        } else {
            let decoded = decode_static(raw, start)?;
            segments.push(Segment::Static(decoded));
        }

        if segment_end == end {
            break;
        }
        start = segment_end + 1;
    }

    Ok((segments, trailing_slash))
}

fn valid_capture_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn decode_static(raw: &str, base: usize) -> Result<String, usize> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
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
                return Err(base + i);
            }
            let hi = hex_value(bytes[i + 1]).ok_or(base + i)?;
            let lo = hex_value(bytes[i + 2]).ok_or(base + i)?;
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

    Ok(String::from_utf8(out).expect("decoded bytes are ASCII"))
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-')
}

fn match_route(route: &Route, request: &[Segment]) -> Option<Vec<(String, String)>> {
    let mut params = Vec::new();
    let mut ri = 0usize;

    for segment in &route.segments {
        match segment {
            Segment::Static(expected) => {
                let Some(Segment::Static(actual)) = request.get(ri) else {
                    return None;
                };
                if expected != actual {
                    return None;
                }
                ri += 1;
            }
            Segment::Param(name) => {
                let Some(Segment::Static(value)) = request.get(ri) else {
                    return None;
                };
                params.push((name.clone(), value.clone()));
                ri += 1;
            }
            Segment::Wildcard(name) => {
                let rest = request[ri..]
                    .iter()
                    .map(|s| match s {
                        Segment::Static(v) => v.as_str(),
                        _ => "",
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                if request[ri..]
                    .iter()
                    .any(|s| !matches!(s, Segment::Static(_)))
                {
                    return None;
                }
                params.push((name.clone(), rest));
                ri = request.len();
            }
        }
    }

    if ri != request.len() {
        return None;
    }

    // Wildcards consume all remaining path segments, including the empty remainder.
    let _ = route.wildcard;
    Some(params)
}