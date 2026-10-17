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

pub fn decode_component(_component: Component, input: &str) -> Result<String, CodecError> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut offsets = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() || !is_hex(bytes[i + 1]) || !is_hex(bytes[i + 2]) {
                return Err(CodecError::BadEscape { at: i });
            }
            decoded.push((hex_value(bytes[i + 1]) << 4) | hex_value(bytes[i + 2]));
            offsets.push(i);
            i += 3;
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
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn validate_component(
    component: Component,
    raw: &str,
    base: usize,
) -> Result<(), ParseError> {
    match decode_component(component, raw) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: base + at }),
    }
}

fn parse_port(port: &str, port_start: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    if let Some((i, _)) = port.bytes().enumerate().find(|(_, b)| !b.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: port_start + i });
    }

    let bytes = port.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for &b in bytes {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((b - b'0') as u32)) {
            Some(v) if v <= 65535 => v,
            _ => return Err(ParseError::InvalidPort { at: port_start }),
        };
    }
    if value == 0 {
        return Err(ParseError::InvalidPort { at: port_start });
    }
    Ok(value as u16)
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

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let tail = &input[authority_start..];
    let authority_len = tail
        .bytes()
        .position(|b| matches!(b, b'/' | b'?' | b'#'))
        .unwrap_or(tail.len());
    let authority_end = authority_start + authority_len;
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

    let (userinfo, hostport_start) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (Some(&authority[..at]), at + 1)
    } else {
        (None, 0)
    };

    let hostport = &authority[hostport_start..];
    let hostport_abs = authority_start + hostport_start;

    let (host, port, ipv6) = if hostport.as_bytes().first() == Some(&b'[') {
        let open_abs = hostport_abs;
        let close = match hostport.as_bytes().iter().position(|&b| b == b']') {
            Some(i) => i,
            None => return Err(ParseError::InvalidIpv6 { at: open_abs }),
        };
        let interior = &hostport[1..close];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: open_abs });
        }
        let suffix = &hostport[close + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            Some(parse_port(&suffix[1..], hostport_abs + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_abs + close + 1,
            });
        };
        (interior.to_ascii_lowercase(), parsed_port, true)
    } else {
        let split = hostport.as_bytes().iter().position(|&b| b == b':');
        let (raw_host, parsed_port) = if let Some(i) = split {
            (&hostport[..i], Some(parse_port(&hostport[i + 1..], hostport_abs + i + 1)?))
        } else {
            (hostport, None)
        };

        if raw_host.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_abs });
        }
        for (i, b) in raw_host.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost { at: hostport_abs + i });
            }
        }
        (raw_host.to_ascii_lowercase(), parsed_port, false)
    };

    let after_authority = &input[authority_end..];
    let fragment_rel = after_authority.find('#');
    let before_fragment = fragment_rel
        .map(|i| &after_authority[..i])
        .unwrap_or(after_authority);
    let fragment = fragment_rel.map(|i| &after_authority[i + 1..]);

    let query_rel = before_fragment.find('?');
    let path_raw = query_rel
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment);
    let query = query_rel.map(|i| &before_fragment[i + 1..]);

    let path = if path_raw.is_empty() { "/" } else { path_raw };

    if let Some(raw) = userinfo {
        validate_component(Component::UserInfo, raw, authority_start)?;
    }
    if !path_raw.is_empty() {
        validate_component(Component::Path, path, authority_end)?;
    }
    if let Some(raw) = query {
        let base = authority_end + query_rel.unwrap() + 1;
        validate_component(Component::Query, raw, base)?;
    }
    if let Some(raw) = fragment {
        let base = authority_end + fragment_rel.unwrap() + 1;
        validate_component(Component::Fragment, raw, base)?;
    }

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo: userinfo.map(str::to_owned),
        host,
        port,
        path: path.to_owned(),
        query: query.map(str::to_owned),
        fragment: fragment.map(str::to_owned),
        ipv6,
    })
}