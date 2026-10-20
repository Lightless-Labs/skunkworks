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

fn global_check(bytes: &[u8]) -> Result<(), ParseError> {
    for (i, &b) in bytes.iter().enumerate() {
        if b < 0x21 || b > 0x7e {
            return Err(ParseError::InvalidByte { at: i });
        }
    }
    Ok(())
}
fn hv(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
fn map_codec(e: CodecError, offset: usize) -> ParseError {
    match e {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: offset + at },
    }
}

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut origin = Vec::with_capacity(input.len());
    let s = input.as_bytes();
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%' {
            if i.checked_add(2).filter(|&n| n < s.len()).is_none() {
                return Err(CodecError::BadEscape { at: i });
            }
            let h = hv(s[i + 1]);
            let l = hv(s[i + 2]);
            if h.is_none() || l.is_none() {
                return Err(CodecError::BadEscape { at: i });
            }
            bytes.push(h.unwrap() * 16 + l.unwrap());
            origin.push(i);
            i += 3;
        } else {
            bytes.push(s[i]);
            origin.push(i);
            i += 1;
        }
    }
    match std::str::from_utf8(&bytes) {
        Ok(t) => Ok(t.to_owned()),
        Err(e) => Err(CodecError::InvalidUtf8 {
            at: origin.get(e.valid_up_to()).copied().unwrap_or(s.len()),
        }),
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut result = String::new();
    for b in text.bytes() {
        let safe = b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'.' | b'_' | b'~')
            || match component {
                Component::UserInfo => b == b':',
                Component::Path => b == b'/',
                Component::Query => b == b'/' || b == b'?',
                Component::Fragment => b == b'/' || b == b'?' || b == b'#',
            };
        if safe {
            result.push(b as char);
        } else {
            result.push('%');
            result.push(HEX[(b >> 4) as usize] as char);
            result.push(HEX[(b & 15) as usize] as char);
        }
    }
    result
}

fn parse_port(s: &str, at: usize) -> Result<u16, ParseError> {
    if s.is_empty() {
        return Err(ParseError::InvalidPort { at });
    }
    if let Some((i, _)) = s.bytes().enumerate().find(|(_, b)| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: at + i });
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(ParseError::InvalidPort { at });
    }
    let mut n = 0u32;
    for b in s.bytes() {
        n = n
            .checked_mul(10)
            .and_then(|x| x.checked_add((b - b'0') as u32))
            .ok_or(ParseError::InvalidPort { at })?;
    }
    if n == 0 || n > 65535 {
        Err(ParseError::InvalidPort { at })
    } else {
        Ok(n as u16)
    }
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    global_check(input.as_bytes())?;
    let b = input.as_bytes();
    let c = match b.iter().position(|&x| x == b':') {
        Some(i) => i,
        None => return Err(ParseError::InvalidScheme { at: b.len() }),
    };
    if c == 0 || !b[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for i in 1..c {
        if !(b[i].is_ascii_alphanumeric() || matches!(b[i], b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }
    if b.get(c + 1) != Some(&b'/') || b.get(c + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: c + 1 });
    }
    let scheme = input[..c].to_ascii_lowercase();
    let auth_start = c + 3;
    let auth_end = b[auth_start..]
        .iter()
        .position(|x| matches!(x, b'/' | b'?' | b'#'))
        .map(|x| auth_start + x)
        .unwrap_or(b.len());
    let a = &input[auth_start..auth_end];

    let first_at = a.find('@');
    let userinfo;
    let hp_start;
    if let Some(at) = first_at {
        if let Some(second) = a[at + 1..].find('@') {
            return Err(ParseError::InvalidUserInfo {
                at: auth_start + at + 1 + second,
            });
        }
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: auth_start });
        }
        userinfo = Some(a[..at].to_owned());
        hp_start = at + 1;
    } else {
        userinfo = None;
        hp_start = 0;
    }
    let hp = &a[hp_start..];
    let host_at = auth_start + hp_start;
    let (host, port, ipv6) = if hp.starts_with('[') {
        let close = hp.find(']').ok_or(ParseError::InvalidIpv6 { at: host_at })?;
        let inner = &hp[1..close];
        if Ipv6Addr::from_str(inner).is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_at });
        }
        let rest = &hp[close + 1..];
        let port = if rest.is_empty() {
            None
        } else if rest.starts_with(':') {
            Some(parse_port(&rest[1..], host_at + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_at + close + 1,
            });
        };
        (inner.to_ascii_lowercase(), port, true)
    } else {
        let split = hp.find(':');
        let (h, p, p_at) = match split {
            Some(i) => (&hp[..i], Some(&hp[i + 1..]), host_at + i + 1),
            None => (hp, None, host_at + hp.len()),
        };
        if h.is_empty() {
            return Err(ParseError::InvalidHost { at: host_at });
        }
        for (i, ch) in h.bytes().enumerate() {
            if !(ch.is_ascii_alphanumeric() || matches!(ch, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost { at: host_at + i });
            }
        }
        let port = match p {
            Some(s) => Some(parse_port(s, p_at)?),
            None => None,
        };
        (h.to_ascii_lowercase(), port, false)
    };

    let tail = &input[auth_end..];
    let fpos = tail.find('#');
    let pre_f = fpos.map(|i| &tail[..i]).unwrap_or(tail);
    let frag = fpos.map(|i| tail[i + 1..].to_owned());
    let qpos = pre_f.find('?');
    let raw_path = qpos.map(|i| &pre_f[..i]).unwrap_or(pre_f);
    let query = qpos.map(|i| pre_f[i + 1..].to_owned());
    let path = if raw_path.is_empty() { "/".to_string() } else { raw_path.to_string() };

    if let Some(u) = userinfo.as_ref() {
        decode_component(Component::UserInfo, u).map_err(|e| map_codec(e, auth_start))?;
    }
    if !raw_path.is_empty() {
        decode_component(Component::Path, raw_path).map_err(|e| map_codec(e, auth_end))?;
    }
    if let Some(q) = query.as_ref() {
        decode_component(Component::Query, q).map_err(|e| map_codec(e, auth_end + qpos.unwrap() + 1))?;
    }
    if let Some(f) = frag.as_ref() {
        decode_component(Component::Fragment, f).map_err(|e| map_codec(e, auth_end + fpos.unwrap() + 1))?;
    }

    Ok(Url { scheme, userinfo, host, port, path, query, fragment: frag, ipv6 })
}

fn codec_to_path(e: CodecError) -> PathError {
    match e {
        CodecError::BadEscape { at } => PathError::BadEscape { at },
        CodecError::InvalidUtf8 { at } => PathError::InvalidUtf8 { at },
    }
}

pub fn normalise_path(path: &str) -> Result<String, PathError> {
    if !path.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    decode_component(Component::Path, path).map_err(codec_to_path)?;
    let mut stack: Vec<&str> = Vec::new();
    let mut last_navigation = false;
    for segment in path[1..].split('/') {
        let decoded = decode_component(Component::Path, segment).map_err(codec_to_path)?;
        match decoded.as_str() {
            "." => last_navigation = true,
            ".." => {
                last_navigation = true;
                stack.pop();
            }
            _ => {
                last_navigation = false;
                stack.push(segment);
            }
        }
    }
    if path == "/" {
        return Ok("/".to_owned());
    }
    if last_navigation {
        stack.push("");
    }
    Ok(format!("/{}", stack.join("/")))
}

fn codec_to_query(e: CodecError, offset: usize) -> QueryError {
    match e {
        CodecError::BadEscape { at } => QueryError::BadEscape { at: offset + at },
        CodecError::InvalidUtf8 { at } => QueryError::InvalidUtf8 { at: offset + at },
    }
}

pub fn parse_query(input: &str) -> Result<QueryMap, QueryError> {
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut start = 0;
    for field in input.split('&') {
        let at = start;
        start += field.len() + 1;
        if field.is_empty() {
            continue;
        }
        let eq = field.find('=');
        let (key_raw, value_raw) = eq.map(|i| (&field[..i], &field[i + 1..])).unwrap_or((field, ""));
        let key_prepared = key_raw.replace('+', " ");
        let key = decode_component(Component::Query, &key_prepared)
            .map_err(|e| codec_to_query(e, at))?;
        if key.is_empty() {
            return Err(QueryError::EmptyKey { at });
        }
        let value_at = at + key_raw.len() + usize::from(eq.is_some());
        let value_prepared = value_raw.replace('+', " ");
        let value = decode_component(Component::Query, &value_prepared)
            .map_err(|e| codec_to_query(e, value_at))?;
        if let Some((_, values)) = entries.iter_mut().find(|(k, _)| k == &key) {
            values.push(value);
        } else {
            entries.push((key, vec![value]));
        }
    }
    Ok(QueryMap { entries })
}

fn form_encode(input: &str) -> String {
    let mut output = String::new();
    const H: &[u8; 16] = b"0123456789ABCDEF";
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            output.push(b as char);
        } else if b == b' ' {
            output.push('+');
        } else {
            output.push('%');
            output.push(H[(b >> 4) as usize] as char);
            output.push(H[(b & 15) as usize] as char);
        }
    }
    output
}

pub fn serialise_query(map: &QueryMap) -> String {
    let mut merged: Vec<(String, Vec<String>)> = Vec::new();
    for (key, vals) in &map.entries {
        if key.is_empty() || vals.is_empty() { continue; }
        if let Some((_, found)) = merged.iter_mut().find(|(k, _)| k == key) {
            found.extend(vals.iter().cloned());
        } else {
            merged.push((key.clone(), vals.clone()));
        }
    }
    let mut fields = Vec::new();
    for (key, vals) in merged {
        let key = form_encode(&key);
        for val in vals {
            fields.push(format!("{}={}", key, form_encode(&val)));
        }
    }
    fields.join("&")
}

pub fn resolve_url(base: &str, reference: &str) -> Result<Url, ResolveError> {
    let base_url = parse_url(base).map_err(ResolveError::Base)?;
    let base_path = normalise_path(&base_url.path).map_err(ResolveError::Path)?;
    global_check(reference.as_bytes()).map_err(ResolveError::Reference)?;

    let b = reference.as_bytes();
    let cutoff = b.iter().position(|&x| matches!(x, b'/' | b'?' | b'#')).unwrap_or(b.len());
    if b[..cutoff].contains(&b':') {
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

    let fpos = reference.find('#');
    let pre_f = fpos.map(|i| &reference[..i]).unwrap_or(reference);
    let fragment = fpos.map(|i| reference[i + 1..].to_string());
    let qpos = pre_f.find('?');
    let ref_path = qpos.map(|i| &pre_f[..i]).unwrap_or(pre_f);
    let ref_query = qpos.map(|i| pre_f[i + 1..].to_string());

    let (chosen_path, chosen_query) = if ref_path.is_empty() {
        (base_path, if qpos.is_some() { ref_query } else { base_url.query.clone() })
    } else {
        let merged = if ref_path.starts_with('/') {
            ref_path.to_string()
        } else {
            let cut = base_path.rfind('/').map(|i| i + 1).unwrap_or(0);
            format!("{}{}", &base_path[..cut], ref_path)
        };
        (merged, ref_query)
    };

    let mut raw = format!("{}://", base_url.scheme);
    if let Some(info) = &base_url.userinfo {
        raw.push_str(info);
        raw.push('@');
    }
    if base_url.ipv6 {
        raw.push('[');
        raw.push_str(&base_url.host);
        raw.push(']');
    } else {
        raw.push_str(&base_url.host);
    }
    if let Some(p) = base_url.port {
        raw.push(':');
        raw.push_str(&p.to_string());
    }
    raw.push_str(&chosen_path);
    if let Some(q) = chosen_query {
        raw.push('?');
        raw.push_str(&q);
    }
    if let Some(f) = fragment {
        raw.push('#');
        raw.push_str(&f);
    }
    let mut u = parse_url(&raw).map_err(ResolveError::Reference)?;
    u.path = normalise_path(&u.path).map_err(ResolveError::Path)?;
    Ok(u)
}

fn encode_unreserved(input: &str) -> String {
    let mut out = String::new();
    const H: &[u8; 16] = b"0123456789ABCDEF";
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(H[(b >> 4) as usize] as char);
            out.push(H[(b & 15) as usize] as char);
        }
    }
    out
}

pub fn canonical_url(input: &str) -> Result<String, CanonicalError> {
    let u = parse_url(input).map_err(CanonicalError::Parse)?;
    let path = normalise_path(&u.path).map_err(CanonicalError::Path)?;
    let query = match &u.query {
        Some(q) => Some(parse_query(q).map_err(CanonicalError::Query)?),
        None => None,
    };

    let mut out = format!("{}://", u.scheme);
    if let Some(info) = &u.userinfo {
        let decoded = decode_component(Component::UserInfo, info)
            .map_err(|e| CanonicalError::Parse(map_codec(e, 0)))?;
        out.push_str(&encode_component(Component::UserInfo, &decoded));
        out.push('@');
    }
    if u.ipv6 {
        let address = Ipv6Addr::from_str(&u.host).expect("parsed IPv6");
        out.push('[');
        out.push_str(&address.to_string());
        out.push(']');
    } else {
        out.push_str(&u.host);
    }
    if let Some(port) = u.port {
        if !((u.scheme == "http" && port == 80) || (u.scheme == "https" && port == 443)) {
            out.push(':');
            out.push_str(&port.to_string());
        }
    }

    let encoded_segments: Vec<String> = path
        .split('/')
        .map(|segment| {
            let decoded = decode_component(Component::Path, segment).expect("normalized path validated");
            encode_unreserved(&decoded)
        })
        .collect();
    out.push_str(&encoded_segments.join("/"));
    if let Some(q) = query {
        out.push('?');
        out.push_str(&serialise_query(&q));
    }
    if let Some(fragment) = &u.fragment {
        let decoded = decode_component(Component::Fragment, fragment)
            .map_err(|e| CanonicalError::Parse(map_codec(e, 0)))?;
        out.push('#');
        out.push_str(&encode_component(Component::Fragment, &decoded));
    }
    Ok(out)
}
