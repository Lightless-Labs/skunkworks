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
        Ok(s) => Ok(s.to_owned()),
        Err(err) => {
            let decoded_at = err.valid_up_to();
            let original_at = offsets.get(decoded_at).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_at })
        }
    }
}

fn component_safe(component: Component, b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~')
        || match component {
            Component::UserInfo => b == b':',
            Component::Path => b == b'/',
            Component::Query => matches!(b, b'/' | b'?'),
            Component::Fragment => matches!(b, b'/' | b'?' | b'#'),
        }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
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

fn parse_port(port: &str, port_start: usize) -> Result<Option<u16>, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    for (i, b) in port.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidPort {
                at: port_start + i,
            });
        }
    }

    let bytes = port.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for &b in bytes {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as u32)) {
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
    Ok(Some(value as u16))
}

fn validate_component(raw: &str, component: Component, start: usize) -> Result<(), ParseError> {
    match decode_component(component, raw) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: start + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: start + at }),
    }
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
        .map(|n| authority_start + n)
        .unwrap_or(input.len());
    let authority = &input[authority_start..authority_end];

    let mut at_positions = authority
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| (b == b'@').then_some(i));
    let first_at = at_positions.next();
    if let Some(second) = at_positions.next() {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + second,
        });
    }

    let (userinfo, hostport_start) = match first_at {
        Some(pos) if pos == 0 => {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + pos,
            });
        }
        Some(pos) => (Some(&authority[..pos]), pos + 1),
        None => (None, 0),
    };

    let hostport = &authority[hostport_start..];
    let hostport_abs = authority_start + hostport_start;
    let (host, port, ipv6) = if hostport.starts_with('[') {
        let close = match hostport.as_bytes()[1..].iter().position(|&b| b == b']') {
            Some(n) => n + 1,
            None => return Err(ParseError::InvalidIpv6 { at: hostport_abs }),
        };
        let interior = &hostport[1..close];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_abs });
        }
        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            parse_port(&suffix[1..], hostport_abs + close + 2)?
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_abs + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), port, true)
    } else {
        let host_colon = hostport.bytes().position(|b| b == b':');
        let (raw_host, port) = match host_colon {
            Some(pos) => (
                &hostport[..pos],
                parse_port(&hostport[pos + 1..], hostport_abs + pos + 1)?,
            ),
            None => (hostport, None),
        };
        if raw_host.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_abs });
        }
        for (i, b) in raw_host.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_abs + i,
                });
            }
        }
        (raw_host.to_ascii_lowercase(), port, false)
    };

    let fragment_mark = bytes[authority_end..]
        .iter()
        .position(|&b| b == b'#')
        .map(|n| authority_end + n);
    let before_fragment_end = fragment_mark.unwrap_or(input.len());
    let query_mark = bytes[authority_end..before_fragment_end]
        .iter()
        .position(|&b| b == b'?')
        .map(|n| authority_end + n);

    let path_end = query_mark.unwrap_or(before_fragment_end);
    let path = if authority_end == path_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };
    let query = query_mark.map(|q| input[q + 1..before_fragment_end].to_owned());
    let fragment = fragment_mark.map(|f| input[f + 1..].to_owned());

    if let Some(raw) = userinfo {
        validate_component(
            raw,
            Component::UserInfo,
            authority_start,
        )?;
    }
    if authority_end != path_end {
        validate_component(&path, Component::Path, authority_end)?;
    }
    if let (Some(q), Some(qmark)) = (&query, query_mark) {
        validate_component(q, Component::Query, qmark + 1)?;
    }
    if let (Some(f), Some(fmark)) = (&fragment, fragment_mark) {
        validate_component(f, Component::Fragment, fmark + 1)?;
    }

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo: userinfo.map(str::to_owned),
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

    let raw_segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    let mut terminal_navigation = false;

    for (i, segment) in raw_segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, segment).map_err(|e| match e {
            CodecError::BadEscape { at } => PathError::BadEscape { at: at + 1 },
            CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at: at + 1 },
        })?;
        let is_last = i + 1 == raw_segments.len();
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

fn global_byte_scan(input: &str) -> Result<(), ParseError> {
    for (i, b) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at: i });
        }
    }
    Ok(())
}

fn split_reference_components(reference: &str) -> (&str, Option<&str>, Option<&str>) {
    let hash = reference.find('#');
    let before_fragment = hash.map_or(reference, |i| &reference[..i]);
    let fragment = hash.map(|i| &reference[i + 1..]);
    let question = before_fragment.find('?');
    let path = question.map_or(before_fragment, |i| &before_fragment[..i]);
    let query = question.map(|i| &before_fragment[i + 1..]);
    (path, query, fragment)
}

fn normalise_url_path(url: &mut Url) -> Result<(), PathError> {
    url.path = normalise_path(&url.path)?;
    Ok(())
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let mut base_url = parse_url(base).map_err(ResolveError::Base)?;
    base_url.path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;

    global_byte_scan(reference).map_err(ResolveError::Reference)?;

    let first_delim = reference
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference.as_bytes()[..first_delim].contains(&b':') {
        let mut url = parse_url(reference).map_err(ResolveError::Reference)?;
        normalise_url_path(&mut url).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    if reference.starts_with("//") {
        let absolute = format!("{}:{}", base_url.scheme, reference);
        let mut url = parse_url(&absolute).map_err(ResolveError::Reference)?;
        normalise_url_path(&mut url).map_err(ResolveError::Path)?;
        return Ok(url);
    }

    let (reference_path, reference_query, reference_fragment) =
        split_reference_components(reference);

    let (selected_path, selected_query) = if reference_path.is_empty() {
        (
            base_url.path.clone(),
            if reference_query.is_some() {
                reference_query.map(str::to_owned)
            } else {
                base_url.query.clone()
            },
        )
    } else {
        let merged = if reference_path.starts_with('/') {
            reference_path.to_owned()
        } else {
            let slash = base_url.path.rfind('/').unwrap_or(0);
            format!("{}{}", &base_url.path[..=slash], reference_path)
        };
        (merged, reference_query.map(str::to_owned))
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
    if let Some(fragment) = reference_fragment {
        absolute.push('#');
        absolute.push_str(fragment);
    }

    let mut url = parse_url(&absolute).map_err(ResolveError::Reference)?;
    normalise_url_path(&mut url).map_err(ResolveError::Path)?;
    Ok(url)
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();

    for (field_start, field) in input
        .split('&')
        .scan(0usize, |offset, field| {
            let start = *offset;
            *offset += field.len() + 1;
            Some((start, field))
        })
    {
        if field.is_empty() {
            continue;
        }
        let (raw_key, raw_value) = match field.find('=') {
            Some(i) => (&field[..i], &field[i + 1..]),
            None => (field, ""),
        };
        let key_replaced = raw_key.replace('+', " ");
        let value_replaced = raw_value.replace('+', " ");

        let key = decode_component(Component::Query, &key_replaced).map_err(|e| match e {
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
        let value = decode_component(Component::Query, &value_replaced).map_err(|e| match e {
            CodecError::BadEscape { at } => QueryError::BadEscape {
                at: field_start + raw_key.len() + usize::from(field.contains('=')) + at,
            },
            CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 {
                at: field_start + raw_key.len() + usize::from(field.contains('=')) + at,
            },
        })?;

        if let Some((_, values)) = entries.iter_mut().find(|(existing, _)| existing == &key) {
            values.push(value);
        } else {
            entries.push((key, vec![value]));
        }
    }

    Ok(QueryMap { entries })
}

fn form_encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
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
        if let Some((_, collected)) = merged.iter_mut().find(|(existing, _)| existing == key) {
            collected.extend(values.iter().cloned());
        } else {
            merged.push((key.clone(), values.clone()));
        }
    }

    let mut fields = Vec::new();
    for (key, values) in merged {
        let encoded_key = form_encode(&key);
        for value in values {
            fields.push(format!("{}={}", encoded_key, form_encode(&value)));
        }
    }
    fields.join("&")
}

pub fn canonical_url(input: &str) -> Result<String, CanonicalError> {
    let mut url = parse_url(input).map_err(CanonicalError::Parse)?;
    url.path = normalise_path(&url.path).map_err(CanonicalError::Path)?;
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
        let addr = url
            .host
            .parse::<std::net::Ipv6Addr>()
            .expect("validated IPv6 address");
        out.push('[');
        out.push_str(&addr.to_string());
        out.push(']');
    } else {
        out.push_str(&url.host);
    }

    let keep_port = match (url.scheme.as_str(), url.port) {
        ("http", Some(80)) | ("https", Some(443)) => false,
        (_, Some(_)) => true,
        (_, None) => false,
    };
    if keep_port {
        out.push(':');
        out.push_str(&url.port.unwrap().to_string());
    }

    let mut encoded_segments = Vec::new();
    for segment in url.path.split('/') {
        let decoded = decode_component(Component::Path, segment).map_err(|e| match e {
            CodecError::BadEscape { at } => CanonicalError::Parse(ParseError::BadEscape { at }),
            CodecError::InvalidUtf8 { at } => {
                CanonicalError::Parse(ParseError::InvalidUtf8 { at })
            }
        })?;
        encoded_segments.push(encode_unreserved(&decoded));
    }
    out.push_str(&encoded_segments.join("/"));

    if let Some(map) = query_map {
        out.push('?');
        out.push_str(&serialise_query(&map));
    }
    if let Some(fragment) = &url.fragment {
        let decoded = decode_component(Component::Fragment, fragment).map_err(|e| match e {
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

fn encode_unreserved(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn main() {}