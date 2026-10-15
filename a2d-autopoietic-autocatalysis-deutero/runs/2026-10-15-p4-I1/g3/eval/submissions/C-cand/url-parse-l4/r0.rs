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

fn decode_with_offsets(input: &str) -> Result<(String, Vec<usize>), CodecError> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut offsets = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() || !is_hex(bytes[i + 1]) || !is_hex(bytes[i + 2]) {
                return Err(CodecError::BadEscape { at: i });
            }
            out.push((hex_value(bytes[i + 1]) << 4) | hex_value(bytes[i + 2]));
            offsets.push(i);
            i += 3;
        } else {
            out.push(bytes[i]);
            offsets.push(i);
            i += 1;
        }
    }
    match std::str::from_utf8(&out) {
        Ok(s) => Ok((s.to_owned(), offsets)),
        Err(e) => Err(CodecError::InvalidUtf8 {
            at: offsets.get(e.valid_up_to()).copied().unwrap_or(input.len()),
        }),
    }
}

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    decode_with_offsets(input).map(|(s, _)| s)
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
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn invalid_global_byte(input: &str) -> Option<usize> {
    input
        .as_bytes()
        .iter()
        .position(|&b| !(0x21..=0x7e).contains(&b))
}

fn parse_port(port: &str, start: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    if let Some(i) = port.as_bytes().iter().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: start + i });
    }
    if port.len() >= 2 && port.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }
    let mut value: u32 = 0;
    for &b in port.as_bytes() {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as u32)) {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: start }),
        };
        if value > 65535 {
            return Err(ParseError::InvalidPort { at: start });
        }
    }
    if value == 0 {
        return Err(ParseError::InvalidPort { at: start });
    }
    Ok(value as u16)
}

fn validate_component(raw: &str, offset: usize) -> Result<(), ParseError> {
    decode_component(Component::Path, raw).map(|_| ()).map_err(|e| match e {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    })
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(at) = invalid_global_byte(input) {
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
    for i in 1..colon {
        if !(bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'+' | b'-' | b'.')) {
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
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|i| auth_start + i)
        .unwrap_or(bytes.len());
    let authority = &input[auth_start..auth_end];

    let at_positions: Vec<usize> = authority
        .as_bytes()
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();
    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: auth_start + at_positions[1],
        });
    }
    let (userinfo, host_start_local) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: auth_start });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };
    let host_start = auth_start + host_start_local;
    let host_port = &authority[host_start_local..];

    let (host, port, ipv6) = if host_port.starts_with('[') {
        let open_at = host_start;
        let close = match host_port.as_bytes().iter().position(|&b| b == b']') {
            Some(i) => i,
            None => return Err(ParseError::InvalidIpv6 { at: open_at }),
        };
        let interior = &host_port[1..close];
        let parsed = match Ipv6Addr::from_str(interior) {
            Ok(ip) if !interior.contains('%') => ip,
            _ => return Err(ParseError::InvalidIpv6 { at: open_at }),
        };
        let suffix = &host_port[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], host_start + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close + 1,
            });
        };
        let _ = parsed;
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let split = host_port.as_bytes().iter().position(|&b| b == b':');
        let (host_raw, port) = if let Some(i) = split {
            (&host_port[..i], Some(parse_port(&host_port[i + 1..], host_start + i + 1)?))
        } else {
            (host_port, None)
        };
        if host_raw.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (i, b) in host_raw.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_start + i,
                });
            }
        }
        (host_raw.to_ascii_lowercase(), port, false)
    };

    let after_auth = &input[auth_end..];
    let frag_rel = after_auth.find('#');
    let before_fragment = frag_rel.map(|i| &after_auth[..i]).unwrap_or(after_auth);
    let fragment = frag_rel.map(|i| after_auth[i + 1..].to_owned());
    let query_rel = before_fragment.find('?');
    let path_raw = query_rel.map(|i| &before_fragment[..i]).unwrap_or(before_fragment);
    let query = query_rel.map(|i| before_fragment[i + 1..].to_owned());
    let path = if path_raw.is_empty() {
        "/".to_owned()
    } else {
        path_raw.to_owned()
    };

    if let Some(ref u) = userinfo {
        validate_component(u, auth_start)?;
    }
    if !path_raw.is_empty() {
        validate_component(path_raw, auth_end)?;
    }
    if let Some(ref q) = query {
        let q_offset = auth_end + query_rel.unwrap() + 1;
        validate_component(q, q_offset)?;
    }
    if let Some(ref f) = fragment {
        let f_offset = auth_end + frag_rel.unwrap() + 1;
        validate_component(f, f_offset)?;
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
    let decoded = decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let raw_segments: Vec<&str> = path[1..].split('/').collect();
    let decoded_segments: Vec<&str> = decoded[1..].split('/').collect();
    let terminal_navigation = decoded_segments
        .last()
        .map(|s| *s == "." || *s == "..")
        .unwrap_or(false);
    let mut stack: Vec<&str> = Vec::new();

    for (raw, dec) in raw_segments.iter().zip(decoded_segments.iter()) {
        if *dec == "." {
            continue;
        }
        if *dec == ".." {
            stack.pop();
        } else {
            stack.push(raw);
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

fn rebase_codec(e: CodecError, offset: usize) -> ParseError {
    match e {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

fn parse_reference_parts(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let hash = reference.find('#');
    let before_fragment = hash.map(|i| &reference[..i]).unwrap_or(reference);
    let fragment = hash.map(|i| &reference[i + 1..]);
    let q = before_fragment.find('?');
    let path = q.map(|i| &before_fragment[..i]).unwrap_or(before_fragment);
    let query = q.map(|i| &before_fragment[i + 1..]);
    (path, query, fragment)
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
        let mut u = parse_url(reference).map_err(ResolveError::Reference)?;
        u.path = normalise_path(&u.path).map_err(ResolveError::Path)?;
        return Ok(u);
    }
    if reference.starts_with("//") {
        let absolute = format!("{}:{}", base_url.scheme, reference);
        let mut u = parse_url(&absolute).map_err(ResolveError::Reference)?;
        u.path = normalise_path(&u.path).map_err(ResolveError::Path)?;
        return Ok(u);
    }

    let (ref_path, ref_query, ref_fragment) = parse_reference_parts(reference);
    let (selected_path, selected_query) = if ref_path.is_empty() {
        (base_path.as_str().to_owned(), if ref_query.is_none() { base_url.query.as_deref() } else { ref_query }.map(str::to_owned))
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path.to_owned()
        } else {
            let slash = base_path.rfind('/').map(|i| &base_path[..=i]).unwrap_or("/");
            format!("{slash}{ref_path}")
        };
        (merged, ref_query.map(str::to_owned))
    };

    let mut raw = format!("{}://", base_url.scheme);
    if let Some(ref u) = base_url.userinfo {
        raw.push_str(u);
        raw.push('@');
    }
    if base_url.ipv6 {
        raw.push('[');
        raw.push_str(&base_url.host);
        raw.push(']');
    } else {
        raw.push_str(&base_url.host);
    }
    if let Some(port) = base_url.port {
        raw.push(':');
        raw.push_str(&port.to_string());
    }
    raw.push_str(&selected_path);
    if let Some(ref q) = selected_query {
        raw.push('?');
        raw.push_str(q);
    }
    if let Some(f) = ref_fragment {
        raw.push('#');
        raw.push_str(f);
    }
    let mut u = parse_url(&raw).map_err(ResolveError::Reference)?;
    u.path = normalise_path(&u.path).map_err(ResolveError::Path)?;
    Ok(u)
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut offset = 0usize;

    for field in input.split('&') {
        let field_start = offset;
        offset += field.len() + 1;
        if field.is_empty() {
            continue;
        }
        let eq = field.find('=');
        let (raw_key, raw_value) = match eq {
            Some(i) => (&field[..i], &field[i + 1..]),
            None => (field, ""),
        };
        let key_input = raw_key.replace('+', " ");
        let key = decode_component(Component::Query, &key_input).map_err(|e| match e {
            CodecError::BadEscape { at } => QueryError::BadEscape { at: field_start + at },
            CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: field_start + at },
        })?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at: field_start });
        }
        let value_start = field_start + raw_key.len() + usize::from(eq.is_some());
        let value_input = raw_value.replace('+', " ");
        let value = decode_component(Component::Query, &value_input).map_err(|e| match e {
            CodecError::BadEscape { at } => QueryError::BadEscape { at: value_start + at },
            CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: value_start + at },
        })?;

        if let Some((_, values)) = entries.iter_mut().find(|(k, _)| *k == key) {
            values.push(value);
        } else {
            entries.push((key, vec![value]));
        }
    }
    Ok(QueryMap { entries })
}

fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in text.as_bytes() {
        if b == b' ' {
            out.push('+');
        } else if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
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
        if let Some((_, existing)) = merged.iter_mut().find(|(k, _)| k == key) {
            existing.extend(values.iter().cloned());
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

pub fn canonical_url(input: &str) -> Result<String, CanonicalError> {
    let url = parse_url(input).map_err(CanonicalError::Parse)?;
    let path = normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query_map = match &url.query {
        Some(q) => Some(parse_query(q).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = format!("{}://", url.scheme);
    if let Some(userinfo) = &url.userinfo {
        let decoded =
            decode_component(Component::UserInfo, userinfo).map_err(|e| CanonicalError::Parse(rebase_codec(e, input.find(userinfo.as_str()).unwrap_or(0))))?;
        out.push_str(&encode_component(Component::UserInfo, &decoded));
        out.push('@');
    }
    if url.ipv6 {
        let ip = Ipv6Addr::from_str(&url.host).map_err(|_| {
            CanonicalError::Parse(ParseError::InvalidIpv6 { at: 0 })
        })?;
        out.push('[');
        out.push_str(&ip.to_string());
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
        let decoded = decode_component(Component::Path, segment).map_err(|e| {
            CanonicalError::Path(match e {
                CodecError::BadEscape { at } => PathError::BadEscape { at },
                CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
            })
        })?;
        let mut encoded = String::new();
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        for &b in decoded.as_bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
                encoded.push(b as char);
            } else {
                encoded.push('%');
                encoded.push(HEX[(b >> 4) as usize] as char);
                encoded.push(HEX[(b & 15) as usize] as char);
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
        let decoded = decode_component(Component::Fragment, fragment).map_err(|e| {
            let offset = input.rfind('#').map(|i| i + 1).unwrap_or(input.len());
            CanonicalError::Parse(rebase_codec(e, offset))
        })?;
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }
    Ok(out)
}

fn main() {}