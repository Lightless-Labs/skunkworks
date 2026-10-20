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
        Err(e) => Err(CodecError::InvalidUtf8 {
            at: offsets.get(e.valid_up_to()).copied().unwrap_or(input.len()),
        }),
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
    for i in 1..colon {
        let b = bytes[i];
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

    if at_positions.len() > 1 {
        return Err(ParseError::InvalidUserInfo {
            at: auth_start + at_positions[1],
        });
    }

    let (userinfo, host_start) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo { at: auth_start });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let host_port = &authority[host_start..];
    let absolute_host_start = auth_start + host_start;

    let (host, port, ipv6) = if host_port.starts_with('[') {
        let close = match host_port.as_bytes()[1..].iter().position(|&b| b == b']') {
            Some(n) => n + 1,
            None => return Err(ParseError::InvalidIpv6 { at: absolute_host_start }),
        };

        let interior = &host_port[1..close];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: absolute_host_start });
        }

        let suffix = &host_port[close + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(&suffix[1..], absolute_host_start + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: absolute_host_start + close + 1,
            });
        };

        (interior.to_ascii_lowercase(), parsed_port, true)
    } else {
        let split = host_port.find(':');
        let (host_text, port_text) = match split {
            Some(i) => (&host_port[..i], Some(&host_port[i + 1..])),
            None => (host_port, None),
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: absolute_host_start });
        }
        for (i, b) in host_text.bytes().enumerate() {
            if !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: absolute_host_start + i,
                });
            }
        }

        let parsed_port = match (split, port_text) {
            (Some(i), Some(text)) => Some(parse_port(text, absolute_host_start + i + 1)?),
            _ => None,
        };
        (host_text.to_ascii_lowercase(), parsed_port, false)
    };

    let remainder = &input[auth_end..];
    let (before_fragment, fragment) = match remainder.find('#') {
        Some(i) => (&remainder[..i], Some(&remainder[i + 1..])),
        None => (remainder, None),
    };

    let (path_query, query) = match before_fragment.find('?') {
        Some(i) => (&before_fragment[..i], Some(&before_fragment[i + 1..])),
        None => (before_fragment, None),
    };

    let path = if path_query.is_empty() {
        "/".to_owned()
    } else {
        path_query.to_owned()
    };

    let mut url = Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host,
        port,
        path,
        query: query.map(str::to_owned),
        fragment: fragment.map(str::to_owned),
        ipv6,
    };

    if let Some(ref value) = url.userinfo {
        validate_component(Component::UserInfo, value, auth_start)?;
    }
    if !path_query.is_empty() {
        validate_component(Component::Path, &url.path, auth_end)?;
    }
    if let Some(value) = query {
        validate_component(Component::Query, value, auth_end + path_query.len() + 1)?;
    }
    if let Some(value) = fragment {
        let fragment_offset = auth_end + remainder.find('#').unwrap_or(0) + 1;
        validate_component(Component::Fragment, value, fragment_offset)?;
    }

    Ok(url)
}

fn validate_component(component: Component, value: &str, base: usize) -> Result<(), ParseError> {
    match decode_component(component, value) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: base + at }),
    }
}

fn parse_port(text: &str, start: usize) -> Result<u16, ParseError> {
    if text.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }

    for (i, b) in text.bytes().enumerate() {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: start + i });
        }
    }

    let bytes = text.as_bytes();
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

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}