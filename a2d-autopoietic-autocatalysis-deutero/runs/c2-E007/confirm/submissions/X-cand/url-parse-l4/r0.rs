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
    let mut origins = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(CodecError::BadEscape { at: i });
            }
            match (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
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
        Ok(s) => Ok(s.to_owned()),
        Err(error) => {
            let decoded_at = error.valid_up_to();
            Err(CodecError::InvalidUtf8 {
                at: origins.get(decoded_at).copied().unwrap_or(input.len()),
            })
        }
    }
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        let safe = is_unreserved(byte)
            || match component {
                Component::UserInfo => byte == b':',
                Component::Path => byte == b'/',
                Component::Query => matches!(byte, b'/' | b'?'),
                Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
            };
        if safe {
            out.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

fn global_byte_error(input: &str) -> Option<ParseError> {
    input
        .as_bytes()
        .iter()
        .position(|&b| !(0x21..=0x7e).contains(&b))
        .map(|at| ParseError::InvalidByte { at })
}

fn parse_port(input: &str, start: usize) -> Result<u16, ParseError> {
    if input.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    if let Some(offset) = input.bytes().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: start + offset });
    }
    if input.len() >= 2 && input.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for byte in input.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
        {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: start }),
        };
    }
    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at: start });
    }
    Ok(value as u16)
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

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(error) = global_byte_error(input) {
        return Err(error);
    }

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(pos) => pos,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };
    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (i, &byte) in bytes[..colon].iter().enumerate().skip(1) {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
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

    let at_positions: Vec<usize> = authority
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| (b == b'@').then_some(i))
        .collect();
    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }
    let (userinfo, hostport, hostport_start) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
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

    let (host, port, ipv6) = if hostport.as_bytes().first() == Some(&b'[') {
        let close = hostport.as_bytes()[1..]
            .iter()
            .position(|&b| b == b']')
            .map(|i| i + 1);
        let close = match close {
            Some(i) => i,
            None => return Err(ParseError::InvalidIpv6 { at: hostport_start }),
        };
        let interior = &hostport[1..close];
        if interior.contains('%') || Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_start });
        }
        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            Some(parse_port(
                &suffix[1..],
                hostport_start + close + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let split = hostport.as_bytes().iter().position(|&b| b == b':');
        let (host_text, port_text, port_start) = if let Some(i) = split {
            (
                &hostport[..i],
                Some(&hostport[i + 1..]),
                hostport_start + i + 1,
            )
        } else {
            (hostport, None, 0)
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_start });
        }
        for (i, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_start + i,
                });
            }
        }

        let port = match port_text {
            Some(text) => Some(parse_port(text, port_start)?),
            None => None,
        };
        (host_text.to_ascii_lowercase(), port, false)
    };

    let fragment_pos = input[authority_end..]
        .bytes()
        .position(|b| b == b'#')
        .map(|i| authority_end + i);
    let component_end = fragment_pos.unwrap_or(input.len());
    let query_pos = input[authority_end..component_end]
        .bytes()
        .position(|b| b == b'?')
        .map(|i| authority_end + i);

    let path_end = query_pos.unwrap_or(component_end);
    let path_raw = &input[authority_end..path_end];
    let path = if path_raw.is_empty() {
        "/".to_owned()
    } else {
        path_raw.to_owned()
    };
    let query = query_pos.map(|q| input[q + 1..component_end].to_owned());
    let fragment = fragment_pos.map(|f| input[f + 1..].to_owned());

    if let Some(ref value) = userinfo {
        validate_component(Component::UserInfo, value, authority_start)?;
    }
    if !path_raw.is_empty() {
        validate_component(Component::Path, path_raw, authority_end)?;
    }
    if let Some(q) = query_pos {
        validate_component(
            Component::Query,
            query.as_deref().unwrap_or(""),
            q + 1,
        )?;
    }
    if let Some(f) = fragment_pos {
        validate_component(
            Component::Fragment,
            fragment.as_deref().unwrap_or(""),
            f + 1,
        )?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    NotAbsolute,
    BadEscape { at: usize },
    InvalidUtf8 { at: usize },
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
    let mut stack: Vec<&str> = Vec::new();
    let mut terminal_navigation = false;

    for (i, raw) in raw_segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, raw).map_err(|error| match error {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 },
        })?;
        let is_terminal = i + 1 == raw_segments.len();
        if decoded == "." {
            terminal_navigation = is_terminal;
        } else if decoded == ".." {
            stack.pop();
            terminal_navigation = is_terminal;
        } else {
            stack.push(raw);
            terminal_navigation = false;
        }
    }

    if terminal_navigation {
        stack.push("");
    }

    let mut result = String::from("/");
    result.push_str(&stack.join("/"));
    Ok(result)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Base(ParseError),
    Reference(ParseError),
    Path(PathError),
}

fn split_reference(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let fragment_pos = reference.find('#');
    let before_fragment = fragment_pos.map(|i| &reference[..i]).unwrap_or(reference);
    let fragment = fragment_pos.map(|i| &reference[i + 1..]);
    let query_pos = before_fragment.find('?');
    let path = query_pos
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment);
    let query = query_pos.map(|i| &before_fragment[i + 1..]);
    (path, query, fragment)
}

fn build_absolute(url: &Url, path: &str, query: Option<&str>, fragment: Option<&str>) -> String {
    let mut out = format!("{}://", url.scheme);
    if let Some(userinfo) = &url.userinfo {
        out.push_str(userinfo);
        out.push('@');
    }
    if url.ipv6 {
        out.push('[');
        out.push_str(&url.host);
        out.push(']');
    } else {
        out.push_str(&url.host);
    }
    if let Some(port) = url.port {
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

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;

    if let Some(error) = global_byte_error(reference) {
        return Err(ResolveError::Reference(error));
    }

    let first_delimiter = reference
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference[..first_delimiter].contains(':') {
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

    let (reference_path, reference_query, reference_fragment) = split_reference(reference);
    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            base_path.clone(),
            reference_query.or(base_url.query.as_deref()),
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = base_path.rfind('/').unwrap_or(0);
            format!("{}{}", &base_path[..=slash], reference_path)
        };
        (merged, reference_query)
    };

    let constructed = build_absolute(
        &base_url,
        &selected_path,
        selected_query,
        reference_fragment,
    );
    let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryMap {
    pub entries: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    BadEscape { at: usize },
    InvalidUtf8 { at: usize },
    EmptyKey { at: usize },
}

fn decode_form_part(input: &str, base: usize) -> Result<String, QueryError> {
    let replaced = input.replace('+', " ");
    decode_component(Component::Query, &replaced).map_err(|error| match error {
        CodecError::BadEscape { at } => QueryError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: base + at },
    })
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut offset = 0;

    for field in input.split('&') {
        let field_start = offset;
        offset += field.len() + 1;
        if field.is_empty() {
            continue;
        }

        let equals = field.find('=');
        let key_raw = equals.map(|i| &field[..i]).unwrap_or(field);
        let value_raw = equals.map(|i| &field[i + 1..]).unwrap_or("");
        let key = decode_form_part(key_raw, field_start)?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at: field_start });
        }
        let value_base = field_start + equals.map(|i| i + 1).unwrap_or(field.len());
        let value = decode_form_part(value_raw, value_base)?;

        if let Some((_, values)) = entries.iter_mut().find(|(existing, _)| *existing == key) {
            values.push(value);
        } else {
            entries.push((key, vec![value]));
        }
    }

    Ok(QueryMap { entries })
}

fn encode_form_part(input: &str) -> String {
    let mut out = String::new();
    for &byte in input.as_bytes() {
        if byte == b' ' {
            out.push('+');
        } else if is_unreserved(byte) {
            out.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

pub fn serialise_query(map: &QueryMap) -> String {
    let mut merged: Vec<(&str, Vec<&str>)> = Vec::new();
    for (key, values) in &map.entries {
        if key.is_empty() || values.is_empty() {
            continue;
        }
        if let Some((_, combined)) = merged.iter_mut().find(|(existing, _)| *existing == key) {
            combined.extend(values.iter().map(String::as_str));
        } else {
            merged.push((key, values.iter().map(String::as_str).collect()));
        }
    }

    let mut fields = Vec::new();
    for (key, values) in merged {
        let encoded_key = encode_form_part(key);
        for value in values {
            fields.push(format!("{}={}", encoded_key, encode_form_part(value)));
        }
    }
    fields.join("&")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError {
    Parse(ParseError),
    Path(PathError),
    Query(QueryError),
}

pub fn canonical_url(input: &str) -> Result<String, CanonicalError> {
    let url = parse_url(input).map_err(CanonicalError::Parse)?;
    let path = normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query_map = match &url.query {
        Some(query) => Some(parse_query(query).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = format!("{}://", url.scheme);
    if let Some(userinfo) = &url.userinfo {
        let decoded = decode_component(Component::UserInfo, userinfo)
            .map_err(|error| match error {
                CodecError::BadEscape { at } => {
                    CanonicalError::Parse(ParseError::BadEscape { at })
                }
                CodecError::InvalidUtf8 { at } => {
                    CanonicalError::Parse(ParseError::InvalidUtf8 { at })
                }
            })?;
        out.push_str(&encode_component(Component::UserInfo, &decoded));
        out.push('@');
    }

    if url.ipv6 {
        let address = Ipv6Addr::from_str(&url.host).expect("validated IPv6 host");
        out.push('[');
        out.push_str(&address.to_string());
        out.push(']');
    } else {
        out.push_str(&url.host);
    }

    if let Some(port) = url.port {
        let omit = (url.scheme == "http" && port == 80)
            || (url.scheme == "https" && port == 443);
        if !omit {
            out.push(':');
            out.push_str(&port.to_string());
        }
    }

    let mut canonical_segments = Vec::new();
    for segment in path.split('/') {
        let decoded = decode_component(Component::Path, segment)
            .map_err(|error| match error {
                CodecError::BadEscape { at } => {
                    CanonicalError::Path(PathError::BadEscape { at })
                }
                CodecError::InvalidUtf8 { at } => {
                    CanonicalError::Path(PathError::InvalidUtf8 { at })
                }
            })?;
        let mut encoded = String::new();
        for &byte in decoded.as_bytes() {
            if is_unreserved(byte) {
                encoded.push(byte as char);
            } else {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
        canonical_segments.push(encoded);
    }
    out.push_str(&canonical_segments.join("/"));

    if let Some(map) = query_map {
        out.push('?');
        out.push_str(&serialise_query(&map));
    }
    if let Some(fragment) = &url.fragment {
        let decoded = decode_component(Component::Fragment, fragment)
            .map_err(|error| match error {
                CodecError::BadEscape { at } => {
                    CanonicalError::Parse(ParseError::BadEscape { at })
                }
                CodecError::InvalidUtf8 { at } => {
                    CanonicalError::Parse(ParseError::InvalidUtf8 { at })
                }
            })?;
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }

    Ok(out)
}