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

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
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
                (Some(hi), Some(lo)) => {
                    decoded.push((hi << 4) | lo);
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
            let decoded_offset = error.valid_up_to();
            let at = origins.get(decoded_offset).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for &byte in text.as_bytes() {
        let safe = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || match component {
                Component::UserInfo => byte == b':',
                Component::Path => byte == b'/',
                Component::Query => matches!(byte, b'/' | b'?'),
                Component::Fragment => matches!(byte, b'/' | b'?' | b'#'),
            };

        if safe {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }

    encoded
}

fn ascii_lowercase(text: &str) -> String {
    text.bytes()
        .map(|b| if b.is_ascii_uppercase() { (b + 32) as char } else { b as char })
        .collect()
}

fn validate_component(
    component: Component,
    text: &str,
    base: usize,
) -> Result<(), ParseError> {
    match decode_component(component, text) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: base + at }),
    }
}

fn parse_port(port_text: &str, port_start: usize) -> Result<u16, ParseError> {
    if port_text.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    if let Some((offset, _)) = port_text
        .bytes()
        .enumerate()
        .find(|(_, byte)| !byte.is_ascii_digit())
    {
        return Err(ParseError::InvalidPort {
            at: port_start + offset,
        });
    }

    let bytes = port_text.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for &byte in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|n| n.checked_add((byte - b'0') as u32))
        {
            Some(n) => n,
            None => return Err(ParseError::InvalidPort { at: port_start }),
        };
    }

    if !(1..=65535).contains(&value) {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    Ok(value as u16)
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

    let colon = match bytes.iter().position(|&byte| byte == b':') {
        Some(index) => index,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    let scheme_bytes = &bytes[..colon];
    if scheme_bytes.is_empty() || !scheme_bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (at, &byte) in scheme_bytes.iter().enumerate().skip(1) {
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
    if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
    }

    let (userinfo, hostport_start) = if let Some(&at) = at_positions.first() {
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let hostport = &authority[hostport_start..];
    let hostport_abs = authority_start + hostport_start;
    let mut ipv6 = false;
    let (host, port) = if hostport.as_bytes().first() == Some(&b'[') {
        let close = match hostport.as_bytes().iter().position(|&byte| byte == b']') {
            Some(index) => index,
            None => return Err(ParseError::InvalidIpv6 { at: hostport_abs }),
        };
        let interior = &hostport[1..close];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: hostport_abs });
        }
        ipv6 = true;

        let suffix = &hostport[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            Some(parse_port(&suffix[1..], hostport_abs + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_abs + close + 1,
            });
        };

        (ascii_lowercase(interior), port)
    } else {
        let hostport_bytes = hostport.as_bytes();
        let split = hostport_bytes.iter().position(|&byte| byte == b':');
        let host_end = split.unwrap_or(hostport.len());
        let host_text = &hostport[..host_end];

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_abs });
        }
        for (offset, &byte) in host_text.as_bytes().iter().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_abs + offset,
                });
            }
        }

        let port = if let Some(split) = split {
            Some(parse_port(
                &hostport[split + 1..],
                hostport_abs + split + 1,
            )?)
        } else {
            None
        };

        (ascii_lowercase(host_text), port)
    };

    let fragment_start = bytes[authority_end..]
        .iter()
        .position(|&byte| byte == b'#')
        .map(|offset| authority_end + offset);
    let before_fragment_end = fragment_start.unwrap_or(bytes.len());

    let query_start = bytes[authority_end..before_fragment_end]
        .iter()
        .position(|&byte| byte == b'?')
        .map(|offset| authority_end + offset);

    let component_end = query_start.unwrap_or(before_fragment_end);
    let path_text = &input[authority_end..component_end];
    let path = if path_text.is_empty() {
        "/".to_owned()
    } else {
        path_text.to_owned()
    };

    let query = query_start.map(|start| input[start + 1..before_fragment_end].to_owned());
    let fragment = fragment_start.map(|start| input[start + 1..].to_owned());

    if let Some(userinfo_text) = userinfo.as_deref() {
        validate_component(Component::UserInfo, userinfo_text, authority_start)?;
    }
    if !path_text.is_empty() {
        validate_component(Component::Path, path_text, authority_end)?;
    }
    if let Some(start) = query_start {
        validate_component(
            Component::Query,
            &input[start + 1..before_fragment_end],
            start + 1,
        )?;
    }
    if let Some(start) = fragment_start {
        validate_component(Component::Fragment, &input[start + 1..], start + 1)?;
    }

    Ok(Url {
        scheme: ascii_lowercase(&input[..colon]),
        userinfo,
        host,
        port,
        path,
        query,
        fragment,
        ipv6,
    })
}