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

fn scan_ascii_printable(input: &str) -> Result<(), usize> {
    for (at, byte) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(at);
        }
    }
    Ok(())
}

fn codec_error_to_parse(error: CodecError, offset: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Err(at) = scan_ascii_printable(input) {
        return Err(ParseError::InvalidByte { at });
    }

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(index) => index,
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
    let auth_bytes = authority.as_bytes();

    let ats: Vec<usize> = auth_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();

    if ats.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + ats[1],
        });
    }
    if ats.len() == 1 && ats[0] == 0 {
        return Err(ParseError::InvalidUserInfo { at: authority_start });
    }

    let (userinfo, host_start_rel) = if let Some(&at) = ats.first() {
        (Some(authority[..at].to_string()), at + 1)
    } else {
        (None, 0)
    };

    let host_start = authority_start + host_start_rel;
    let host_port = &authority[host_start_rel..];
    let host_port_bytes = host_port.as_bytes();

    let (host_raw, port_raw, ipv6, port_colon_abs) = if host_port.starts_with('[') {
        let close = match host_port_bytes.iter().position(|&b| b == b']') {
            Some(index) => index,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };
        let interior = &host_port[1..close];
        if Ipv6Addr::from_str(interior).is_err() || interior.contains('%') {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }
        let suffix = &host_port[close + 1..];
        if suffix.is_empty() {
            (&host_port[1..close], None, true, None)
        } else if suffix.as_bytes()[0] == b':' {
            let port = &suffix[1..];
            (
                &host_port[1..close],
                Some(port),
                true,
                Some(host_start + close + 1),
            )
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close + 1,
            });
        }
    } else {
        let colon_rel = host_port_bytes.iter().position(|&b| b == b':');
        if let Some(index) = colon_rel {
            (
                &host_port[..index],
                Some(&host_port[index + 1..]),
                false,
                Some(host_start + index),
            )
        } else {
            (host_port, None, false, None)
        }
    };

    if host_raw.is_empty() {
        return Err(ParseError::InvalidHost { at: host_start });
    }
    if !ipv6 {
        let host_offset = host_start;
        for (i, byte) in host_raw.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_offset + i,
                });
            }
        }
    }

    let port = if let Some(port_text) = port_raw {
        let port_start = port_colon_abs.unwrap() + 1;
        if port_text.is_empty() {
            return Err(ParseError::InvalidPort { at: port_start });
        }
        for (i, byte) in port_text.bytes().enumerate() {
            if !byte.is_ascii_digit() {
                return Err(ParseError::InvalidPort {
                    at: port_start + i,
                });
            }
        }
        if port_text.len() >= 2 && port_text.as_bytes()[0] == b'0' {
            return Err(ParseError::InvalidPort { at: port_start });
        }
        let mut value: u32 = 0;
        for byte in port_text.bytes() {
            value = match value.checked_mul(10).and_then(|v| v.checked_add((byte - b'0') as u32)) {
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
        Some(value as u16)
    } else {
        None
    };

    let remainder = &input[authority_end..];
    let fragment_rel = remainder.find('#');
    let before_fragment = fragment_rel.map(|i| &remainder[..i]).unwrap_or(remainder);
    let fragment = fragment_rel.map(|i| remainder[i + 1..].to_string());

    let query_rel = before_fragment.find('?');
    let path_end_rel = query_rel.unwrap_or(before_fragment.len());
    let raw_path = &before_fragment[..path_end_rel];
    let query = query_rel.map(|i| before_fragment[i + 1..].to_string());
    let path = if raw_path.is_empty() {
        "/".to_string()
    } else {
        raw_path.to_string()
    };

    if let Some(ref raw_userinfo) = userinfo {
        decode_component(Component::UserInfo, raw_userinfo)
            .map_err(|error| codec_error_to_parse(error, authority_start))?;
    }
    if !raw_path.is_empty() {
        decode_component(Component::Path, raw_path)
            .map_err(|error| codec_error_to_parse(error, authority_end))?;
    }
    if let Some(query_value) = &query {
        let query_offset = authority_end + query_rel.unwrap() + 1;
        decode_component(Component::Query, query_value)
            .map_err(|error| codec_error_to_parse(error, query_offset))?;
    }
    if let Some(fragment_value) = &fragment {
        let fragment_offset = authority_end + fragment_rel.unwrap() + 1;
        decode_component(Component::Fragment, fragment_value)
            .map_err(|error| codec_error_to_parse(error, fragment_offset))?;
    }

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host: host_raw.to_ascii_lowercase(),
        port,
        path,
        query,
        fragment,
        ipv6,
    })
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
                (Some(hi), Some(lo)) => {
                    decoded.push((hi << 4) | lo);
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
        Ok(value) => Ok(value.to_string()),
        Err(error) => {
            let decoded_at = error.valid_up_to();
            let original_at = origins.get(decoded_at).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_at })
        }
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        let safe = byte.is_ascii_alphanumeric()
            || matches!(*byte, b'-' | b'.' | b'_' | b'~')
            || match component {
                Component::UserInfo => *byte == b':',
                Component::Path => *byte == b'/',
                Component::Query => matches!(*byte, b'/' | b'?'),
                Component::Fragment => matches!(*byte, b'/' | b'?' | b'#'),
            };
        if safe {
            encoded.push(*byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(|error| match error {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let mut stack: Vec<&str> = Vec::new();
    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut terminal_navigation = false;

    for (index, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(|error| match error {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 },
        })?;
        match decoded.as_str() {
            "." => {
                terminal_navigation = index + 1 == segments.len();
            }
            ".." => {
                if !stack.is_empty() {
                    stack.pop();
                }
                terminal_navigation = index + 1 == segments.len();
            }
            _ => {
                stack.push(segment);
                terminal_navigation = false;
            }
        }
    }

    if terminal_navigation {
        stack.push("");
    }

    if stack.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", stack.join("/")))
    }
}

fn map_path_result(url: &mut Url) -> Result<(), ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(())
}

fn split_reference(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let fragment_pos = reference.find('#');
    let before_fragment = fragment_pos.map(|i| &reference[..i]).unwrap_or(reference);
    let fragment = fragment_pos.map(|i| &reference[i + 1..]);
    let query_pos = before_fragment.find('?');
    let path_end = query_pos.unwrap_or(before_fragment.len());
    let query = query_pos.map(|i| &before_fragment[i + 1..]);
    (&before_fragment[..path_end], query, fragment)
}

fn raw_url_from_parts(url: &Url) -> String {
    let mut output = String::new();
    output.push_str(&url.scheme);
    output.push_str("://");
    if let Some(userinfo) = &url.userinfo {
        output.push_str(userinfo);
        output.push('@');
    }
    if url.ipv6 {
        output.push('[');
        output.push_str(&url.host);
        output.push(']');
    } else {
        output.push_str(&url.host);
    }
    if let Some(port) = url.port {
        output.push(':');
        output.push_str(&port.to_string());
    }
    output
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let mut base_url = parse_url(base).map_err(ResolveError::Base)?;
    map_path_result(&mut base_url)?;

    if let Err(at) = scan_ascii_printable(reference) {
        return Err(ResolveError::Reference(ParseError::InvalidByte { at }));
    }

    let first_delimiter = reference
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference[..first_delimiter].contains(':') {
        let mut url = parse_url(reference).map_err(ResolveError::Reference)?;
        map_path_result(&mut url)?;
        return Ok(url);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        map_path_result(&mut url)?;
        return Ok(url);
    }

    let (reference_path, reference_query, reference_fragment) = split_reference(reference);
    let selected_path;
    let selected_query;

    if reference_path.is_empty() {
        selected_path = base_url.path.clone();
        selected_query = if reference_query.is_none() {
            base_url.query.clone()
        } else {
            reference_query.map(str::to_string)
        };
    } else {
        selected_query = reference_query.map(str::to_string);
        if reference_path.starts_with('/') {
            selected_path = reference_path.to_string();
        } else {
            let base_dir_end = base_url.path.rfind('/').unwrap_or(0) + 1;
            selected_path = format!("{}{}", &base_url.path[..base_dir_end], reference_path);
        }
    }

    let mut constructed = raw_url_from_parts(&base_url);
    constructed.push_str(&selected_path);
    if let Some(query) = selected_query {
        constructed.push('?');
        constructed.push_str(&query);
    }
    if let Some(fragment) = reference_fragment {
        constructed.push('#');
        constructed.push_str(fragment);
    }

    let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    map_path_result(&mut url)?;
    Ok(url)
}