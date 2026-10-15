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
            let high = hex_value(bytes[i + 1]);
            let low = hex_value(bytes[i + 2]);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(CodecError::BadEscape { at: i });
            };
            decoded.push((high << 4) | low);
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
        Err(error) => Err(CodecError::InvalidUtf8 {
            at: origins.get(error.valid_up_to()).copied().unwrap_or(input.len()),
        }),
    }
}

fn component_safe(component: Component, b: u8) -> bool {
    if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
        return true;
    }
    match component {
        Component::UserInfo => b == b':',
        Component::Path => b == b'/',
        Component::Query => matches!(b, b'/' | b'?'),
        Component::Fragment => matches!(b, b'/' | b'?' | b'#'),
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if component_safe(component, b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn invalid_raw_byte(input: &str) -> Option<usize> {
    input
        .as_bytes()
        .iter()
        .position(|&b| !(0x21..=0x7e).contains(&b))
}

fn validate_port(input: &str, start: usize) -> Result<Option<u16>, ParseError> {
    if input.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    if let Some(i) = input.bytes().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: start + i });
    }
    let bytes = input.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for &b in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u32))
        {
            Some(n) => n,
            None => return Err(ParseError::InvalidPort { at: start }),
        };
        if value > 65535 {
            return Err(ParseError::InvalidPort { at: start });
        }
    }
    if value == 0 {
        return Err(ParseError::InvalidPort { at: start });
    }
    Ok(Some(value as u16))
}

fn validate_decoded_component(
    component: Component,
    value: &str,
    start: usize,
) -> Result<(), ParseError> {
    match decode_component(component, value) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: start + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: start + at }),
    }
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(at) = invalid_raw_byte(input) {
        return Err(ParseError::InvalidByte { at });
    }

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

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|i| authority_start + i)
        .unwrap_or(bytes.len());
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
    let (userinfo, host_start_rel) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let host_start = authority_start + host_start_rel;
    let hostport = &authority[host_start_rel..];
    let (host, port, ipv6) = if hostport.as_bytes().first() == Some(&b'[') {
        let close = match hostport.as_bytes()[1..].iter().position(|&b| b == b']') {
            Some(i) => i + 1,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }
        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            validate_port(
                &suffix[1..],
                host_start + close + 2,
            )?
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let split = hostport.as_bytes().iter().position(|&b| b == b':');
        let (host_raw, port) = if let Some(i) = split {
            (&hostport[..i], validate_port(&hostport[i + 1..], host_start + i + 1)?)
        } else {
            (hostport, None)
        };
        if host_raw.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        if let Some(i) = host_raw
            .bytes()
            .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')))
        {
            return Err(ParseError::InvalidHost {
                at: host_start + i,
            });
        }
        (host_raw.to_ascii_lowercase(), port, false)
    };

    let fragment_pos = bytes[authority_end..]
        .iter()
        .position(|&b| b == b'#')
        .map(|i| authority_end + i);
    let before_fragment_end = fragment_pos.unwrap_or(bytes.len());
    let query_pos = bytes[authority_end..before_fragment_end]
        .iter()
        .position(|&b| b == b'?')
        .map(|i| authority_end + i);

    let path_end = query_pos.unwrap_or(before_fragment_end);
    let path = if path_end == authority_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };
    let query = query_pos.map(|i| input[i + 1..before_fragment_end].to_owned());
    let fragment = fragment_pos.map(|i| input[i + 1..].to_owned());

    if let Some(ref userinfo_value) = userinfo {
        validate_decoded_component(Component::UserInfo, userinfo_value, authority_start)?;
    }
    if path_end != authority_end {
        validate_decoded_component(Component::Path, &path, authority_end)?;
    }
    if let (Some(i), Some(ref q)) = (query_pos, query.as_ref()) {
        validate_decoded_component(Component::Query, q, i + 1)?;
    }
    if let (Some(i), Some(ref f)) = (fragment_pos, fragment.as_ref()) {
        validate_decoded_component(Component::Fragment, f, i + 1)?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Base(ParseError),
    Reference(ParseError),
    Path(PathError),
}

fn map_codec_path(error: CodecError) -> PathError {
    match error {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    }
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(map_codec_path)?;

    let raw_segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<String> = Vec::new();
    let mut terminal_navigation = false;

    for (i, segment) in raw_segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(map_codec_path)?;
        let is_dot = decoded == ".";
        let is_dotdot = decoded == "..";
        if i + 1 == raw_segments.len() {
            terminal_navigation = is_dot || is_dotdot;
        }
        if is_dot {
            continue;
        } else if is_dotdot {
            stack.pop();
        } else {
            stack.push((*segment).to_owned());
        }
    }

    if terminal_navigation {
        stack.push(String::new());
    }
    if stack.is_empty() {
        Ok("/".to_owned())
    } else {
        Ok(format!("/{}", stack.join("/")))
    }
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

fn plus_to_space(s: &str) -> String {
    s.replace('+', " ")
}

fn query_decode(s: &str, start: usize) -> Result<String, QueryError> {
    match decode_component(Component::Query, &plus_to_space(s)) {
        Ok(v) => Ok(v),
        Err(CodecError::BadEscape { at }) => Err(QueryError::BadEscape { at: start + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(QueryError::InvalidUtf8 { at: start + at }),
    }
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut field_start = 0;

    for field in input.split('&') {
        if !field.is_empty() {
            let eq = field.find('=');
            let (key_raw, value_raw, value_offset) = match eq {
                Some(i) => (&field[..i], &field[i + 1..], field_start + i + 1),
                None => (field, "", field_start + field.len()),
            };

            let key = query_decode(key_raw, field_start)?;
            if key.is_empty() {
                return Err(QueryError::EmptyKey { at: field_start });
            }
            let value = query_decode(value_raw, value_offset)?;

            if let Some((_, values)) = entries.iter_mut().find(|(existing, _)| *existing == key) {
                values.push(value);
            } else {
                entries.push((key, vec![value]));
            }
        }
        field_start += field.len() + 1;
    }

    Ok(QueryMap { entries })
}

fn form_encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if b == b' ' {
            out.push('+');
        } else if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
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
        if let Some((_, accumulated)) = merged.iter_mut().find(|(existing, _)| existing == key) {
            accumulated.extend(values.iter().cloned());
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
    let url = parse_url(input).map_err(CanonicalError::Parse)?;
    let normalized = normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query_map = match url.query.as_deref() {
        Some(query) => Some(parse_query(query).map_err(CanonicalError::Query)?),
        None => None,
    };

    let userinfo = match url.userinfo.as_deref() {
        Some(raw) => {
            let decoded = decode_component(Component::UserInfo, raw)
                .expect("parse_url already validated userinfo");
            Some(encode_component(Component::UserInfo, &decoded))
        }
        None => None,
    };

    let host = if url.ipv6 {
        let parsed = Ipv6Addr::from_str(&url.host).expect("parse_url already validated IPv6");
        format!("[{}]", parsed)
    } else {
        url.host.clone()
    };

    let mut out = format!("{}://", url.scheme);
    if let Some(userinfo) = userinfo {
        out.push_str(&userinfo);
        out.push('@');
    }
    out.push_str(&host);
    if let Some(port) = url.port {
        let omit = (url.scheme == "http" && port == 80) || (url.scheme == "https" && port == 443);
        if !omit {
            out.push(':');
            out.push_str(&port.to_string());
        }
    }

    let mut canonical_segments = Vec::new();
    for segment in normalized.split('/') {
        let decoded = decode_component(Component::Path, segment)
            .expect("normalise_path already validated the path");
        canonical_segments.push(encode_unreserved(&decoded));
    }
    out.push_str(&canonical_segments.join("/"));

    if let Some(map) = query_map {
        out.push('?');
        out.push_str(&serialise_query(&map));
    }
    if let Some(fragment) = url.fragment.as_deref() {
        let decoded = decode_component(Component::Fragment, fragment)
            .expect("parse_url already validated fragment");
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }
    Ok(out)
}

fn encode_unreserved(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn scan_reference(reference: &str) -> Result<(), ParseError> {
    if let Some(at) = invalid_raw_byte(reference) {
        Err(ParseError::InvalidByte { at })
    } else {
        Ok(())
    }
}

fn normalize_url_path(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn split_reference(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let bytes = reference.as_bytes();
    let fragment_pos = bytes.iter().position(|&b| b == b'#');
    let before_fragment = fragment_pos.unwrap_or(bytes.len());
    let query_pos = bytes[..before_fragment].iter().position(|&b| b == b'?');
    let path_end = query_pos.unwrap_or(before_fragment);
    let path = &reference[..path_end];
    let query = query_pos.map(|i| &reference[i + 1..before_fragment]);
    let fragment = fragment_pos.map(|i| &reference[i + 1..]);
    (path, query, fragment)
}

fn absolute_prefix(url: &Url) -> String {
    let mut s = format!("{}://", url.scheme);
    if let Some(userinfo) = &url.userinfo {
        s.push_str(userinfo);
        s.push('@');
    }
    if url.ipv6 {
        s.push('[');
        s.push_str(&url.host);
        s.push(']');
    } else {
        s.push_str(&url.host);
    }
    if let Some(port) = url.port {
        s.push(':');
        s.push_str(&port.to_string());
    }
    s
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let mut base_url = normalize_url_path(base_url).map_err(|e| e)?;

    scan_reference(reference).map_err(ResolveError::Reference)?;

    let first_delimiter = reference
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference[..first_delimiter].contains(':') {
        let url = parse_url(reference).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalize_url_path(url);
    }

    let (reference_path, reference_query, reference_fragment) = split_reference(reference);
    let (selected_path, selected_query) = if reference_path.is_empty() {
        let query = if reference_query.is_some() {
            reference_query.map(str::to_owned)
        } else {
            base_url.query.clone()
        };
        (base_url.path.clone(), query)
    } else {
        let merged_path = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = base_url.path.rfind('/').unwrap_or(0);
            format!("{}{}", &base_url.path[..=slash], reference_path)
        };
        (merged_path, reference_query.map(str::to_owned))
    };

    let mut constructed = absolute_prefix(&base_url);
    constructed.push_str(&selected_path);
    if let Some(query) = selected_query {
        constructed.push('?');
        constructed.push_str(&query);
    }
    if let Some(fragment) = reference_fragment {
        constructed.push('#');
        constructed.push_str(fragment);
    }
    let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    base_url = normalize_url_path(url).map_err(|e| e)?;
    Ok(base_url)
}

fn main() {}