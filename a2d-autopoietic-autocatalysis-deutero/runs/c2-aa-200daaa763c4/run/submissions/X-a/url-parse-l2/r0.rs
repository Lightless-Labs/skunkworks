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
    let mut offsets = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(CodecError::BadEscape { at: i });
            }
            let high = hex_value(bytes[i + 1]);
            let low = hex_value(bytes[i + 2]);
            match (high, low) {
                (Some(high), Some(low)) => {
                    decoded.push((high << 4) | low);
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
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            let decoded_offset = error.valid_up_to();
            let original_offset = offsets.get(decoded_offset).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_offset })
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

fn parse_port(value: &str, colon_at: usize) -> Result<u16, ParseError> {
    if value.is_empty() {
        return Err(ParseError::InvalidPort { at: colon_at + 1 });
    }

    if let Some(index) = value.bytes().position(|byte| !byte.is_ascii_digit()) {
        return Err(ParseError::InvalidPort {
            at: colon_at + 1 + index,
        });
    }

    let start = colon_at + 1;
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: start });
    }

    let mut number: u32 = 0;
    for byte in bytes {
        number = match number
            .checked_mul(10)
            .and_then(|number| number.checked_add((byte - b'0') as u32))
        {
            Some(number) => number,
            None => return Err(ParseError::InvalidPort { at: start }),
        };
        if number > u16::MAX as u32 {
            return Err(ParseError::InvalidPort { at: start });
        }
    }

    if number == 0 {
        return Err(ParseError::InvalidPort { at: start });
    }

    Ok(number as u16)
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
        Some(colon) => colon,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (at, &byte) in bytes[..colon].iter().enumerate().skip(1) {
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

    let (userinfo, host_start_index) = if let Some(&at) = at_positions.first() {
        if at == 0 {
            return Err(ParseError::InvalidUserInfo {
                at: authority_start + at,
            });
        }
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let host_start = authority_start + host_start_index;
    let host_port = &authority[host_start_index..];
    let host_port_bytes = host_port.as_bytes();

    let (host, port, ipv6) = if host_port_bytes.first() == Some(&b'[') {
        let closing = host_port_bytes.iter().position(|&byte| byte == b']');
        let closing = match closing {
            Some(index) => index,
            None => return Err(ParseError::InvalidIpv6 { at: host_start }),
        };
        let interior = &host_port[1..closing];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 { at: host_start });
        }

        let suffix_start = closing + 1;
        let suffix = &host_port[suffix_start..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            Some(parse_port(&suffix[1..], host_start + suffix_start)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + suffix_start,
            });
        };

        (interior.to_ascii_lowercase(), port, true)
    } else {
        let host_colon = host_port_bytes.iter().position(|&byte| byte == b':');
        let (host_text, port) = match host_colon {
            Some(index) => (
                &host_port[..index],
                Some(parse_port(&host_port[index + 1..], host_start + index)?),
            ),
            None => (host_port, None),
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (index, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_start + index,
                });
            }
        }

        (host_text.to_ascii_lowercase(), port, false)
    };

    let fragment_start = bytes[authority_end..]
        .iter()
        .position(|&byte| byte == b'#')
        .map(|offset| authority_end + offset);

    let before_fragment_end = fragment_start.unwrap_or(input.len());
    let query_start = bytes[authority_end..before_fragment_end]
        .iter()
        .position(|&byte| byte == b'?')
        .map(|offset| authority_end + offset);

    let path_end = query_start.unwrap_or(before_fragment_end);
    let path = if authority_end == path_end {
        "/".to_owned()
    } else {
        input[authority_end..path_end].to_owned()
    };

    let query = query_start.map(|start| input[start + 1..before_fragment_end].to_owned());
    let fragment = fragment_start.map(|start| input[start + 1..].to_owned());

    if let Some(value) = &userinfo {
        validate_component(Component::UserInfo, value, authority_start)?;
    }
    if path != "/" || authority_end != path_end {
        validate_component(Component::Path, &path, authority_end)?;
    }
    if let (Some(start), Some(value)) = (query_start, &query) {
        validate_component(Component::Query, value, start + 1)?;
    }
    if let (Some(start), Some(value)) = (fragment_start, &fragment) {
        validate_component(Component::Fragment, value, start + 1)?;
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