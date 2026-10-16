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
    let mut origins = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(CodecError::BadEscape { at: i });
            }
            match (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                (Some(high), Some(low)) => {
                    decoded.push((high << 4) | low);
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
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            let offset = error.valid_up_to();
            Err(CodecError::InvalidUtf8 {
                at: origins.get(offset).copied().unwrap_or(input.len()),
            })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for &byte in text.as_bytes() {
        if is_safe_byte(component, byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

pub fn parse_url(input: &str) -> Result<Url, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }

    let bytes = input.as_bytes();
    for (at, &byte) in bytes.iter().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at });
        }
    }

    let colon = bytes
        .iter()
        .position(|&byte| byte == b':')
        .ok_or(ParseError::InvalidScheme { at: input.len() })?;

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for at in 1..colon {
        let byte = bytes[at];
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at });
        }
    }

    if bytes.get(colon + 1) != Some(&b'/') || bytes.get(colon + 2) != Some(&b'/') {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&byte| matches!(byte, b'/' | b'?' | b'#'))
        .map(|offset| authority_start + offset)
        .unwrap_or(bytes.len());
    let authority = &input[authority_start..authority_end];
    let authority_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(index, &byte)| (byte == b'@').then_some(index))
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
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let hostport = &authority[hostport_start..];
    let hostport_bytes = hostport.as_bytes();
    let host_start = authority_start + hostport_start;

    let (host, port, ipv6) = if hostport_bytes.first() == Some(&b'[') {
        let close = match hostport_bytes.iter().position(|&byte| byte == b']') {
            Some(position) => position,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };

        let interior = &hostport[1..close];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }

        let suffix_start = close + 1;
        let port = if suffix_start == hostport.len() {
            None
        } else if hostport_bytes[suffix_start] == b':' {
            Some(parse_port(
                &hostport[suffix_start + 1..],
                host_start + suffix_start + 1,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + suffix_start,
            });
        };

        (interior.to_ascii_lowercase(), port, true)
    } else {
        let split = hostport_bytes.iter().position(|&byte| byte == b':');
        let (host_text, port) = match split {
            Some(position) => (
                &hostport[..position],
                Some(parse_port(
                    &hostport[position + 1..],
                    host_start + position + 1,
                )?),
            ),
            None => (hostport, None),
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (offset, &byte) in host_text.as_bytes().iter().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_start + offset,
                });
            }
        }

        (host_text.to_ascii_lowercase(), port, false)
    };

    let fragment_mark = bytes[authority_end..]
        .iter()
        .position(|&byte| byte == b'#')
        .map(|offset| authority_end + offset);

    let component_end = fragment_mark.unwrap_or(bytes.len());
    let query_mark = bytes[authority_end..component_end]
        .iter()
        .position(|&byte| byte == b'?')
        .map(|offset| authority_end + offset);

    let path_end = query_mark.unwrap_or(component_end);
    let path = if authority_end == path_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };
    let query = query_mark.map(|mark| input[mark + 1..component_end].to_owned());
    let fragment = fragment_mark.map(|mark| input[mark + 1..].to_owned());

    if let Some(userinfo_text) = userinfo.as_ref() {
        validate_component(userinfo_text, authority_start)?;
    }
    if authority_end != path_end {
        validate_component(&path, authority_end)?;
    }
    if let (Some(mark), Some(query_text)) = (query_mark, query.as_ref()) {
        validate_component(query_text, mark + 1)?;
    }
    if let (Some(mark), Some(fragment_text)) = (fragment_mark, fragment.as_ref()) {
        validate_component(fragment_text, mark + 1)?;
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

fn parse_port(text: &str, start: usize) -> Result<u16, ParseError> {
    if text.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }

    for (offset, &byte) in text.as_bytes().iter().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: start + offset });
        }
    }

    let digits = text.as_bytes();
    if digits.len() >= 2 && digits[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for &digit in digits {
        value = match value
            .checked_mul(10)
            .and_then(|current| current.checked_add((digit - b'0') as u32))
        {
            Some(value) if value <= 65535 => value,
            _ => return Err(ParseError::InvalidPort { at: start }),
        };
    }

    if value == 0 {
        return Err(ParseError::InvalidPort { at: start });
    }
    Ok(value as u16)
}

fn validate_component(text: &str, base: usize) -> Result<(), ParseError> {
    match decode_component(Component::Path, text) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: base + at }),
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_safe_byte(component: Component, byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(byte, b'-' | b'.' | b'_' | b'~')
        || match component {
            Component::UserInfo => byte == b':',
            Component::Path => byte == b'/',
            Component::Query => matches!(byte, b'/' | b'?'),
            Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
        }
}