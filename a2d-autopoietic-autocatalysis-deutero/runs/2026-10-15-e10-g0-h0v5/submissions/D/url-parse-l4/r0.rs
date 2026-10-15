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
            let Some(hi) = hex_value(bytes[i + 1]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            let Some(lo) = hex_value(bytes[i + 2]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            decoded.push((hi << 4) | lo);
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
        Err(e) => {
            let decoded_offset = e.valid_up_to();
            let original_offset = origins.get(decoded_offset).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_offset })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        let extra_safe = match component {
            Component::UserInfo => b == b':',
            Component::Path => b == b'/',
            Component::Query => matches!(b, b'/' | b'?'),
            Component::Fragment => matches!(b, b'/' | b'?' | b'#'),
        };
        if unreserved || extra_safe {
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

fn map_codec_parse(error: CodecError, offset: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

fn parse_port(raw: &str, start: usize) -> Result<u16, ParseError> {
    if raw.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }
    if let Some(i) = raw.as_bytes().iter().position(|b| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: start + i });
    }
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for &b in bytes {
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

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    let bytes = input.as_bytes();
    if let Some(i) = bytes.iter().position(|b| !(0x21..=0x7e).contains(b)) {
        return Err(ParseError::InvalidByte { at: i });
    }

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

    if colon + 2 >= bytes.len() || bytes[colon + 1] != b'/' || bytes[colon + 2] != b'/' {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .map(|i| authority_start + i)
        .unwrap_or(bytes.len());
    let authority = &input[authority_start..authority_end];
    let auth_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = auth_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();
    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }

    let (userinfo, hostport, hostport_start) = if let Some(&at_pos) = at_positions.first() {
        if at_pos == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at_pos,
            });
        }
        (
            Some(authority[..at_pos].to_owned()),
            &authority[at_pos + 1..],
            authority_start + at_pos + 1,
        )
    } else {
        (None, authority, authority_start)
    };

    let mut ipv6 = false;
    let (host, port) = if hostport.starts_with('[') {
        let close = match hostport.as_bytes()[1..].iter().position(|&b| b == b']') {
            Some(i) => i + 1,
            None => return Err(ParseError::InvalidIpv6 { at: hostport_start }),
        };
        let interior = &hostport[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_start });
        }
        ipv6 = true;
        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], hostport_start + close + 1)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_start + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), port)
    } else {
        let split = hostport.find(':');
        let (raw_host, raw_port, port_start) = if let Some(pos) = split {
            (
                &hostport[..pos],
                Some(&hostport[pos + 1..]),
                hostport_start + pos + 1,
            )
        } else {
            (hostport, None, hostport_start + hostport.len())
        };

        if raw_host.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_start });
        }
        if let Some(i) = raw_host
            .as_bytes()
            .iter()
            .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')))
        {
            return Err(ParseError::InvalidHost {
                at: hostport_start + i,
            });
        }
        let port = match raw_port {
            Some(p) => Some(parse_port(p, port_start)?),
            None => None,
        };
        (raw_host.to_ascii_lowercase(), port)
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
    let path = if authority_end == path_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };
    let query = query_pos.map(|q| input[q + 1..before_fragment_end].to_owned());
    let fragment = fragment_pos.map(|f| input[f + 1..].to_owned());

    if let Some(user) = &userinfo {
        let start = authority_start;
        decode_component(Component::UserInfo, user)
            .map_err(|e| map_codec_parse(e, start))?;
    }
    if path != "/" || authority_end != path_end {
        decode_component(Component::Path, &path)
            .map_err(|e| map_codec_parse(e, authority_end))?;
    }
    if let (Some(q), Some(qpos)) = (&query, query_pos) {
        decode_component(Component::Query, q)
            .map_err(|e| map_codec_parse(e, qpos + 1))?;
    }
    if let (Some(f), Some(fpos)) = (&fragment, fragment_pos) {
        decode_component(Component::Fragment, f)
            .map_err(|e| map_codec_parse(e, fpos + 1))?;
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

    let decoded = decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;

    let raw_segments: Vec<&str> = path[1..].split('/').collect();
    let decoded_segments: Vec<&str> = decoded[1..].split('/').collect();
    let mut stack: Vec<String> = Vec::new();
    let mut last_navigation = false;

    for (raw, dec) in raw_segments.iter().zip(decoded_segments.iter()) {
        if *dec == "." {
            last_navigation = true;
        } else if *dec == ".." {
            if !stack.is_empty() {
                stack.pop();
            }
            last_navigation = true;
        } else {
            stack.push((*raw).to_owned());
            last_navigation = false;
        }
    }

    if last_navigation {
        stack.push(String::new());
    }

    if stack.is_empty() {
        Ok("/".to_owned())
    } else {
        Ok(format!("/{}", stack.join("/")))
    }
}

fn scan_reference_bytes(reference: &str) -> Result<(), ParseError> {
    if let Some(at) = reference
        .as_bytes()
        .iter()
        .position(|b| !(0x21..=0x7e).contains(b))
    {
        Err(ParseError::InvalidByte { at })
    } else {
        Ok(())
    }
}

fn normalized_url(mut url: Url) -> Result<Url, PathError> {
    url.path = normalise_path(&url.path)?;
    Ok(url)
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let normalized_base_path =
        normalise_path(&base_url.path).map_err(ResolveError::Path)?;

    scan_reference_bytes(reference).map_err(ResolveError::Reference)?;

    let first_delimiter = reference
        .as_bytes()
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(reference.len());
    if reference[..first_delimiter].contains(':') {
        let url = parse_url(reference).map_err(ResolveError::Reference)?;
        return normalized_url(url).map_err(ResolveError::Path);
    }

    if reference.starts_with("//") {
        let constructed = format!("{}:{}", base_url.scheme, reference);
        let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
        return normalized_url(url).map_err(ResolveError::Path);
    }

    let bytes = reference.as_bytes();
    let fragment_pos = bytes.iter().position(|&b| b == b'#');
    let before_fragment_end = fragment_pos.unwrap_or(bytes.len());
    let query_pos = bytes[..before_fragment_end]
        .iter()
        .position(|&b| b == b'?');

    let path_end = query_pos.unwrap_or(before_fragment_end);
    let ref_path = &reference[..path_end];
    let ref_query = query_pos.map(|q| reference[q + 1..before_fragment_end].to_owned());
    let ref_fragment = fragment_pos.map(|f| reference[f + 1..].to_owned());

    let (selected_path, selected_query) = if ref_path.is_empty() {
        (
            normalized_base_path,
            if query_pos.is_none() {
                base_url.query.clone()
            } else {
                ref_query
            },
        )
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path.to_owned()
        } else {
            let slash = normalized_base_path.rfind('/').unwrap_or(0);
            format!("{}{}", &normalized_base_path[..=slash], ref_path)
        };
        (merged, ref_query)
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
    constructed.push_str(&selected_path);
    if let Some(query) = selected_query {
        constructed.push('?');
        constructed.push_str(&query);
    }
    if let Some(fragment) = ref_fragment {
        constructed.push('#');
        constructed.push_str(&fragment);
    }

    let url = parse_url(&constructed).map_err(ResolveError::Reference)?;
    normalized_url(url).map_err(ResolveError::Path)
}

fn replace_plus_with_space(s: &str) -> String {
    s.replace('+', " ")
}

fn map_codec_query(error: CodecError, offset: usize) -> QueryError {
    match error {
        CodecError::BadEscape { at } => QueryError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: offset + at },
    }
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut field_start = 0;

    for field in input.split('&') {
        let field_len = field.len();
        if !field.is_empty() {
            let eq = field.find('=');
            let (raw_key, raw_value, value_offset) = match eq {
                Some(i) => (&field[..i], &field[i + 1..], field_start + i + 1),
                None => (field, "", field_start + field_len),
            };

            let key_input = replace_plus_with_space(raw_key);
            let key = decode_component(Component::Query, &key_input)
                .map_err(|e| map_codec_query(e, field_start))?;
            if key.is_empty() {
                return Err(QueryError::EmptyKey { at: field_start });
            }

            let value_input = replace_plus_with_space(raw_value);
            let value = decode_component(Component::Query, &value_input)
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

fn encode_form(text: &str) -> String {
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
        if let Some((_, existing)) = merged.iter_mut().find(|(k, _)| k == key) {
            existing.extend(values.iter().cloned());
        } else {
            merged.push((key.clone(), values.clone()));
        }
    }

    let mut fields = Vec::new();
    for (key, values) in merged {
        let encoded_key = encode_form(&key);
        for value in values {
            fields.push(format!("{}={}", encoded_key, encode_form(&value)));
        }
    }
    fields.join("&")
}

fn encode_unreserved(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
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

pub fn canonical_url(input: &str) -> Result<String, CanonicalError> {
    let url = parse_url(input).map_err(CanonicalError::Parse)?;
    let path = normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query_map = match &url.query {
        Some(q) => Some(parse_query(q).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = format!("{}://", url.scheme);
    if let Some(userinfo) = &url.userinfo {
        let decoded = decode_component(Component::UserInfo, userinfo)
            .map_err(|e| CanonicalError::Parse(map_codec_parse(e, 0)))?;
        out.push_str(&encode_component(Component::UserInfo, &decoded));
        out.push('@');
    }

    if url.ipv6 {
        let address = Ipv6Addr::from_str(&url.host).expect("validated IPv6 address");
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

    let mut encoded_segments = Vec::new();
    for segment in path.split('/') {
        let decoded = decode_component(Component::Path, segment)
            .expect("normalized parsed path contains valid escapes and UTF-8");
        encoded_segments.push(encode_unreserved(&decoded));
    }
    out.push_str(&encoded_segments.join("/"));

    if let Some(map) = query_map {
        out.push('?');
        out.push_str(&serialise_query(&map));
    }
    if let Some(fragment) = &url.fragment {
        let decoded = decode_component(Component::Fragment, fragment)
            .expect("parsed fragment contains valid escapes and UTF-8");
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }
    Ok(out)
}