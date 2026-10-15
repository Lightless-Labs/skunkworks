// Fault: Loses empty values on duplicate keys.
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
    InvalidIpv6 { at: usize }, BadEscape { at: usize }, InvalidUtf8 { at: usize },
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() { return Err(ParseError::Empty); }
    for (at, b) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) { return Err(ParseError::InvalidByte { at }); }
    }
    let colon = input.find(':').ok_or(ParseError::InvalidScheme { at: input.len() })?;
    let scheme = &input[..colon];
    if scheme.is_empty() || !scheme.as_bytes()[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (at, b) in scheme.bytes().enumerate().skip(1) {
        if !(b.is_ascii_alphanumeric() || b"+-.".contains(&b)) {
            return Err(ParseError::InvalidScheme { at });
        }
    }
    if !input[colon + 1..].starts_with("//") {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }
    let start = colon + 3;
    let end = input[start..].find(['/', '?', '#']).map_or(input.len(), |i| start + i);
    let authority = &input[start..end];
    let ats: Vec<usize> = authority.match_indices('@').map(|(i, _)| i).collect();
    if ats.len() > 1 { return Err(ParseError::InvalidUserInfo { at: start + ats[1] }); }
    let (userinfo, host_start) = match ats.first() {
        Some(&0) => return Err(ParseError::InvalidUserInfo { at: start }),
        Some(&i) => (Some(authority[..i].to_owned()), start + i + 1),
        None => (None, start),
    };
    let hp = &input[host_start..end];
    
    let (host, ipv6, port_start) = if hp.starts_with('[') {
        let close = hp.find(']').ok_or(ParseError::InvalidIpv6 { at: host_start })?;
        let interior = &hp[1..close];
        interior.parse::<std::net::Ipv6Addr>().map_err(|_| ParseError::InvalidIpv6 { at: host_start })?;
        let suffix = host_start + close + 1;
        if suffix < end && input.as_bytes()[suffix] != b':' {
            return Err(ParseError::InvalidHost { at: suffix });
        }
        (interior.to_ascii_lowercase(), true, (suffix < end).then_some(suffix + 1))
    } else {
        let h_end = hp.find(':').map_or(end, |i| host_start + i);
        let raw_host = &input[host_start..h_end];
        check_host(raw_host, host_start)?;
        (raw_host.to_ascii_lowercase(), false, (h_end < end).then_some(h_end + 1))
    };

    let port = if let Some(ps) = port_start {
        let p = &input[ps..end];
        if p.is_empty() { return Err(ParseError::InvalidPort { at: ps }); }
        if let Some(i) = p.bytes().position(|b| !b.is_ascii_digit()) {
            return Err(ParseError::InvalidPort { at: ps + i });
        }
        if p.len() > 1 && p.starts_with('0') { return Err(ParseError::InvalidPort { at: ps }); }
        let n: u16 = p.parse().map_err(|_| ParseError::InvalidPort { at: ps })?;
        if n == 0 { return Err(ParseError::InvalidPort { at: ps }); }
        Some(n)
    } else { None };
    let hash = input[end..].find('#').map(|i| end + i);
    let before_hash = hash.unwrap_or(input.len());
    let question = input[end..before_hash].find('?').map(|i| end + i);
    let path_end = question.unwrap_or(before_hash);
    let path = if path_end == end { "/".to_owned() } else { input[end..path_end].to_owned() };
    let query = question.map(|i| input[i + 1..before_hash].to_owned());
    let fragment = hash.map(|i| input[i + 1..].to_owned());
    
    let validate = |component, text: &str, origin| -> Result<(), ParseError> {
        decode_component(component, text).map(|_| ()).map_err(|e| match e {
            CodecError::BadEscape { at } => ParseError::BadEscape { at: origin + at },
            CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: origin + at },
        })
    };
    if let Some(ref u) = userinfo { validate(Component::UserInfo, u, start)?; }
    validate(Component::Path, &path, end)?;
    if let Some(ref q) = query { validate(Component::Query, q, question.unwrap() + 1)?; }
    if let Some(ref f) = fragment { validate(Component::Fragment, f, hash.unwrap() + 1)?; }

    Ok(Url { scheme: scheme.to_ascii_lowercase(), userinfo, host, port, path, query, fragment, ipv6 })
}

fn check_host(host: &str, start: usize) -> Result<(), ParseError> {
    if host.is_empty() { return Err(ParseError::InvalidHost { at: start }); }
    for (i, b) in host.bytes().enumerate() {
        if !(b.is_ascii_alphanumeric() || b".-_".contains(&b)) {
            return Err(ParseError::InvalidHost { at: start + i });
        }
    }
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component { UserInfo, Path, Query, Fragment }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError { BadEscape { at: usize }, InvalidUtf8 { at: usize } }

fn hex(b: u8) -> Option<u8> {
    match b { b'0'..=b'9' => Some(b - b'0'), b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10), _ => None }
}
pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::new();
    let mut origins = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        origins.push(i);
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() { return Err(CodecError::BadEscape { at: i }); }
            let a = hex(bytes[i + 1]).ok_or(CodecError::BadEscape { at: i })?;
            let b = hex(bytes[i + 2]).ok_or(CodecError::BadEscape { at: i })?;
            decoded.push(a * 16 + b);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    match String::from_utf8(decoded) {
        Ok(s) => Ok(s),
        Err(e) => Err(CodecError::InvalidUtf8 { at: origins[e.utf8_error().valid_up_to()] }),
    }
}
fn unreserved(b: u8) -> bool { b.is_ascii_alphanumeric() || b"-._~".contains(&b) }
fn escaped(out: &mut String, b: u8) {
    const HEX: &[u8] = b"0123456789ABCDEF";
    out.push('%'); out.push(HEX[(b >> 4) as usize] as char); out.push(HEX[(b & 15) as usize] as char);
}
pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        let extra = match component {
            Component::UserInfo => b == b':', Component::Path => b == b'/',
            Component::Query => b"/?".contains(&b), Component::Fragment => b"/?#".contains(&b),
        };
        if unreserved(b) || extra { out.push(b as char); } else { escaped(&mut out, b); }
    }
    out
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError { NotAbsolute, BadEscape { at: usize }, InvalidUtf8 { at: usize } }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError { Base(ParseError), Reference(ParseError), Path(PathError) }

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') { return Err(PathError::NotAbsolute); }
    decode_component(Component::Path, path).map_err(|e| match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    })?;
    let segments: Vec<&str> = path[1..].split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    for (i, raw) in segments.iter().enumerate() {
        let decoded = decode_component(Component::Path, raw).unwrap();
        match decoded.as_str() {
            "." => {},
            ".." => { stack.pop(); },
            _ => stack.push(raw),
        }
        if i + 1 == segments.len() && (decoded == "." || decoded == "..") { stack.push(""); }
    }
    Ok(format!("/{}", stack.join("/")))
}
fn raw_url(url: &Url) -> String {
    let mut s = format!("{}://", url.scheme);
    if let Some(ref u) = url.userinfo { s.push_str(u); s.push('@'); }
    if url.ipv6 { s.push('['); s.push_str(&url.host); s.push(']'); } else { s.push_str(&url.host); }
    if let Some(p) = url.port { s.push(':'); s.push_str(&p.to_string()); }
    s.push_str(&url.path);
    if let Some(ref q) = url.query { s.push('?'); s.push_str(q); }
    if let Some(ref f) = url.fragment { s.push('#'); s.push_str(f); }
    s
}
fn normalise_url(mut url: Url) -> Result<Url, ResolveError> {
    url.path = normalise_path(&url.path).map_err(ResolveError::Path)?;
    Ok(url)
}
pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base = normalise_url(parse_url(base).map_err(ResolveError::Base)?)?;
    for (at, b) in reference.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&b) { return Err(ResolveError::Reference(ParseError::InvalidByte { at })); }
    }
    let limit = reference.find(['/', '?', '#']).unwrap_or(reference.len());
    if reference[..limit].contains(':') {
        return normalise_url(parse_url(reference).map_err(ResolveError::Reference)?);
    }
    if reference.starts_with("//") {
        let absolute = format!("{}:{}", base.scheme, reference);
        return normalise_url(parse_url(&absolute).map_err(ResolveError::Reference)?);
    }
    let hash = reference.find('#');
    let end = hash.unwrap_or(reference.len());
    let question = reference[..end].find('?');
    let path = &reference[..question.unwrap_or(end)];
    let query = question.map(|i| reference[i + 1..end].to_owned());
    let fragment = hash.map(|i| reference[i + 1..].to_owned());
    let selected_path = if path.is_empty() { base.path.clone() }
        else if path.starts_with('/') { path.to_owned() }
        else { format!("{}{}", &base.path[..base.path.rfind('/').unwrap() + 1], path) };
    let selected_query = if path.is_empty() && question.is_none() { base.query.clone() } else { query };
    let url = Url { path: selected_path, query: selected_query, fragment, ..base };
    normalise_url(parse_url(&raw_url(&url)).map_err(ResolveError::Reference)?)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryMap { pub entries: Vec<(String, Vec<String>)> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError { BadEscape { at: usize }, InvalidUtf8 { at: usize }, EmptyKey { at: usize } }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError { Parse(ParseError), Path(PathError), Query(QueryError) }

fn query_decode(raw: &str, origin: usize) -> Result<String, QueryError> {
    decode_component(Component::Query, &raw.replace('+', " ")).map_err(|e| match e {
        CodecError::BadEscape { at } => QueryError::BadEscape { at: origin + at },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: origin + at },
    })
}
fn add_entry(entries: &mut Vec<(String, Vec<String>)>, key: String, values: Vec<String>) {
    if let Some((_, old)) = entries.iter_mut().find(|(k, _)| k == &key) { old.extend(values.into_iter().filter(|v| !v.is_empty())); }
    else { entries.push((key, values)); }
}
pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries = Vec::new();
    let mut start = 0;
    for field in input.split('&') {
        if !field.is_empty() {
            let eq = field.find('=');
            let key = query_decode(&field[..eq.unwrap_or(field.len())], start)?;
            if key.is_empty() { return Err(QueryError::EmptyKey { at: start }); }
            let value = if let Some(i) = eq { query_decode(&field[i + 1..], start + i + 1)? } else { String::new() };
            add_entry(&mut entries, key, vec![value]);
        }
        start += field.len() + 1;
    }
    Ok(QueryMap { entries })
}
fn form_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if unreserved(b) { out.push(b as char); }
        else if b == b' ' { out.push('+'); }
        else { escaped(&mut out, b); }
    }
    out
}
pub fn serialise_query(map: &QueryMap) -> String {
    let mut entries = Vec::new();
    for (key, values) in &map.entries {
        if !key.is_empty() && !values.is_empty() { add_entry(&mut entries, key.clone(), values.clone()); }
    }
    let mut fields = Vec::new();
    for (key, values) in entries {
        for value in values { fields.push(format!("{}={}", form_encode(&key), form_encode(&value))); }
    }
    fields.join("&")
}
pub fn canonical_url(input: &str) -> Result<String, CanonicalError> {
    let mut url = parse_url(input).map_err(CanonicalError::Parse)?;
    let path = normalise_path(&url.path).map_err(CanonicalError::Path)?;
    let query = url.query.as_deref().map(parse_query).transpose().map_err(CanonicalError::Query)?;
    if let Some(ref mut u) = url.userinfo {
        *u = encode_component(Component::UserInfo, &decode_component(Component::UserInfo, u).unwrap());
    }
    if url.ipv6 { url.host = url.host.parse::<std::net::Ipv6Addr>().unwrap().to_string(); }
    if (url.scheme == "http" && url.port == Some(80)) || (url.scheme == "https" && url.port == Some(443)) {
        url.port = None;
    }
    let segments: Vec<String> = path.split('/').map(|raw| {
        let decoded = decode_component(Component::Path, raw).unwrap();
        let mut out = String::new();
        for b in decoded.bytes() { if unreserved(b) { out.push(b as char); } else { escaped(&mut out, b); } }
        out
    }).collect();
    url.path = segments.join("/");
    url.query = query.as_ref().map(serialise_query);
    url.fragment = url.fragment.as_ref().map(|f| encode_component(Component::Fragment, &decode_component(Component::Fragment, f).unwrap()));
    Ok(raw_url(&url))
}
