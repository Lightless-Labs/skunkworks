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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError {
    Parse(ParseError),
    Path(PathError),
    Query(QueryError),
}

fn scan_bytes(input: &str) -> Result<(), ParseError> {
    for (at, byte) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at });
        }
    }
    Ok(())
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

fn decode_with_offsets(input: &str) -> Result<(String, Vec<usize>), CodecError> {
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
        Ok(text) => Ok((text.to_owned(), offsets)),
        Err(error) => {
            let at = offsets.get(error.valid_up_to()).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at })
        }
    }
}

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    decode_with_offsets(input).map(|(text, _)| text)
}

fn safe_byte(component: Component, byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(byte, b'-' | b'.' | b'_' | b'~')
        || match component {
            Component::UserInfo => byte == b':',
            Component::Path => byte == b'/',
            Component::Query => matches!(byte, b'/' | b'?'),
            Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
        }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in text.as_bytes() {
        if safe_byte(component, byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

fn parse_port(text: &str, start: usize) -> Result<u16, ParseError> {
    if text.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    if let Some(index) = text.bytes().position(|byte| !byte.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: start + index });
    }
    if text.len() >= 2 && text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
        {
            Some(value) => value,
            None => return Err(ParseError::InvalidPort { at: start }),
        };
    }
    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at: start });
    }
    Ok(value as u16)
}

fn map_codec_error(error: CodecError, offset: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

fn validate_component(text: &str, offset: usize) -> Result<(), ParseError> {
    decode_component(Component::Path, text).map(|_| ()).map_err(|e| map_codec_error(e, offset))
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    scan_bytes(input)?;

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(at) => at,
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
        .map(|i| authority_start + i)
        .unwrap_or(input.len());
    let authority = &input[authority_start..authority_end];

    let ats: Vec<usize> = authority
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| (b == b'@').then_some(i))
        .collect();
    if ats.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + ats[1],
        });
    }

    let (userinfo, host_start_in_authority) = if let Some(&at) = ats.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let host_port = &authority[host_start_in_authority..];
    let host_start = authority_start + host_start_in_authority;
    let (host, port, ipv6);

    if host_port.starts_with('[') {
        let close = match host_port.find(']') {
            Some(index) => index,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };
        let interior = &host_port[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }

        let suffix = &host_port[close + 1..];
        if suffix.is_empty() {
            port = None;
        } else if let Some(port_text) = suffix.strip_prefix(':') {
            port = Some(parse_port(port_text, host_start + close + 2)?);
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close + 1,
            });
        }
        host = interior.to_ascii_lowercase();
        ipv6 = true;
    } else {
        let host_colon = host_port.find(':');
        let host_text = host_colon.map(|i| &host_port[..i]).unwrap_or(host_port);
        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (i, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost { at: host_start + i });
            }
        }
        host = host_text.to_ascii_lowercase();
        ipv6 = false;
        port = if let Some(index) = host_colon {
            Some(parse_port(
                &host_port[index + 1..],
                host_start + index + 1,
            )?)
        } else {
            None
        };
    }

    let mut fragment = None;
    let before_fragment_end = if let Some(hash) = input[authority_end..].find('#') {
        let hash_at = authority_end + hash;
        fragment = Some(input[hash_at + 1..].to_owned());
        hash_at
    } else {
        input.len()
    };

    let mut query = None;
    let before_query_end = if let Some(question) = input[authority_end..before_fragment_end].find('?') {
        let query_at = authority_end + question;
        query = Some(input[query_at + 1..before_fragment_end].to_owned());
        query_at
    } else {
        before_fragment_end
    };

    let path = if authority_end == before_query_end {
        "/".to_owned()
    } else {
        input[authority_end..before_query_end].to_owned()
    };

    if let Some(userinfo) = &userinfo {
        validate_component(userinfo, authority_start)?;
    }
    if path != "/" {
        validate_component(&path, authority_end)?;
    }
    if let Some(query_text) = &query {
        validate_component(query_text, before_query_end + 1)?;
    }
    if let Some(fragment_text) = &fragment {
        let fragment_at = input[..before_fragment_end]
            .rfind('#')
            .unwrap_or(before_fragment_end);
        validate_component(fragment_text, fragment_at + 1)?;
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
    decode_component(Component::Path, path).map_err(|error| match error {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let segments: Vec<&str> = path.split('/').skip(1).collect();
    let mut stack: Vec<&str> = Vec::new();
    let mut terminal_navigation = false;

    for (index, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(|error| match error {
            CodecError::BadEscape { at } => PathError::BadEscape {
                at: path.find(segment).unwrap_or(0) + at,
            },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 {
                at: path.find(segment).unwrap_or(0) + at,
            },
        })?;
        let last = index + 1 == segments.len();
        if decoded == "." {
            terminal_navigation = last;
        } else if decoded == ".." {
            stack.pop();
            terminal_navigation = last;
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

fn split_reference_components(input: &str) -> (&str, Option<&str>, Option<&str>) {
    let fragment_at = input.find('#');
    let before_fragment = fragment_at.map(|i| &input[..i]).unwrap_or(input);
    let fragment = fragment_at.map(|i| &input[i + 1..]);

    let query_at = before_fragment.find('?');
    let path = query_at
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment);
    let query = query_at.map(|i| &before_fragment[i + 1..]);
    (path, query, fragment)
}

fn url_authority_prefix(url: &Url) -> String {
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
    out
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;
    scan_bytes(reference).map_err(ResolveError::Reference)?;

    let first_delimiter = reference
        .bytes()
        .position(|byte| matches!(byte, b'/' | b'?' | b'#'))
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

    let (reference_path, reference_query, reference_fragment) =
        split_reference_components(reference);
    let selected_path;
    let selected_query;

    if reference_path.is_empty() {
        selected_path = base_path.clone();
        selected_query = if reference_query.is_none() {
            base_url.query.as_deref()
        } else {
            reference_query
        };
    } else {
        selected_query = reference_query;
        if reference_path.starts_with('/') {
            selected_path = reference_path.to_owned();
        } else {
            let slash = base_path.rfind('/').unwrap_or(0);
            selected_path = format!("{}{}", &base_path[..=slash], reference_path);
        }
    }

    let mut constructed = url_authority_prefix(&base_url);
    constructed.push_str(&selected_path);
    if let Some(query) = selected_query {
        constructed.push('?');
        constructed.push_str(query);
    }
    if let Some(fragment) = reference_fragment {
        constructed.push('#');
        constructed.push_str(fragment);
    }

    let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn decode_form_part(input: &str, offset: usize) -> Result<String, QueryError> {
    let replaced = input.replace('+', " ");
    decode_component(Component::Query, &replaced).map_err(|error| match error {
        CodecError::BadEscape { at } => QueryError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: offset + at },
    })
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut field_start = 0;

    for field in input.split('&') {
        let current_start = field_start;
        field_start += field.len() + 1;
        if field.is_empty() {
            continue;
        }

        let equals = field.find('=');
        let (raw_key, raw_value, value_offset) = match equals {
            Some(index) => (&field[..index], &field[index + 1..], current_start + index + 1),
            None => (field, "", current_start + field.len()),
        };

        let key = decode_form_part(raw_key, current_start)?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at: current_start });
        }
        let value = decode_form_part(raw_value, value_offset)?;

        if let Some((_, values)) = entries.iter_mut().find(|(existing, _)| existing == &key) {
            values.push(value);
        } else {
            entries.push((key, vec![value]));
        }
    }

    Ok(QueryMap { entries })
}

fn encode_form_part(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if byte == b' ' {
            out.push('+');
        } else if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
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
        if let Some((_, target_values)) = merged.iter_mut().find(|(existing, _)| existing == key) {
            target_values.extend(values.iter().cloned());
        } else {
            merged.push((key.clone(), values.clone()));
        }
    }

    let mut fields = Vec::new();
    for (key, values) in merged {
        for value in values {
            fields.push(format!("{}={}", encode_form_part(&key), encode_form_part(&value)));
        }
    }
    fields.join("&")
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
        let decoded =
            decode_component(Component::UserInfo, userinfo).expect("parse_url validated userinfo");
        out.push_str(&encode_component(Component::UserInfo, &decoded));
        out.push('@');
    }

    if url.ipv6 {
        let address = Ipv6Addr::from_str(&url.host).expect("parse_url validated IPv6 host");
        out.push('[');
        out.push_str(&address.to_string());
        out.push(']');
    } else {
        out.push_str(&url.host);
    }

    if let Some(port) = url.port {
        if !((url.scheme == "http" && port == 80) || (url.scheme == "https" && port == 443)) {
            out.push(':');
            out.push_str(&port.to_string());
        }
    }

    let mut canonical_segments = Vec::new();
    for segment in path.split('/') {
        let decoded = decode_component(Component::Path, segment)
            .expect("normalise_path validated path");
        canonical_segments.push(encode_component(Component::UserInfo, &decoded));
    }
    out.push_str(&canonical_segments.join("/"));

    if let Some(query) = query_map {
        out.push('?');
        out.push_str(&serialise_query(&query));
    }
    if let Some(fragment) = &url.fragment {
        let decoded =
            decode_component(Component::Fragment, fragment).expect("parse_url validated fragment");
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }

    Ok(out)
}