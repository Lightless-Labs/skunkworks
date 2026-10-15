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

fn global_byte_scan(input: &str) -> Result<(), ParseError> {
    for (at, byte) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at });
        }
    }
    Ok(())
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
            Err(CodecError::InvalidUtf8 {
                at: offsets.get(decoded_at).copied().unwrap_or(input.len()),
            })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        let additionally_safe = match component {
            Component::UserInfo => byte == b':',
            Component::Path => byte == b'/',
            Component::Query => matches!(byte, b'/' | b'?'),
            Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
        };
        if unreserved || additionally_safe {
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

fn parse_port(port_text: &str, port_at: usize) -> Result<u16, ParseError> {
    if port_text.is_empty() {
        return Err(ParseError::InvalidPort { at: port_at });
    }

    for (offset, byte) in port_text.bytes().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: port_at + offset });
        }
    }

    if port_text.len() >= 2 && port_text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_at });
    }

    let mut value: u32 = 0;
    for byte in port_text.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|current| current.checked_add((byte - b'0') as u32))
        {
            Some(value) if value <= 65535 => value,
            _ => return Err(ParseError::InvalidPort { at: port_at }),
        };
    }

    if value == 0 {
        return Err(ParseError::InvalidPort { at: port_at });
    }
    Ok(value as u16)
}

fn parse_component_validation(
    component: Component,
    text: &str,
    original_at: usize,
) -> Result<(), ParseError> {
    match decode_component(component, text) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape {
            at: original_at + at,
        }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 {
            at: original_at + at,
        }),
    }
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    global_byte_scan(input)?;

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&byte| byte == b':') {
        Some(at) => at,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (at, byte) in bytes[..colon].iter().copied().enumerate().skip(1) {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at });
        }
    }

    if colon + 2 >= bytes.len() || bytes[colon + 1] != b'/' || bytes[colon + 2] != b'/' {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let scheme = input[..colon].to_ascii_lowercase();
    let after_marker = colon + 3;
    let authority_end = bytes[after_marker..]
        .iter()
        .position(|&byte| matches!(byte, b'/' | b'?' | b'#'))
        .map(|offset| after_marker + offset)
        .unwrap_or(bytes.len());
    let authority = &input[after_marker..authority_end];

    let mut at_signs = authority.match_indices('@');
    let first_at = at_signs.next().map(|(index, _)| index);
    let second_at = at_signs.next().map(|(index, _)| index);
    if let Some(index) = second_at {
        return Err(ParseError::InvalidUserInfo {
            at: after_marker + index,
        });
    }

    let (userinfo, host_port, host_port_at) = match first_at {
        Some(index) if index == 0 => {
            return Err(ParseError::InvalidUserInfo { at: after_marker });
        }
        Some(index) => (
            Some(authority[..index].to_owned()),
            &authority[index + 1..],
            after_marker + index + 1,
        ),
        None => (None, authority, after_marker),
    };

    let mut ipv6 = false;
    let (host, port) = if host_port.starts_with('[') {
        let close = match host_port.find(']') {
            Some(index) => index,
            None => {
                return Err(ParseError::InvalidIpv6 {
                    at: host_port_at,
                });
            }
        };
        let interior = &host_port[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 {
                at: host_port_at,
            });
        }
        let suffix = &host_port[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if let Some(port_text) = suffix.strip_prefix(':') {
            Some(parse_port(
                port_text,
                host_port_at + close + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_port_at + close + 1,
            });
        };
        ipv6 = true;
        (interior.to_ascii_lowercase(), port)
    } else {
        let host_colon = host_port.find(':');
        let (host_text, port) = match host_colon {
            Some(index) => {
                let port_text = &host_port[index + 1..];
                (
                    &host_port[..index],
                    Some(parse_port(port_text, host_port_at + index + 1)?),
                )
            }
            None => (host_port, None),
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: host_port_at });
        }
        for (offset, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_port_at + offset,
                });
            }
        }
        (host_text.to_ascii_lowercase(), port)
    };

    let remainder = &input[authority_end..];
    let fragment_relative = remainder.find('#');
    let before_fragment = fragment_relative
        .map(|index| &remainder[..index])
        .unwrap_or(remainder);
    let fragment = fragment_relative.map(|index| remainder[index + 1..].to_owned());
    let query_relative = before_fragment.find('?');
    let path_text = query_relative
        .map(|index| &before_fragment[..index])
        .unwrap_or(before_fragment);
    let query = query_relative.map(|index| before_fragment[index + 1..].to_owned());

    let path_at = authority_end;
    let query_at = query_relative.map(|index| authority_end + index + 1);
    let fragment_at = fragment_relative.map(|index| authority_end + index + 1);

    if let Some(userinfo_text) = userinfo.as_deref() {
        let userinfo_at = after_marker;
        parse_component_validation(Component::UserInfo, userinfo_text, userinfo_at)?;
    }

    let path = if path_text.is_empty() {
        "/".to_owned()
    } else {
        path_text.to_owned()
    };
    if !path_text.is_empty() {
        parse_component_validation(Component::Path, path_text, path_at)?;
    }
    if let Some(query_text) = query.as_deref() {
        parse_component_validation(
            Component::Query,
            query_text,
            query_at.unwrap_or(input.len()),
        )?;
    }
    if let Some(fragment_text) = fragment.as_deref() {
        parse_component_validation(
            Component::Fragment,
            fragment_text,
            fragment_at.unwrap_or(input.len()),
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

    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    let mut terminal_navigation = false;

    for (index, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(|error| match error {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 },
        })?;

        let is_last = index + 1 == segments.len();
        if decoded == "." {
            terminal_navigation = is_last;
        } else if decoded == ".." {
            if !stack.is_empty() {
                stack.pop();
            }
            terminal_navigation = is_last;
        } else {
            stack.push(segment);
            terminal_navigation = false;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Base(ParseError),
    Reference(ParseError),
    Path(PathError),
}

fn normalize_url_path(url: &mut Url) -> Result<(), PathError> {
    url.path = normalise_path(&url.path)?;
    Ok(())
}

fn component_parts(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let fragment_at = reference.find('#');
    let before_fragment = fragment_at.map(|i| &reference[..i]).unwrap_or(reference);
    let fragment = fragment_at.map(|i| &reference[i + 1..]);
    let query_at = before_fragment.find('?');
    let path = query_at
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment);
    let query = query_at.map(|i| &before_fragment[i + 1..]);
    (path, query, fragment)
}

fn build_absolute_url(url: &Url) -> String {
    let mut out = String::new();
    out.push_str(&url.scheme);
    out.push_str("://");
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
    out.push_str(&url.path);
    if let Some(query) = &url.query {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = &url.fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let mut base_url = parse_url(base).map_err(ResolveError::Base)?;
    normalize_url_path(&mut base_url).map_err(ResolveError::Path)?;
    global_byte_scan(reference).map_err(ResolveError::Reference)?;

    if let Some(delimiter) = reference.find(|c| matches!(c, '/' | '?' | '#')) {
        if reference[..delimiter].contains(':') {
            let mut url = parse_url(reference).map_err(ResolveError::Reference)?;
            normalize_url_path(&mut url).map_err(ResolveError::Path)?;
            return Ok(url);
        }
    } else if reference.contains(':') {
        let mut url = parse_url(reference).map_err(ResolveError::Reference)?;
        normalize_url_path(&mut url).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        normalize_url_path(&mut url).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    let (reference_path, reference_query, reference_fragment) = component_parts(reference);
    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            base_url.path.clone(),
            if reference_query.is_none() {
                base_url.query.clone()
            } else {
                reference_query.map(str::to_owned)
            },
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = base_url.path.rfind('/').map(|i| i + 1).unwrap_or(0);
            format!("{}{}", &base_url.path[..slash], reference_path)
        };
        (merged, reference_query.map(str::to_owned))
    };

    let mut constructed_url = base_url.clone();
    constructed_url.path = selected_path;
    constructed_url.query = selected_query;
    constructed_url.fragment = reference_fragment.map(str::to_owned);

    let constructed = build_absolute_url(&constructed_url);
    let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    normalize_url_path(&mut url).map_err(ResolveError::Path)?;
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

fn replace_plus(text: &str) -> String {
    text.replace('+', " ")
}

fn decode_query_field(text: &str, field_at: usize) -> Result<String, QueryError> {
    let replaced = replace_plus(text);
    decode_component(Component::Query, &replaced).map_err(|error| match error {
        CodecError::BadEscape { at } => QueryError::BadEscape {
            at: field_at + at,
        },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 {
            at: field_at + at,
        },
    })
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut field_start = 0;

    for field in input.split('&') {
        let this_start = field_start;
        field_start += field.len() + 1;

        if field.is_empty() {
            continue;
        }

        let equals = field.find('=');
        let (raw_key, raw_value, value_offset) = match equals {
            Some(index) => (&field[..index], &field[index + 1..], index + 1),
            None => (field, "", field.len()),
        };

        let key = decode_query_field(raw_key, this_start)?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at: this_start });
        }
        let value = decode_query_field(raw_value, this_start + value_offset)?;

        if let Some((_, values)) = entries.iter_mut().find(|(existing, _)| existing == &key) {
            values.push(value);
        } else {
            entries.push((key, vec![value]));
        }
    }

    Ok(QueryMap { entries })
}

fn form_encode(text: &str) -> String {
    let mut out = String::new();
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else if byte == b' ' {
            out.push('+');
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

pub fn serialise_query(map: &QueryMap) -> String {
    let mut merged: Vec<(String, Vec<String>)> = Vec::new();

    for (key, values) in &map.entries {
        if key.is_empty() || values.is_empty() {
            continue;
        }
        if let Some((_, existing_values)) =
            merged.iter_mut().find(|(existing_key, _)| existing_key == key)
        {
            existing_values.extend(values.iter().cloned());
        } else {
            merged.push((key.clone(), values.clone()));
        }
    }

    let mut fields = Vec::new();
    for (key, values) in merged {
        for value in values {
            fields.push(format!("{}={}", form_encode(&key), form_encode(&value)));
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
    let mut url = parse_url(input).map_err(CanonicalError::Parse)?;
    url.path = normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query_map = match url.query.as_deref() {
        Some(query) => Some(parse_query(query).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = format!("{}://", url.scheme);
    if let Some(raw_userinfo) = &url.userinfo {
        let decoded = decode_component(Component::UserInfo, raw_userinfo).map_err(|error| {
            let parse_error = match error {
                CodecError::BadEscape { at } => ParseError::BadEscape {
                    at: at + input.find(raw_userinfo.as_str()).unwrap_or(0),
                },
                CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 {
                    at: at + input.find(raw_userinfo.as_str()).unwrap_or(0),
                },
            };
            CanonicalError::Parse(parse_error)
        })?;
        out.push_str(&encode_component(Component::UserInfo, &decoded));
        out.push('@');
    }

    if url.ipv6 {
        let addr = Ipv6Addr::from_str(&url.host).map_err(|_| {
            CanonicalError::Parse(ParseError::InvalidIpv6 {
                at: input.find('[').unwrap_or(0),
            })
        })?;
        out.push('[');
        out.push_str(&addr.to_string());
        out.push(']');
    } else {
        out.push_str(&url.host);
    }

    let omit_port = matches!((url.scheme.as_str(), url.port), ("http", Some(80)) | ("https", Some(443)));
    if let Some(port) = url.port.filter(|_| !omit_port) {
        out.push(':');
        out.push_str(&port.to_string());
    }

    let segments: Vec<&str> = url.path.split('/').collect();
    let mut encoded_segments = Vec::with_capacity(segments.len());
    for segment in segments {
        let decoded = decode_component(Component::Path, segment).map_err(|error| {
            CanonicalError::Path(match error {
                CodecError::BadEscape { at } => PathError::BadEscape { at },
                CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
            })
        })?;
        encoded_segments.push(encode_component(Component::UserInfo, &decoded));
    }
    out.push_str(&encoded_segments.join("/"));

    if let Some(map) = query_map.as_ref() {
        out.push('?');
        out.push_str(&serialise_query(map));
    }

    if let Some(fragment) = url.fragment.as_deref() {
        let decoded = decode_component(Component::Fragment, fragment).map_err(|error| {
            let at = input.rfind('#').unwrap_or(input.len()) + 1;
            CanonicalError::Parse(match error {
                CodecError::BadEscape { at: offset } => ParseError::BadEscape { at: at + offset },
                CodecError::InvalidUtf8 { at: offset } => {
                    ParseError::InvalidUtf8 { at: at + offset }
                }
            })
        })?;
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }

    Ok(out)
}

fn main() {}