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

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn hex_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut offsets = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() || !is_hex(bytes[i + 1]) || !is_hex(bytes[i + 2]) {
                return Err(CodecError::BadEscape { at: i });
            }
            decoded.push((hex_value(bytes[i + 1]) << 4) | hex_value(bytes[i + 2]));
            offsets.push(i);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            offsets.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(s) => Ok(s.to_owned()),
        Err(err) => {
            let at = offsets.get(err.valid_up_to()).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at })
        }
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

fn scan_global_bytes(input: &str) -> Result<(), ParseError> {
    for (i, &b) in input.as_bytes().iter().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at: i });
        }
    }
    Ok(())
}

fn parse_port(raw: &str, start: usize) -> Result<u16, ParseError> {
    if raw.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    for (i, b) in raw.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: start + i });
        }
    }
    if raw.len() >= 2 && raw.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for b in raw.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(ParseError::InvalidPort { at: start })?;
        if value > 65535 {
            return Err(ParseError::InvalidPort { at: start });
        }
    }
    if value == 0 {
        return Err(ParseError::InvalidPort { at: start });
    }
    Ok(value as u16)
}

fn codec_to_parse(err: CodecError, offset: usize) -> ParseError {
    match err {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    scan_global_bytes(input)?;

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(i) => i,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };
    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (i, &b) in bytes[..colon].iter().enumerate().skip(1) {
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

    let after_marker = colon + 3;
    let auth_end = bytes[after_marker..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|n| after_marker + n)
        .unwrap_or(bytes.len());
    let authority = &input[after_marker..auth_end];

    let mut at_positions = authority.match_indices('@').map(|(i, _)| i);
    let first_at = at_positions.next();
    let second_at = at_positions.next();
    if let Some(second) = second_at {
        return Err(ParseError::InvalidUserInfo {
            at: after_marker + second,
        });
    }

    let (userinfo, hostport, hostport_start) = if let Some(at) = first_at {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: after_marker + at,
            });
        }
        (
            Some(authority[..at].to_owned()),
            &authority[at + 1..],
            after_marker + at + 1,
        )
    } else {
        (None, authority, after_marker)
    };

    let mut ipv6 = false;
    let (host, port) = if hostport.starts_with('[') {
        let opening = hostport_start;
        let closing = match hostport[1..].find(']') {
            Some(i) => i + 1,
            None => return Err(ParseError::InvalidIpv6 { at: opening }),
        };
        let interior = &hostport[1..closing];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: opening });
        }

        let suffix = &hostport[closing + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(
                &suffix[1..],
                hostport_start + closing + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + closing + 1,
            });
        };

        ipv6 = true;
        (interior.to_ascii_lowercase(), parsed_port)
    } else {
        let host_colon = hostport.find(':');
        let (raw_host, raw_port, port_start) = if let Some(i) = host_colon {
            (&hostport[..i], Some(&hostport[i + 1..]), hostport_start + i + 1)
        } else {
            (hostport, None, 0)
        };

        if raw_host.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_start });
        }
        for (i, b) in raw_host.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_start + i,
                });
            }
        }
        let parsed_port = match raw_port {
            Some(p) => Some(parse_port(p, port_start)?),
            None => None,
        };
        (raw_host.to_ascii_lowercase(), parsed_port)
    };

    let remainder = &input[auth_end..];
    let (before_fragment, fragment, fragment_start) = match remainder.find('#') {
        Some(i) => (
            &remainder[..i],
            Some(&remainder[i + 1..]),
            auth_end + i + 1,
        ),
        None => (remainder, None, 0),
    };

    let (path_and_query, query, query_start) = match before_fragment.find('?') {
        Some(i) => (
            &before_fragment[..i],
            Some(&before_fragment[i + 1..]),
            auth_end + i + 1,
        ),
        None => (before_fragment, None, 0),
    };

    let path = if path_and_query.is_empty() {
        "/".to_owned()
    } else {
        path_and_query.to_owned()
    };

    if let Some(ref raw_userinfo) = userinfo {
        let userinfo_start = after_marker;
        decode_component(Component::UserInfo, raw_userinfo)
            .map_err(|e| codec_to_parse(e, userinfo_start))?;
    }
    if !path_and_query.is_empty() {
        decode_component(Component::Path, path_and_query)
            .map_err(|e| codec_to_parse(e, auth_end))?;
    }
    if let Some(raw_query) = query {
        decode_component(Component::Query, raw_query)
            .map_err(|e| codec_to_parse(e, query_start))?;
    }
    if let Some(raw_fragment) = fragment {
        decode_component(Component::Fragment, raw_fragment)
            .map_err(|e| codec_to_parse(e, fragment_start))?;
    }

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host,
        port,
        path,
        query: query.map(str::to_owned),
        fragment: fragment.map(str::to_owned),
        ipv6,
    })
}

fn path_error_from_codec(err: CodecError) -> PathError {
    match err {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    }
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }

    decode_component(Component::Path, path).map_err(path_error_from_codec)?;

    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    let mut terminal_navigation = false;

    for (i, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment)
            .map_err(|e| path_error_from_codec(offset_codec_error(e, path, segment, i, &segments)))?;
        let is_dot = decoded == ".";
        let is_dotdot = decoded == "..";

        if is_dot || is_dotdot {
            if is_dotdot {
                stack.pop();
            }
            terminal_navigation = i + 1 == segments.len();
            if terminal_navigation {
                stack.push("");
            }
        } else {
            terminal_navigation = false;
            stack.push(segment);
        }
    }

    let _ = terminal_navigation;
    Ok(format!("/{}", stack.join("/")))
}

fn offset_codec_error(
    err: CodecError,
    path: &str,
    segment: &str,
    segment_index: usize,
    segments: &[&str],
) -> CodecError {
    let _ = (path, segment, segment_index, segments);
    err
}

fn reference_global_scan(input: &str) -> Result<(), ParseError> {
    scan_global_bytes(input)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let normalized_base_path =
        normalise_path(&base_url.path).map_err(ResolveError::Path)?;

    reference_global_scan(reference).map_err(ResolveError::Reference)?;

    let ref_bytes = reference.as_bytes();
    let first_structural = ref_bytes
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(ref_bytes.len());

    if ref_bytes[..first_structural].contains(&b':') {
        let mut url = parse_url(reference).map_err(ResolveError::Reference)?;
        url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    let (reference_before_fragment, fragment) = match reference.find('#') {
        Some(i) => (&reference[..i], Some(&reference[i + 1..])),
        None => (reference, None),
    };
    let (reference_path, reference_query) = match reference_before_fragment.find('?') {
        Some(i) => (
            &reference_before_fragment[..i],
            Some(&reference_before_fragment[i + 1..]),
        ),
        None => (reference_before_fragment, None),
    };

    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            normalized_base_path.as_str(),
            if reference_query.is_some() {
                reference_query
            } else {
                base_url.query.as_deref()
            },
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let base_dir_end = normalized_base_path
                .rfind('/')
                .map(|i| i + 1)
                .unwrap_or(0);
            format!("{}{}", &normalized_base_path[..base_dir_end], reference_path)
        };
        // Keep this owned path alive through URL construction below.
        let constructed = construct_absolute(
            &base_url,
            &merged,
            reference_query,
            fragment,
        );
        let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
        return Ok(url);
    };

    let constructed = construct_absolute(
        &base_url,
        selected_path,
        selected_query,
        fragment,
    );
    let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn construct_absolute(
    base: &Url,
    path: &str,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str(&base.scheme);
    out.push_str("://");
    if let Some(userinfo) = &base.userinfo {
        out.push_str(userinfo);
        out.push('@');
    }
    if base.ipv6 {
        out.push('[');
        out.push_str(&base.host);
        out.push(']');
    } else {
        out.push_str(&base.host);
    }
    if let Some(port) = base.port {
        out.push(':');
        out.push_str(&port.to_string());
    }
    out.push_str(path);
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}