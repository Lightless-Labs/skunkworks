use std::net::Ipv6Addr;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub scheme: String,
    pub userinfo: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
    pub ipv6: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    InvalidByte { at: usize },
    InvalidScheme { at: usize },
    MissingAuthority { at: usize },
    InvalidUserInfo { at: usize },
    InvalidHost { at: usize },
    InvalidPort { at: usize },
    InvalidIpv6 { at: usize },
    BadEscape { at: usize },
    InvalidUtf8 { at: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    UserInfo,
    Path,
    Query,
    Fragment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    BadEscape { at: usize },
    InvalidUtf8 { at: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    NotAbsolute,
    BadEscape { at: usize },
    InvalidUtf8 { at: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Base(ParseError),
    Reference(ParseError),
    Path(PathError),
}

fn global_byte_error(input: &str) -> Option<usize> {
    input
        .as_bytes()
        .iter()
        .position(|&b| !(0x21..=0x7e).contains(&b))
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut origins = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(CodecError::BadEscape { at: i });
            }
            let Some(hi) = hex_value(bytes[i + 1]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            let Some(lo) = hex_value(bytes[i + 2]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            decoded.push((hi << 4) | lo);
            origins.push(i);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            origins.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(s) => Ok(s.to_owned()),
        Err(e) => Err(CodecError::InvalidUtf8 {
            at: origins.get(e.valid_up_to()).copied().unwrap_or(input.len()),
        }),
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        let safe = b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'.' | b'_' | b'~')
            || match component {
                Component::UserInfo => b == b':',
                Component::Path => b == b'/',
                Component::Query => matches!(b, b'/' | b'?'),
                Component::Fragment => matches!(b, b'/' | b'?' | b'#'),
            };
        if safe {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn parse_port(port: &str, port_start: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    if let Some(i) = port.as_bytes().iter().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort {
            at: port_start + i,
        });
    }
    if port.len() >= 2 && port.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for &b in port.as_bytes() {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as u32)) {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: port_start }),
        };
        if value > 65535 {
            return Err(ParseError::InvalidPort { at: port_start });
        }
    }
    if value == 0 {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    Ok(value as u16)
}

fn map_codec_error(error: CodecError, offset: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

fn validate_component(
    component: Component,
    text: &str,
    offset: usize,
) -> Result<(), ParseError> {
    decode_component(component, text)
        .map(|_| ())
        .map_err(|e| map_codec_error(e, offset))
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(at) = global_byte_error(input) {
        return Err(ParseError::InvalidByte { at });
    }

    let bytes = input.as_bytes();
    let Some(colon) = bytes.iter().position(|&b| b == b':') else {
        return Err(ParseError::InvalidScheme { at: input.len() });
    };
    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (i, &b) in bytes[..colon].iter().enumerate().skip(1) {
        if !(b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|n| authority_start + n)
        .unwrap_or(input.len());
    let authority = &input[authority_start..authority_end];

    let ats: Vec<usize> = authority
        .as_bytes()
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();
    if ats.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + ats[1],
        });
    }
    let (userinfo, hostport, hostport_start) = if let Some(&at) = ats.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start,
            });
        }
        (
            Some(authority[..at].to_owned()),
            &authority[at + 1..],
            authority_start + at + 1,
        )
    } else {
        (None, authority, authority_start)
    };

    let mut ipv6 = false;
    let (host, port, port_start) = if hostport.starts_with('[') {
        let Some(close_rel) = hostport.as_bytes()[1..].iter().position(|&b| b == b']') else {
            return Err(ParseError::InvalidIpv6 { at: hostport_start });
        };
        let close = close_rel + 1;
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_start });
        }
        let suffix = &hostport[close + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(
                &suffix[1..],
                hostport_start + close + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + close + 1,
            });
        };
        ipv6 = true;
        (interior.to_ascii_lowercase(), parsed_port, 0usize)
    } else {
        let split = hostport.as_bytes().iter().position(|&b| b == b':');
        let (host_text, port_text, port_at) = if let Some(i) = split {
            (&hostport[..i], Some(&hostport[i + 1..]), hostport_start + i + 1)
        } else {
            (hostport, None, 0)
        };
        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_start });
        }
        for (i, &b) in host_text.as_bytes().iter().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_start + i,
                });
            }
        }
        let parsed_port = match port_text {
            Some(p) => Some(parse_port(p, port_at)?),
            None => None,
        };
        (host_text.to_ascii_lowercase(), parsed_port, port_at)
    };
    let _ = (port_start, hostport_start);

    let rest = &input[authority_end..];
    let fragment_rel = rest.as_bytes().iter().position(|&b| b == b'#');
    let before_fragment = fragment_rel.map(|i| &rest[..i]).unwrap_or(rest);
    let fragment = fragment_rel.map(|i| before_fragment.len().checked_add(1).map(|_| rest[i + 1..].to_owned()).unwrap());
    let fragment_offset = fragment_rel.map(|i| authority_end + i + 1);

    let query_rel = before_fragment.as_bytes().iter().position(|&b| b == b'?');
    let before_query = query_rel
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment);
    let query = query_rel.map(|i| before_fragment[i + 1..].to_owned());
    let query_offset = query_rel.map(|i| authority_end + i + 1);

    let path = if before_query.is_empty() {
        "/".to_owned()
    } else {
        before_query.to_owned()
    };

    if let Some(ref u) = userinfo {
        validate_component(Component::UserInfo, u, authority_start)?;
    }
    if path != "/" || !before_query.is_empty() {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let Some(ref q) = query {
        validate_component(Component::Query, q, query_offset.unwrap())?;
    }
    if let Some(ref f) = fragment {
        validate_component(Component::Fragment, f, fragment_offset.unwrap())?;
    }

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host,
        port,
        path,
        query,
        fragment,
        ipv6,
    })
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let raw_segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    let mut final_navigation = false;

    for (i, raw) in raw_segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, raw).map_err(|e| match e {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 + raw_segments[..i].iter().map(|s| s.len() + 1).sum::<usize>() },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 + raw_segments[..i].iter().map(|s| s.len() + 1).sum::<usize>() },
        })?;
        let is_dot = decoded == ".";
        let is_dotdot = decoded == "..";
        final_navigation = i + 1 == raw_segments.len() && (is_dot || is_dotdot);

        if is_dot {
            continue;
        } else if is_dotdot {
            stack.pop();
        } else {
            stack.push(raw);
        }
    }

    if final_navigation {
        stack.push("");
    }
    if stack.is_empty() {
        Ok("/".to_owned())
    } else {
        Ok(format!("/{}", stack.join("/")))
    }
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;

    if let Some(at) = global_byte_error(reference) {
        return Err(ResolveError::Reference(ParseError::InvalidByte { at }));
    }

    let bytes = reference.as_bytes();
    let first_delim = bytes
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(bytes.len());
    if bytes[..first_delim].contains(&b':') {
        let mut url = parse_url(reference).map_err(ResolveError::Reference)?;
        url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    if reference.starts_with("//") {
        let absolute = format!("{}:{}", base_url.scheme, reference);
        let mut url = parse_url(&absolute).map_err(ResolveError::Reference)?;
        url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    let fragment_pos = reference.as_bytes().iter().position(|&b| b == b'#');
    let before_fragment = fragment_pos.map(|i| &reference[..i]).unwrap_or(reference);
    let fragment = fragment_pos.map(|i| reference[i + 1..].to_owned());
    let query_pos = before_fragment.as_bytes().iter().position(|&b| b == b'?');
    let ref_path = query_pos
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment);
    let ref_query = query_pos.map(|i| before_fragment[i + 1..].to_owned());

    let (selected_path, selected_query) = if ref_path.is_empty() {
        (
            base_path.clone(),
            if query_pos.is_some() {
                ref_query
            } else {
                base_url.query.clone()
            },
        )
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path.to_owned()
        } else {
            let slash = base_path.rfind('/').map(|i| &base_path[..=i]).unwrap_or("/");
            format!("{slash}{ref_path}")
        };
        (merged, ref_query)
    };

    let raw_host = if base_url.ipv6 {
        format!("[{}]", base_url.host)
    } else {
        base_url.host.clone()
    };
    let mut absolute = format!("{}://", base_url.scheme);
    if let Some(ref u) = base_url.userinfo {
        absolute.push_str(u);
        absolute.push('@');
    }
    absolute.push_str(&raw_host);
    if let Some(port) = base_url.port {
        absolute.push(':');
        absolute.push_str(&port.to_string());
    }
    absolute.push_str(&selected_path);
    if let Some(ref q) = selected_query {
        absolute.push('?');
        absolute.push_str(q);
    }
    if let Some(ref f) = fragment {
        absolute.push('#');
        absolute.push_str(f);
    }

    let mut url = parse_url(&absolute).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn main() {}