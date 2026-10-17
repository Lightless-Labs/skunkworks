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

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut offsets = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(CodecError::BadEscape { at: i });
            }
            let hi = hex_value(bytes[i + 1]);
            let lo = hex_value(bytes[i + 2]);
            match (hi, lo) {
                (Some(hi), Some(lo)) => {
                    decoded.push((hi << 4) | lo);
                    offsets.push(i);
                    i += 3;
                }
                _ => return Err(CodecError::BadEscape { at: i }),
            }
        } else {
            decoded.push(bytes[i]);
            offsets.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            let decoded_at = error.valid_up_to();
            let original_at = offsets.get(decoded_at).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_at })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        let safe = matches!(
            byte,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'
        ) || match component {
            Component::UserInfo => byte == b':',
            Component::Path => byte == b'/',
            Component::Query => matches!(byte, b'/' | b'?'),
            Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
        };

        if safe {
            output.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    output
}

fn scan_global_bytes(input: &str) -> Result<(), ParseError> {
    for (at, byte) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at });
        }
    }
    Ok(())
}

fn validate_component(
    component: Component,
    text: &str,
    base: usize,
) -> Result<(), ParseError> {
    match decode_component(component, text) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: base + at }),
    }
}

fn parse_port(port_text: &str, port_start: usize) -> Result<u16, ParseError> {
    if port_text.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    for (offset, byte) in port_text.bytes().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort {
                at: port_start + offset,
            });
        }
    }

    if port_text.len() >= 2 && port_text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for byte in port_text.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
        {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: port_start }),
        };
    }

    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    Ok(value as u16)
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    scan_global_bytes(input)?;

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&byte| byte == b':') {
        Some(position) => position,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (at, &byte) in bytes[..colon].iter().enumerate().skip(1) {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at });
        }
    }
    let scheme = input[..colon].to_ascii_lowercase();

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&byte| matches!(byte, b'/' | b'?' | b'#'))
        .map(|offset| authority_start + offset)
        .unwrap_or(input.len());
    let authority = &input[authority_start..authority_end];

    let at_positions: Vec<usize> = authority
        .bytes()
        .enumerate()
        .filter_map(|(offset, byte)| (byte == b'@').then_some(offset))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, hostport, hostport_start) = if let Some(&at) = at_positions.first() {
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
        let close = match hostport.as_bytes()[1..].iter().position(|&byte| byte == b']') {
            Some(offset) => offset + 1,
            None => {
                return Err(ParseError::InvalidIpv6 {
                    at: hostport_start,
                })
            }
        };
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 {
                at: hostport_start,
            });
        }
        ipv6 = true;
        let suffix = &hostport[close + 1..];
        let host = interior.to_ascii_lowercase();
        if suffix.is_empty() {
            (host, None, 0)
        } else if suffix.starts_with(':') {
            let start = hostport_start + close + 2;
            let port = parse_port(&suffix[1..], start)?;
            (host, Some(port), start)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + close + 1,
            });
        }
    } else {
        let split = hostport.find(':');
        let (host_text, port_text) = match split {
            Some(position) => (&hostport[..position], Some(&hostport[position + 1..])),
            None => (hostport, None),
        };
        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_start });
        }
        for (offset, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_start + offset,
                });
            }
        }
        let host = host_text.to_ascii_lowercase();
        match (split, port_text) {
            (Some(position), Some(text)) => {
                let start = hostport_start + position + 1;
                let port = parse_port(text, start)?;
                (host, Some(port), start)
            }
            _ => (host, None, 0),
        }
    };
    let _ = port_start;

    let fragment_mark = input[authority_end..]
        .find('#')
        .map(|offset| authority_end + offset);
    let before_fragment_end = fragment_mark.unwrap_or(input.len());
    let fragment = fragment_mark.map(|at| input[at + 1..].to_owned());

    let query_mark = input[authority_end..before_fragment_end]
        .find('?')
        .map(|offset| authority_end + offset);
    let path_end = query_mark.unwrap_or(before_fragment_end);
    let query = query_mark.map(|at| input[at + 1..before_fragment_end].to_owned());
    let path = if path_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };

    if let Some(user) = &userinfo {
        let user_at = authority_start;
        validate_component(Component::UserInfo, user, user_at)?;
    }
    if path_end != authority_end {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let Some(query_text) = &query {
        validate_component(
            Component::Query,
            query_text,
            query_mark.unwrap() + 1,
        )?;
    }
    if let Some(fragment_text) = &fragment {
        validate_component(
            Component::Fragment,
            fragment_text,
            fragment_mark.unwrap() + 1,
        )?;
    }

    Ok(Url {
        scheme,
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

    decode_component(Component::Path, path).map_err(|error| match error {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let raw_segments: Vec<&str> = path[1..].split('/').collect();
    let mut decoded_segments = Vec::with_capacity(raw_segments.len());
    for segment in &raw_segments {
        let decoded = decode_component(Component::Path, segment).map_err(|error| match error {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 },
        })?;
        decoded_segments.push(decoded);
    }

    let terminal_navigation = matches!(
        decoded_segments.last().map(String::as_str),
        Some(".") | Some("..")
    );
    let mut stack: Vec<&str> = Vec::new();

    for (raw, decoded) in raw_segments.iter().zip(decoded_segments.iter()) {
        match decoded.as_str() {
            "." => {}
            ".." => {
                stack.pop();
            }
            _ => stack.push(raw),
        }
    }

    if terminal_navigation {
        stack.push("");
    }

    if stack.is_empty() {
        Ok("/".to_owned())
    } else {
        Ok(format!("/{}", stack.join("/")))
    }
}

fn path_error_from_resolve(error: PathError) -> ResolveError {
    ResolveError::Path(error)
}

fn parse_and_normalize(input: &str) -> Result<Url, ResolveError> {
    let mut url = parse_url(input).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(path_error_from_resolve)?;
    Ok(url)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let normalized_base_path =
        normalise_path(&base_url.path).map_err(ResolveError::Path)?;
    scan_global_bytes(reference).map_err(ResolveError::Reference)?;

    let bytes = reference.as_bytes();
    let first_structural = bytes
        .iter()
        .position(|&byte| matches!(byte, b'/' | b'?' | b'#'))
        .unwrap_or(bytes.len());
    if bytes[..first_structural].contains(&b':') {
        return parse_and_normalize(reference);
    }

    if reference.starts_with("//") {
        let absolute = format!("{}:{}", base_url.scheme, reference);
        return parse_and_normalize(&absolute);
    }

    let fragment_mark = reference.find('#');
    let before_fragment_end = fragment_mark.unwrap_or(reference.len());
    let fragment = fragment_mark.map(|at| reference[at + 1..].to_owned());
    let query_mark = reference[..before_fragment_end].find('?');
    let path_end = query_mark.unwrap_or(before_fragment_end);
    let ref_path = &reference[..path_end];
    let ref_query = query_mark.map(|at| reference[at + 1..before_fragment_end].to_owned());

    let (selected_path, selected_query) = if ref_path.is_empty() {
        (
            normalized_base_path.clone(),
            if query_mark.is_some() {
                ref_query
            } else {
                base_url.query.clone()
            },
        )
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path.to_owned()
        } else {
            let slash = normalized_base_path.rfind('/').unwrap_or(0);
            format!("{}{}", &normalized_base_path[..=slash], ref_path)
        };
        (merged, ref_query)
    };

    let mut absolute = format!("{}://", base_url.scheme);
    if let Some(userinfo) = &base_url.userinfo {
        absolute.push_str(userinfo);
        absolute.push('@');
    }
    if base_url.ipv6 {
        absolute.push('[');
        absolute.push_str(&base_url.host);
        absolute.push(']');
    } else {
        absolute.push_str(&base_url.host);
    }
    if let Some(port) = base_url.port {
        absolute.push(':');
        absolute.push_str(&port.to_string());
    }
    absolute.push_str(&selected_path);
    if let Some(query) = &selected_query {
        absolute.push('?');
        absolute.push_str(query);
    }
    if let Some(fragment) = &fragment {
        absolute.push('#');
        absolute.push_str(fragment);
    }

    parse_and_normalize(&absolute)
}