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
                (Some(h), Some(l)) => {
                    decoded.push((h << 4) | l);
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
        Ok(s) => Ok(s.to_owned()),
        Err(e) => {
            let at = offsets.get(e.valid_up_to()).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at })
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
            out.push('%');
            out.push(char::from_digit((b >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            out.push(char::from_digit((b & 0x0f) as u32, 16).unwrap().to_ascii_uppercase());
        }
    }
    out
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some((i, _)) = input.bytes().enumerate().find(|(_, b)| !(0x21..=0x7e).contains(b)) {
        return Err(ParseError::InvalidByte { at: i });
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

    let auth_start = colon + 3;
    let auth_end = bytes[auth_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|n| auth_start + n)
        .unwrap_or(bytes.len());
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
    let (userinfo, host_start) = if let Some(&a) = at_positions.first() {
        if a == 0 {
            return Err(ParseError::InvalidUserInfo { at: auth_start });
        }
        (Some(authority[..a].to_owned()), a + 1)
    } else {
        (None, 0)
    };
    let hostport = &authority[host_start..];
    let host_abs = auth_start + host_start;

    let (host, port, ipv6) = if hostport.as_bytes().first() == Some(&b'[') {
        let close = hostport.as_bytes()[1..]
            .iter()
            .position(|&b| b == b']')
            .map(|n| n + 1)
            .ok_or(ParseError::InvalidIpv6 { at: host_abs })?;
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_abs });
        }
        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            Some(parse_port(&suffix[1..], host_abs + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_abs + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let split = hostport.bytes().position(|b| b == b':');
        let (h, p) = match split {
            Some(i) => (&hostport[..i], Some((&hostport[i + 1..], host_abs + i + 1))),
            None => (hostport, None),
        };
        if h.is_empty() {
            return Err(ParseError::InvalidHost { at: host_abs });
        }
        for (i, b) in h.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost { at: host_abs + i });
            }
        }
        let port = match p {
            Some((v, at)) => Some(parse_port(v, at)?),
            None => None,
        };
        (h.to_ascii_lowercase(), port, false)
    };

    let rest = &input[auth_end..];
    let hash_rel = rest.find('#');
    let pre_fragment = hash_rel.map(|i| &rest[..i]).unwrap_or(rest);
    let fragment = hash_rel.map(|i| rest[i + 1..].to_owned());

    let query_rel = pre_fragment.find('?');
    let path_raw = query_rel.map(|i| &pre_fragment[..i]).unwrap_or(pre_fragment);
    let query = query_rel.map(|i| pre_fragment[i + 1..].to_owned());
    let path = if path_raw.is_empty() {
        "/".to_owned()
    } else {
        path_raw.to_owned()
    };

    let mut component_start = auth_end;
    if let Some(ref u) = userinfo {
        let u_at = auth_start;
        validate_component(Component::UserInfo, u, u_at)?;
        component_start = auth_end;
    }
    if !path_raw.is_empty() {
        validate_component(Component::Path, path_raw, component_start)?;
    }
    if let Some(q) = &query {
        let q_at = auth_end + query_rel.unwrap() + 1;
        validate_component(Component::Query, q, q_at)?;
    }
    if let Some(f) = &fragment {
        let f_at = auth_end + hash_rel.unwrap() + 1;
        validate_component(Component::Fragment, f, f_at)?;
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

fn parse_port(text: &str, at: usize) -> Result<u16, ParseError> {
    if text.is_empty() {
        return Err(ParseError::InvalidPort { at });
    }
    for (i, b) in text.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: at + i });
        }
    }
    if text.len() >= 2 && text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at });
    }
    let mut value: u32 = 0;
    for b in text.bytes() {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as u32)) {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at }),
        };
    }
    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at });
    }
    Ok(value as u16)
}

fn validate_component(component: Component, text: &str, base: usize) -> Result<(), ParseError> {
    match decode_component(component, text) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: base + at }),
    }
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

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let raw_segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<String> = Vec::new();
    let mut terminal_nav = false;
    for (i, segment) in raw_segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(|e| match e {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 + segment_offset(&raw_segments, i) },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 + segment_offset(&raw_segments, i) },
        })?;
        if decoded == "." {
            terminal_nav = i + 1 == raw_segments.len();
        } else if decoded == ".." {
            stack.pop();
            terminal_nav = i + 1 == raw_segments.len();
        } else {
            stack.push((*segment).to_owned());
            terminal_nav = false;
        }
    }
    if terminal_nav {
        stack.push(String::new());
    }
    if stack.is_empty() {
        Ok("/".to_owned())
    } else {
        Ok(format!("/{}", stack.join("/")))
    }
}

fn segment_offset(segments: &[&str], index: usize) -> usize {
    segments.iter().take(index).map(|s| s.len() + 1).sum()
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

fn decode_query_piece(text: &str, at: usize) -> Result<String, QueryError> {
    let replaced = text.replace('+', " ");
    decode_component(Component::Query, &replaced).map_err(|e| match e {
        CodecError::BadEscape { at: i } => QueryError::BadEscape { at: at + i },
        CodecError::InvalidUtf8 { at: i } => QueryError::InvalidUtf8 { at: at + i },
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
        let eq = field.find('=');
        let (key_raw, value_raw, value_offset) = match eq {
            Some(i) => (&field[..i], &field[i + 1..], field_start + i + 1),
            None => (field, "", field_start + field.len()),
        };
        let key = decode_query_piece(key_raw, field_start)?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at: field_start });
        }
        let value = decode_query_piece(value_raw, value_offset)?;
        if let Some((_, values)) = entries.iter_mut().find(|(k, _)| *k == key) {
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
        if let Some((_, existing)) = merged.iter_mut().find(|(k, _)| k == key) {
            existing.extend(values.iter().cloned());
        } else {
            merged.push((key.clone(), values.clone()));
        }
    }

    let mut fields = Vec::new();
    for (key, values) in merged {
        let key = form_encode(&key);
        for value in values {
            fields.push(format!("{}={}", key, form_encode(&value)));
        }
    }
    fields.join("&")
}

fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if b == b' ' {
            out.push('+');
        } else if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(char::from_digit((b >> 4) as u32, 16).unwrap().to_ascii_uppercase());
            out.push(char::from_digit((b & 0x0f) as u32, 16).unwrap().to_ascii_uppercase());
        }
    }
    out
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
        Some(q) => Some(parse_query(q).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = format!("{}://", url.scheme);
    if let Some(raw) = &url.userinfo {
        let decoded = decode_component(Component::UserInfo, raw)
            .map_err(|e| CanonicalError::Parse(match e {
                CodecError::BadEscape { at } => ParseError::BadEscape { at },
                CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at },
            }))?;
        out.push_str(&encode_component(Component::UserInfo, &decoded));
        out.push('@');
    }
    if url.ipv6 {
        let addr = Ipv6Addr::from_str(&url.host).map_err(|_| CanonicalError::Parse(ParseError::InvalidIpv6 { at: 0 }))?;
        out.push('[');
        out.push_str(&addr.to_string());
        out.push(']');
    } else {
        out.push_str(&url.host);
    }
    if let Some(port) = url.port {
        let omit = (url.scheme == "http" && port == 80) || (url.scheme == "https" && port == 443);
        if !omit {
            out.push(':');
            out.push_str(&port.to_string());
        }
    }

    out.push_str(&canonical_path(&path).map_err(CanonicalError::Path)?);
    if let Some(map) = query_map {
        out.push('?');
        out.push_str(&serialise_query(&map));
    }
    if let Some(raw) = &url.fragment {
        let decoded = decode_component(Component::Fragment, raw)
            .map_err(|e| CanonicalError::Parse(match e {
                CodecError::BadEscape { at } => ParseError::BadEscape { at },
                CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at },
            }))?;
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }
    Ok(out)
}

fn canonical_path(path: &str) -> Result<String, PathError> {
    let mut pieces = Vec::new();
    for segment in path.split('/') {
        let decoded = decode_component(Component::Path, segment).map_err(|e| match e {
            CodecError::BadEscape { at } => PathError::BadEscape { at },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
        })?;
        let mut encoded = String::new();
        for &b in decoded.as_bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
                encoded.push(b as char);
            } else {
                encoded.push('%');
                encoded.push(char::from_digit((b >> 4) as u32, 16).unwrap().to_ascii_uppercase());
                encoded.push(char::from_digit((b & 0x0f) as u32, 16).unwrap().to_ascii_uppercase());
            }
        }
        pieces.push(encoded);
    }
    Ok(pieces.join("/"))
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let mut base_url = parse_url(base).map_err(ResolveError::Base)?;
    base_url.path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;

    if let Some((i, _)) = reference
        .bytes()
        .enumerate()
        .find(|(_, b)| !(0x21..=0x7e).contains(b))
    {
        return Err(ResolveError::Reference(ParseError::InvalidByte { at: i }));
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

    let (before_fragment, fragment) = match reference.find('#') {
        Some(i) => (&reference[..i], Some(reference[i + 1..].to_owned())),
        None => (reference, None),
    };
    let (ref_path, query) = match before_fragment.find('?') {
        Some(i) => (&before_fragment[..i], Some(before_fragment[i + 1..].to_owned())),
        None => (before_fragment, None),
    };

    if ref_path.is_empty() {
        let selected_query = if query.is_some() { query } else { base_url.query.clone() };
        let absolute = construct_url(&base_url, &base_url.path, selected_query.as_deref(), fragment.as_deref());
        let mut url = parse_url(&absolute).map_err(ResolveError::Reference)?;
        url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    let merged = if ref_path.starts_with('/') {
        ref_path.to_owned()
    } else {
        let slash = base_url.path.rfind('/').map(|i| &base_url.path[..=i]).unwrap_or("/");
        format!("{}{}", slash, ref_path)
    };
    let absolute = construct_url(&base_url, &merged, query.as_deref(), fragment.as_deref());
    let mut url = parse_url(&absolute).map_err(ResolveError::Reference)?;
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn construct_url(base: &Url, path: &str, query: Option<&str>, fragment: Option<&str>) -> String {
    let mut out = format!("{}://", base.scheme);
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
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out
}

fn main() {}