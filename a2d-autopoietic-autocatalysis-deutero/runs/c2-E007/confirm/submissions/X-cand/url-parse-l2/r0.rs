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
        Err(e) => {
            let decoded_at = e.valid_up_to();
            Err(CodecError::InvalidUtf8 {
                at: offsets.get(decoded_at).copied().unwrap_or(input.len()),
            })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

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
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    let bytes = input.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if !(0x21..=0x7e).contains(&b) {
            return Err(ParseError::InvalidByte { at: i });
        }
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

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|i| authority_start + i)
        .unwrap_or(bytes.len());

    let authority = &bytes[authority_start..authority_end];
    let mut at_positions = authority
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i));
    let first_at = at_positions.next();
    if let Some(second_at) = at_positions.next() {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + second_at,
        });
    }

    let (userinfo, host_start) = if let Some(at) = first_at {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (
            Some(input[authority_start..authority_start + at].to_owned()),
            authority_start + at + 1,
        )
    } else {
        (None, authority_start)
    };

    let mut ipv6 = false;
    let (host, port) = if bytes.get(host_start) == Some(&b'[') {
        let close = bytes[host_start + 1..authority_end]
            .iter()
            .position(|&b| b == b']')
            .map(|i| host_start + 1 + i);
        let close = match close {
            Some(i) => i,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };
        let interior = &input[host_start + 1..close];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }
        ipv6 = true;
        let suffix_start = close + 1;
        let port = if suffix_start == authority_end {
            None
        } else if bytes[suffix_start] == b':' {
            Some(parse_port(bytes, suffix_start + 1, authority_end)?)
        } else {
            return Err(ParseError::InvalidHost { at: suffix_start });
        };
        (interior.to_ascii_lowercase(), port)
    } else {
        let split = bytes[host_start..authority_end]
            .iter()
            .position(|&b| b == b':')
            .map(|i| host_start + i);
        let host_end = split.unwrap_or(authority_end);
        if host_end == host_start {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for i in host_start..host_end {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost { at: i });
            }
        }
        let port = match split {
            Some(i) => Some(parse_port(bytes, i + 1, authority_end)?),
            None => None,
        };
        (input[host_start..host_end].to_ascii_lowercase(), port)
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

    let query = query_pos.map(|q| input[q + 1..before_fragment_end].to_owned());
    let fragment = fragment_pos.map(|f| input[f + 1..].to_owned());

    if let Some((text, base)) = userinfo.as_deref().map(|s| (s, authority_start)) {
        validate_component(Component::UserInfo, text, base)?;
    }
    if path_end != authority_end {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let Some(q) = query_pos {
        validate_component(
            Component::Query,
            query.as_deref().unwrap_or(""),
            q + 1,
        )?;
    }
    if let Some(f) = fragment_pos {
        validate_component(
            Component::Fragment,
            fragment.as_deref().unwrap_or(""),
            f + 1,
        )?;
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

fn parse_port(bytes: &[u8], start: usize, end: usize) -> Result<u16, ParseError> {
    if start == end {
        return Err(ParseError::InvalidPort { at: start });
    }
    for i in start..end {
        if !bytes[i].is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: i });
        }
    }
    if end - start >= 2 && bytes[start] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for &b in &bytes[start..end] {
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

fn validate_component(component: Component, text: &str, base: usize) -> Result<(), ParseError> {
    match decode_component(component, text) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: base + at }),
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