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
            let hi = hex_value(bytes[i + 1]);
            let lo = hex_value(bytes[i + 2]);
            match (hi, lo) {
                (Some(h), Some(l)) => {
                    decoded.push((h << 4) | l);
                    origins.push(i);
                    i += 3;
                }
                _ => return Err(CodecError::BadEscape { at: i }),
            }
        } else {
            decoded.push(bytes[i]);
            origins.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(s) => Ok(s.to_owned()),
        Err(e) => {
            let idx = e.valid_up_to();
            Err(CodecError::InvalidUtf8 {
                at: origins.get(idx).copied().unwrap_or(input.len()),
            })
        }
    }
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
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

fn validate_raw_bytes(input: &str) -> Result<(), ParseError> {
    for (i, b) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at: i });
        }
    }
    Ok(())
}

fn map_codec_error(e: CodecError, offset: usize) -> ParseError {
    match e {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    validate_raw_bytes(input)?;

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(i) => i,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for i in 1..colon {
        let b = bytes[i];
        if !(b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }

    if colon + 2 >= bytes.len()
        || bytes[colon + 1] != b'/'
        || bytes[colon + 2] != b'/'
    {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|p| authority_start + p)
        .unwrap_or(bytes.len());
    let authority = &input[authority_start..authority_end];

    let ats: Vec<usize> = authority
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| if b == b'@' { Some(i) } else { None })
        .collect();
    if ats.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + ats[1],
        });
    }
    let (userinfo, hostport_start) = if let Some(&at) = ats.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let hostport = &authority[hostport_start..];
    let host_start = authority_start + hostport_start;
    let (host, port, ipv6) = if hostport.starts_with('[') {
        let close = match hostport.as_bytes()[1..].iter().position(|&b| b == b']') {
            Some(i) => i + 1,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }
        let suffix = &hostport[close + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], host_start + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), parsed_port, true)
    } else {
        let host_colon = hostport.find(':');
        let (host_text, port_text, port_offset) = match host_colon {
            Some(i) => (
                &hostport[..i],
                Some(&hostport[i + 1..]),
                host_start + i + 1,
            ),
            None => (hostport, None, 0),
        };
        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (i, b) in host_text.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_start + i,
                });
            }
        }
        let parsed_port = match port_text {
            Some(p) => Some(parse_port(p, port_offset)?),
            None => None,
        };
        (host_text.to_ascii_lowercase(), parsed_port, false)
    };

    let fragment_pos = input[authority_end..]
        .find('#')
        .map(|p| authority_end + p);
    let before_fragment_end = fragment_pos.unwrap_or(input.len());
    let query_pos = input[authority_end..before_fragment_end]
        .find('?')
        .map(|p| authority_end + p);

    let path_end = query_pos.unwrap_or(before_fragment_end);
    let path = if authority_end == path_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };
    let query = query_pos.map(|q| input[q + 1..before_fragment_end].to_owned());
    let fragment = fragment_pos.map(|f| input[f + 1..].to_owned());

    if let Some(ref value) = userinfo {
        decode_component(Component::UserInfo, value)
            .map_err(|e| map_codec_error(e, authority_start))?;
    }
    if authority_end != path_end {
        decode_component(Component::Path, &path)
            .map_err(|e| map_codec_error(e, authority_end))?;
    }
    if let Some(ref value) = query {
        decode_component(Component::Query, value)
            .map_err(|e| map_codec_error(e, query_pos.unwrap() + 1))?;
    }
    if let Some(ref value) = fragment {
        decode_component(Component::Fragment, value)
            .map_err(|e| map_codec_error(e, fragment_pos.unwrap() + 1))?;
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

fn parse_port(port: &str, at: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at });
    }
    for (i, b) in port.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: at + i });
        }
    }
    if port.len() >= 2 && port.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at });
    }

    let mut value: u32 = 0;
    for b in port.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
        {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at }),
        };
    }
    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at });
    }
    Ok(value as u16)
}

fn path_codec_error(e: CodecError) -> PathError {
    match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    }
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(path_codec_error)?;

    let body = &path[1..];
    let segments: Vec<&str> = body.split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    let mut terminal_navigation = false;

    for (i, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(path_codec_error)?;
        let is_dot = decoded == ".";
        let is_dotdot = decoded == "..";
        if i + 1 == segments.len() {
            terminal_navigation = is_dot || is_dotdot;
        }
        if is_dot {
            continue;
        } else if is_dotdot {
            stack.pop();
        } else {
            stack.push(segment);
        }
    }

    if terminal_navigation {
        stack.push("");
    }
    let mut result = String::from("/");
    result.push_str(&stack.join("/"));
    Ok(result)
}

fn reference_global_scan(reference: &str) -> Result<(), ParseError> {
    validate_raw_bytes(reference)
}

fn normalize_url_path(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_url = normalize_url_path(base_url)?;

    reference_global_scan(reference)
        .map_err(ResolveError::Reference)?;

    let bytes = reference.as_bytes();
    let first_delim = bytes
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(bytes.len());
    if bytes[..first_delim].contains(&b':') {
        let absolute = parse_url(reference).map_err(ResolveError::Reference)?;
        return normalize_url_path(absolute);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let network = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalize_url_path(network);
    }

    let fragment_pos = reference.find('#');
    let before_fragment_end = fragment_pos.unwrap_or(reference.len());
    let query_pos = reference[..before_fragment_end].find('?');
    let path_end = query_pos.unwrap_or(before_fragment_end);
    let reference_path = &reference[..path_end];
    let reference_query = query_pos.map(|q| reference[q + 1..before_fragment_end].to_owned());
    let reference_fragment = fragment_pos.map(|f| reference[f + 1..].to_owned());

    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            base_url.path.clone(),
            if query_pos.is_none() {
                base_url.query.clone()
            } else {
                reference_query
            },
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = base_url.path.rfind('/').map(|i| i + 1).unwrap_or(0);
            format!("{}{}", &base_url.path[..slash], reference_path)
        };
        (merged, reference_query)
    };

    let mut constructed = format!("{}://", base_url.scheme);
    if let Some(ref userinfo) = base_url.userinfo {
        constructed.push_str(userinfo);
        constructed.push('@');
    }
    if base_url.ipv6 {
        constructed.push('[');
        constructed.push_str(&base_url.host);
        constructed.push(']');
    } else {
        constructed.push_str(&base_url.host);
    }
    if let Some(port) = base_url.port {
        constructed.push(':');
        constructed.push_str(&port.to_string());
    }
    constructed.push_str(&selected_path);
    if let Some(ref query) = selected_query {
        constructed.push('?');
        constructed.push_str(query);
    }
    if let Some(ref fragment) = reference_fragment {
        constructed.push('#');
        constructed.push_str(fragment);
    }

    let parsed = parse_url(&constructed).map_err(ResolveError::Reference)?;
    normalize_url_path(parsed)
}