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

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
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
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            let decoded_at = error.valid_up_to();
            let original_at = offsets.get(decoded_at).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_at })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut output = String::with_capacity(text.len());
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
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }

    output
}

fn validate_component(
    component: Component,
    value: &str,
    base: usize,
) -> Result<(), ParseError> {
    match decode_component(component, value) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: base + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: base + at }),
    }
}

fn parse_port(port: &str, start: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: start });
    }

    if let Some(index) = port.bytes().position(|byte| !byte.is_ascii_digit()) {
        return Err(ParseError::InvalidPort { at: start + index });
    }

    let bytes = port.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut value: u32 = 0;
    for &byte in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|number| number.checked_add((byte - b'0') as u32))
        {
            Some(number) => number,
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

    for (at, byte) in input.bytes().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at });
        }
    }

    let bytes = input.as_bytes();
    let colon = match bytes.iter().position(|&byte| byte == b':') {
        Some(index) => index,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (index, &byte) in bytes[..colon].iter().enumerate().skip(1) {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            return Err(ParseError::InvalidScheme { at: index });
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
        .unwrap_or(input.len());
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

    let (userinfo, host_start_in_authority) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (Some(&authority[..at]), at + 1)
    } else {
        (None, 0)
    };

    let host_start = authority_start + host_start_in_authority;
    let host_and_port = &authority[host_start_in_authority..];
    let host_and_port_bytes = host_and_port.as_bytes();

    let mut ipv6 = false;
    let (host, port) = if host_and_port_bytes.first() == Some(&b'[') {
        let close = host_and_port_bytes[1..]
            .iter()
            .position(|&byte| byte == b']')
            .map(|offset| offset + 1);
        let close = match close {
            Some(index) => index,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };

        let interior = &host_and_port[1..close];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }

        let suffix = &host_and_port[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            Some(parse_port(&suffix[1..], host_start + close + 2)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close + 1,
            });
        };

        ipv6 = true;
        (interior.to_ascii_lowercase(), port)
    } else {
        let split = host_and_port_bytes.iter().position(|&byte| byte == b':');
        let (host_part, port) = match split {
            Some(index) => (
                &host_and_port[..index],
                Some(parse_port(
                    &host_and_port[index + 1..],
                    host_start + index + 1,
                )?),
            ),
            None => (host_and_port, None),
        };

        if host_part.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        if let Some(index) = host_part
            .bytes()
            .position(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')))
        {
            return Err(ParseError::InvalidHost {
                at: host_start + index,
            });
        }

        (host_part.to_ascii_lowercase(), port)
    };

    if host.is_empty() {
        return Err(ParseError::InvalidHost { at: host_start });
    }

    let after_authority = &input[authority_end..];
    let fragment_offset = after_authority.find('#');
    let before_fragment = fragment_offset
        .map(|index| &after_authority[..index])
        .unwrap_or(after_authority);
    let fragment = fragment_offset.map(|index| &after_authority[index + 1..]);

    let query_offset = before_fragment.find('?');
    let path_raw = query_offset
        .map(|index| &before_fragment[..index])
        .unwrap_or(before_fragment);
    let query = query_offset.map(|index| &before_fragment[index + 1..]);

    if let Some(userinfo_value) = userinfo {
        validate_component(
            Component::UserInfo,
            userinfo_value,
            authority_start,
        )?;
    }

    let path = if path_raw.is_empty() {
        "/".to_owned()
    } else {
        validate_component(Component::Path, path_raw, authority_end)?;
        path_raw.to_owned()
    };

    if let Some(query_value) = query {
        let query_start = authority_end + query_offset.unwrap() + 1;
        validate_component(Component::Query, query_value, query_start)?;
    }

    if let Some(fragment_value) = fragment {
        let fragment_start = authority_end + fragment_offset.unwrap() + 1;
        validate_component(Component::Fragment, fragment_value, fragment_start)?;
    }

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo: userinfo.map(str::to_owned),
        host,
        port,
        path,
        query: query.map(str::to_owned),
        fragment: fragment.map(str::to_owned),
        ipv6,
    })
}