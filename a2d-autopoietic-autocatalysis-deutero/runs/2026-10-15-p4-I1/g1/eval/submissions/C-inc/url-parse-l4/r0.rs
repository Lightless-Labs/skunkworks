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

fn invalid_global_byte(input: &str) -> Option<usize> {
    input
        .as_bytes()
        .iter()
        .position(|b| !(0x21..=0x7e).contains(b))
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
            let Some(h) = hex_value(bytes[i + 1]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            let Some(l) = hex_value(bytes[i + 2]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            decoded.push((h << 4) | l);
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
        let common = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        let extra = match component {
            Component::UserInfo => b == b':',
            Component::Path => b == b'/',
            Component::Query => matches!(b, b'/' | b'?'),
            Component::Fragment => matches!(b, b'/' | b'?' | b'#'),
        };
        if common || extra {
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

fn parse_port(input: &str, start: usize) -> Result<u16, ParseError> {
    if input.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    if let Some(i) = input.bytes().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: start + i });
    }
    if input.len() >= 2 && input.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for b in input.bytes() {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as u32)) {
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
    raw: &str,
    offset: usize,
) -> Result<(), ParseError> {
    match decode_component(component, raw) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: offset + at }),
        Err(CodecError::InvalidUtf8 { at }) => {
            Err(ParseError::InvalidUtf8 { at: offset + at })
        }
    }
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(at) = invalid_global_byte(input) {
        return Err(ParseError::InvalidByte { at });
    }

    let bytes = input.as_bytes();
    let Some(colon) = bytes.iter().position(|&b| b == b':') else {
        return Err(ParseError::InvalidScheme { at: input.len() });
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
    let scheme = input[..colon].to_ascii_lowercase();

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let auth_start = colon + 3;
    let auth_end = bytes[auth_start..]
        .iter()
        .position(|b| matches!(*b, b'/' | b'?' | b'#'))
        .map(|i| auth_start + i)
        .unwrap_or(input.len());
    let authority = &input[auth_start..auth_end];

    let at_positions: Vec<usize> = authority
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| (b == b'@').then_some(i))
        .collect();
    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: auth_start + at_positions[1],
        });
    }

    let (userinfo, hostport_start) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: auth_start });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let hp = &authority[hostport_start..];
    let hp_abs = auth_start + hostport_start;
    let (host, port, ipv6) = if hp.starts_with('[') {
        let Some(close_rel) = hp[1..].find(']') else {
            return Err(ParseError::InvalidIpv6 { at: hp_abs });
        };
        let close = close_rel + 1;
        let interior = &hp[1..close];
        if Ipv6Addr::from_str(interior).is_err() || interior.contains('%') {
            return Err(ParseError::InvalidIpv6 { at: hp_abs });
        }
        let suffix = &hp[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], hp_abs + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hp_abs + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let (host_text, port_text) = match hp.find(':') {
            Some(i) => (&hp[..i], Some((&hp[i + 1..], hp_abs + i + 1))),
            None => (hp, None),
        };
        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: hp_abs });
        }
        if let Some(i) = host_text
            .bytes()
            .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')))
        {
            return Err(ParseError::InvalidHost { at: hp_abs + i });
        }
        let port = match port_text {
            Some((value, start)) => Some(parse_port(value, start)?),
            None => None,
        };
        (host_text.to_ascii_lowercase(), port, false)
    };

    let rest = &input[auth_end..];
    let fragment_pos = rest.find('#');
    let before_fragment = fragment_pos.map(|i| &rest[..i]).unwrap_or(rest);
    let fragment = fragment_pos.map(|i| rest[i + 1..].to_owned());
    let query_pos = before_fragment.find('?');
    let path_raw = query_pos
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment);
    let query = query_pos.map(|i| before_fragment[i + 1..].to_owned());
    let path = if path_raw.is_empty() {
        "/".to_owned()
    } else {
        path_raw.to_owned()
    };

    if let Some(ui) = &userinfo {
        validate_component(Component::UserInfo, ui, auth_start)?;
    }
    if !path_raw.is_empty() {
        validate_component(Component::Path, path_raw, auth_end)?;
    }
    if let Some(q) = &query {
        let q_offset = auth_end + query_pos.unwrap() + 1;
        validate_component(Component::Query, q, q_offset)?;
    }
    if let Some(f) = &fragment {
        let f_offset = auth_end + fragment_pos.unwrap() + 1;
        validate_component(Component::Fragment, f, f_offset)?;
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
    decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<String> = Vec::new();
    let mut terminal_navigation = false;

    for (i, segment) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(|e| match e {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 + segments[..i].iter().map(|s| s.len() + 1).sum::<usize>() },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 + segments[..i].iter().map(|s| s.len() + 1).sum::<usize>() },
        })?;
        if decoded == "." {
            terminal_navigation = i + 1 == segments.len();
        } else if decoded == ".." {
            stack.pop();
            terminal_navigation = i + 1 == segments.len();
        } else {
            stack.push((*segment).to_owned());
            terminal_navigation = false;
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

fn split_reference_components(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let frag = reference.find('#');
    let before_frag = frag.map(|i| &reference[..i]).unwrap_or(reference);
    let fragment = frag.map(|i| &reference[i + 1..]);
    let query_pos = before_frag.find('?');
    let path = query_pos
        .map(|i| &before_frag[..i])
        .unwrap_or(before_frag);
    let query = query_pos.map(|i| &before_frag[i + 1..]);
    (path, query, fragment)
}

fn serialize_authority(url: &Url) -> String {
    let mut out = String::new();
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
    if let Some(at) = invalid_global_byte(reference) {
        return Err(ResolveError::Reference(ParseError::InvalidByte { at }));
    }

    let first_delim = reference
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference[..first_delim].contains(':') {
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

    let (ref_path, ref_query, ref_fragment) = split_reference_components(reference);
    let (path, query) = if ref_path.is_empty() {
        (base_path.clone(), ref_query.map(str::to_owned).or_else(|| base_url.query.clone()))
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path.to_owned()
        } else {
            let slash = base_path.rfind('/').unwrap_or(0);
            format!("{}{}", &base_path[..=slash], ref_path)
        };
        (merged, ref_query.map(str::to_owned))
    };

    let mut constructed = format!("{}://", base_url.scheme);
    if let Some(userinfo) = &base_url.userinfo {
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
    constructed.push_str(&path);
    if let Some(query) = &query {
        constructed.push('?');
        constructed.push_str(query);
    }
    if let Some(fragment) = ref_fragment {
        constructed.push('#');
        constructed.push_str(fragment);
    }

    let mut url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if b == b' ' {
            out.push('+');
        } else if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
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

fn plus_to_space(input: &str) -> String {
    input.replace('+', " ")
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

        let (raw_key, raw_value) = match field.find('=') {
            Some(i) => (&field[..i], &field[i + 1..]),
            None => (field, ""),
        };
        let key_input = plus_to_space(raw_key);
        let key = decode_component(Component::Query, &key_input).map_err(|e| match e {
            CodecError::BadEscape { at } => QueryError::BadEscape {
                at: field_start + at,
            },
            CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 {
                at: field_start + at,
            },
        })?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at: field_start });
        }

        let value_input = plus_to_space(raw_value);
        let value_offset = field_start + raw_key.len() + usize::from(field.contains('='));
        let value = decode_component(Component::Query, &value_input).map_err(|e| match e {
            CodecError::BadEscape { at } => QueryError::BadEscape {
                at: value_offset + at,
            },
            CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 {
                at: value_offset + at,
            },
        })?;

        if let Some((_, values)) = entries.iter_mut().find(|(existing, _)| *existing == key) {
            values.push(value);
        } else {
            entries.push((key, vec![value]));
        }
    }

    Ok(QueryMap { entries })
}

pub fn serialise_query(map: &QueryMap) -> String {
    let mut merged: Vec<(String, Vec<String>)> = Vec::new();
    for (key, values) in &map.entries {
        if key.is_empty() || values.is_empty() {
            continue;
        }
        if let Some((_, target)) = merged.iter_mut().find(|(existing, _)| existing == key) {
            target.extend(values.iter().cloned());
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

fn encode_unreserved(text: &str) -> String {
    let mut out = String::new();
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
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
            decode_component(Component::UserInfo, userinfo).map_err(|e| match e {
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
        let addr = Ipv6Addr::from_str(&url.host).map_err(|_| {
            CanonicalError::Parse(ParseError::InvalidIpv6 { at: 0 })
        })?;
        out.push('[');
        out.push_str(&addr.to_string());
        out.push(']');
    } else {
        out.push_str(&url.host);
    }

    let keep_port = match (url.scheme.as_str(), url.port) {
        ("http", Some(80)) | ("https", Some(443)) => false,
        (_, Some(_)) => true,
        _ => false,
    };
    if keep_port {
        out.push(':');
        out.push_str(&url.port.unwrap().to_string());
    }

    let encoded_segments: Vec<String> = path
        .split('/')
        .map(encode_unreserved)
        .collect();
    out.push_str(&encoded_segments.join("/"));

    if let Some(map) = query_map {
        out.push('?');
        out.push_str(&serialise_query(&map));
    }
    if let Some(fragment) = &url.fragment {
        let decoded = decode_component(Component::Fragment, fragment).map_err(|e| match e {
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

fn main() {}