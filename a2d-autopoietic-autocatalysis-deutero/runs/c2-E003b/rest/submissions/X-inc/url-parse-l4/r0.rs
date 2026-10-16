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
            let hi = hex_value(bytes[i + 1]);
            let lo = hex_value(bytes[i + 2]);
            match (hi, lo) {
                (Some(h), Some(l)) => {
                    decoded.push((h << 4) | l);
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
        Err(e) => Err(CodecError::InvalidUtf8 {
            at: origins.get(e.valid_up_to()).copied().unwrap_or(input.len()),
        }),
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        let allowed = unreserved
            || match component {
                Component::UserInfo => b == b':',
                Component::Path => b == b'/',
                Component::Query => matches!(b, b'/' | b'?'),
                Component::Fragment => matches!(b, b'/' | b'?' | b'#'),
            };
        if allowed {
            out.push(b as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
        }
    }
    out
}

fn map_codec_parse(e: CodecError, offset: usize) -> ParseError {
    match e {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

fn validate_port(port: &str, start: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    if let Some((i, _)) = port.bytes().enumerate().find(|(_, b)| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: start + i });
    }
    let bytes = port.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }
    let mut value: u32 = 0;
    for b in bytes {
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

fn validate_component(input: &str, start: usize, component: Component) -> Result<(), ParseError> {
    decode_component(component, input).map(|_| ()).map_err(|e| map_codec_parse(e, start))
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    for (i, b) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at: i });
        }
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
    let rest = &input[auth_start..];
    let auth_len = rest
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(rest.len());
    let authority = &rest[..auth_len];
    let auth_end = auth_start + auth_len;

    let at_positions: Vec<usize> = authority
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| if b == b'@' { Some(i) } else { None })
        .collect();
    if at_positions.len() > 1 {
        return Err(ParseError::InvalidUserInfo {
            at: auth_start + at_positions[1],
        });
    }
    let (userinfo, hostport, hostport_start) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: auth_start });
        }
        (
            Some(authority[..at].to_owned()),
            &authority[at + 1..],
            auth_start + at + 1,
        )
    } else {
        (None, authority, auth_start)
    };

    let mut ipv6 = false;
    let (host, port) = if hostport.starts_with('[') {
        let close_rel = match hostport.as_bytes().iter().position(|&b| b == b']') {
            Some(i) => i,
            None => return Err(ParseError::InvalidIpv6 { at: hostport_start }),
        };
        let interior = &hostport[1..close_rel];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_start });
        }
        let suffix = &hostport[close_rel + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(validate_port(
                &suffix[1..],
                hostport_start + close_rel + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + close_rel + 1,
            });
        };
        ipv6 = true;
        (interior.to_ascii_lowercase(), port)
    } else {
        let split = hostport.find(':');
        let (hostpart, port) = match split {
            Some(i) => (
                &hostport[..i],
                Some(validate_port(&hostport[i + 1..], hostport_start + i + 1)?),
            ),
            None => (hostport, None),
        };
        if hostpart.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_start });
        }
        for (i, b) in hostpart.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_start + i,
                });
            }
        }
        (hostpart.to_ascii_lowercase(), port)
    };

    let tail = &input[auth_end..];
    let fragment_pos = tail.find('#');
    let before_fragment = fragment_pos.map(|i| &tail[..i]).unwrap_or(tail);
    let fragment = fragment_pos.map(|i| tail[i + 1..].to_owned());
    let query_pos = before_fragment.find('?');
    let path_and_query = query_pos
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment);
    let query = query_pos.map(|i| before_fragment[i + 1..].to_owned());
    let path = if path_and_query.is_empty() {
        "/".to_owned()
    } else {
        path_and_query.to_owned()
    };

    if let Some(ref ui) = userinfo {
        validate_component(ui, auth_start, Component::UserInfo)?;
    }
    if path != "/" || !path_and_query.is_empty() {
        let path_start = auth_end;
        validate_component(&path, path_start, Component::Path)?;
    }
    if let Some(ref q) = query {
        let q_start = auth_end + query_pos.unwrap() + 1;
        validate_component(q, q_start, Component::Query)?;
    }
    if let Some(ref f) = fragment {
        let f_start = auth_end + fragment_pos.unwrap() + 1;
        validate_component(f, f_start, Component::Fragment)?;
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

fn map_codec_path(e: CodecError) -> PathError {
    match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    }
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(map_codec_path)?;

    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    let mut terminal_navigation = false;

    for (i, raw) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, raw).map_err(map_codec_path)?;
        let dot = decoded == ".";
        let dotdot = decoded == "..";
        terminal_navigation = i + 1 == segments.len() && (dot || dotdot);
        if dot {
            continue;
        } else if dotdot {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Base(ParseError),
    Reference(ParseError),
    Path(PathError),
}

fn reference_global_scan(reference: &str) -> Result<(), ParseError> {
    for (at, b) in reference.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at });
        }
    }
    Ok(())
}

fn normalized_url(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}

fn split_reference(reference: &str) -> (String, Option<String>, Option<String>) {
    let frag_i = reference.find('#');
    let before_frag = frag_i.map(|i| &reference[..i]).unwrap_or(reference);
    let fragment = frag_i.map(|i| reference[i + 1..].to_owned());
    let query_i = before_frag.find('?');
    let path = query_i
        .map(|i| &before_frag[..i])
        .unwrap_or(before_frag)
        .to_owned();
    let query = query_i.map(|i| before_frag[i + 1..].to_owned());
    (path, query, fragment)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;
    reference_global_scan(reference).map_err(ResolveError::Reference)?;

    let first_delim = reference
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference[..first_delim].contains(':') {
        let url = parse_url(reference).map_err(ResolveError::Reference)?;
        return normalized_url(url);
    }
    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalized_url(url);
    }

    let (ref_path, ref_query, ref_fragment) = split_reference(reference);
    let (selected_path, selected_query) = if ref_path.is_empty() {
        (
            base_path,
            if ref_query.is_some() {
                ref_query
            } else {
                base_url.query.clone()
            },
        )
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path
        } else {
            let cut = base_path.rfind('/').map(|i| i + 1).unwrap_or(0);
            format!("{}{}", &base_path[..cut], ref_path)
        };
        (merged, ref_query)
    };

    let authority = format!(
        "{}://{}{}{}{}",
        base_url.scheme,
        base_url
            .userinfo
            .as_ref()
            .map(|u| format!("{}@", u))
            .unwrap_or_default(),
        if base_url.ipv6 {
            format!("[{}]", base_url.host)
        } else {
            base_url.host.clone()
        },
        base_url
            .port
            .map(|p| format!(":{}", p))
            .unwrap_or_default(),
        selected_path
    );
    let constructed = format!(
        "{}{}{}",
        authority,
        selected_query
            .as_ref()
            .map(|q| format!("?{}", q))
            .unwrap_or_default(),
        ref_fragment
            .as_ref()
            .map(|f| format!("#{}", f))
            .unwrap_or_default()
    );
    let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    normalized_url(url)
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

fn map_codec_query(e: CodecError, offset: usize) -> QueryError {
    match e {
        CodecError::BadEscape { at } => QueryError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: offset + at },
    }
}

fn plus_to_space(text: &str) -> String {
    text.replace('+', " ")
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut field_start = 0usize;

    for field in input.split('&') {
        let field_len = field.len();
        if !field.is_empty() {
            let eq = field.find('=');
            let key_raw = eq.map(|i| &field[..i]).unwrap_or(field);
            let value_raw = eq.map(|i| &field[i + 1..]).unwrap_or("");

            let key_prepared = plus_to_space(key_raw);
            let key = decode_component(Component::Query, &key_prepared)
                .map_err(|e| map_codec_query(e, field_start))?;
            if key.is_empty() {
                return Err(QueryError::EmptyKey { at: field_start });
            }

            let value_offset = field_start + eq.map(|i| i + 1).unwrap_or(field_len);
            let value_prepared = plus_to_space(value_raw);
            let value = decode_component(Component::Query, &value_prepared)
                .map_err(|e| map_codec_query(e, value_offset))?;

            if let Some((_, values)) = entries.iter_mut().find(|(k, _)| *k == key) {
                values.push(value);
            } else {
                entries.push((key, vec![value]));
            }
        }
        field_start += field_len + 1;
    }
    Ok(QueryMap { entries })
}

fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError {
    Parse(ParseError),
    Path(PathError),
    Query(QueryError),
}

pub fn canonical_url(input: &str) -> Result<String, CanonicalError> {
    let url = parse_url(input).map_err(CanonicalError::Parse)?;
    let path = normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query_map = match url.query.as_ref() {
        Some(q) => Some(parse_query(q).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = format!(
        "{}://{}",
        url.scheme,
        url.userinfo
            .as_ref()
            .map(|ui| {
                let decoded = decode_component(Component::UserInfo, ui).unwrap();
                format!("{}@", encode_component(Component::UserInfo, &decoded))
            })
            .unwrap_or_default()
    );

    if url.ipv6 {
        let addr = Ipv6Addr::from_str(&url.host).unwrap();
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

    let mut encoded_segments = Vec::new();
    for segment in path.split('/') {
        let decoded = decode_component(Component::Path, segment)
            .map_err(|e| match e {
                CodecError::BadEscape { at } => CanonicalError::Path(PathError::BadEscape { at }),
                CodecError::InvalidUtf8 { at } => {
                    CanonicalError::Path(PathError::InvalidUtf8 { at })
                }
            })?;
        encoded_segments.push(encode_unreserved_only(&decoded));
    }
    out.push_str(&encoded_segments.join("/"));

    if let Some(map) = query_map {
        out.push('?');
        out.push_str(&serialise_query(&map));
    }
    if let Some(fragment) = url.fragment {
        let decoded = decode_component(Component::Fragment, &fragment)
            .map_err(|e| match e {
                CodecError::BadEscape { at } => CanonicalError::Parse(ParseError::BadEscape { at }),
                CodecError::InvalidUtf8 { at } => {
                    CanonicalError::Parse(ParseError::InvalidUtf8 { at })
                }
            })?;
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }
    Ok(out)
}

fn encode_unreserved_only(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
        }
    }
    out
}