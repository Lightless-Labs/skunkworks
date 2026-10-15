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
    let mut source_offsets = Vec::with_capacity(bytes.len());

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
                    source_offsets.push(i);
                    i += 3;
                }
                _ => return Err(CodecError::BadEscape { at: i }),
            }
        } else {
            decoded.push(bytes[i]);
            source_offsets.push(i);
            i += 1;
        }
    }

    match std::str::from_utf8(&decoded) {
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            let decoded_offset = error.valid_up_to();
            let source_offset = source_offsets
                .get(decoded_offset)
                .copied()
                .unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: source_offset })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut output = String::with_capacity(text.len());

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
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }

    output
}

fn validate_component(text: &str, base: usize) -> Result<(), ParseError> {
    match decode_component(Component::Path, text) {
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

    if port_text.len() >= 2 && port_text.as_bytes()[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for byte in port_text.bytes() {
        value = match value
            .checked_mul(10)
            .and_then(|value| value.checked_add((byte - b'0') as u32))
        {
            Some(value) => value,
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
        .position(|byte| matches!(*byte, b'/' | b'?' | b'#'))
        .map(|offset| authority_start + offset)
        .unwrap_or(input.len());

    let authority = &input[authority_start..authority_end];
    let authority_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(offset, byte)| (*byte == b'@').then_some(offset))
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
        (Some(authority[..at].to_owned()), at + 1)
    } else {
        (None, 0)
    };

    let host_port = &authority[host_start_in_authority..];
    let host_port_start = authority_start + host_start_in_authority;

    let (host, port, ipv6) = if host_port.starts_with('[') {
        let closing = host_port.as_bytes()[1..]
            .iter()
            .position(|&byte| byte == b']')
            .map(|offset| offset + 1);

        let closing = match closing {
            Some(index) => index,
            None => {
                return Err(ParseError::InvalidIpv6 {
                    at: host_port_start,
                });
            }
        };

        let interior = &host_port[1..closing];
        if Ipv6Addr::from_str(interior).is_err() {
            return Err(ParseError::InvalidIpv6 {
                at: host_port_start,
            });
        }

        let suffix = &host_port[closing + 1..];
        let port = if suffix.is_empty() {
            None
        } else if suffix.starts_with(':') {
            Some(parse_port(
                &suffix[1..],
                host_port_start + closing + 2,
            )?)
        } else {
            return Err(ParseError::InvalidHost {
                at: host_port_start + closing + 1,
            });
        };

        (interior.to_ascii_lowercase(), port, true)
    } else {
        let host_port_bytes = host_port.as_bytes();
        let host_port_colon = host_port_bytes.iter().position(|&byte| byte == b':');
        let host_end = host_port_colon.unwrap_or(host_port.len());
        let host_text = &host_port[..host_end];

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost {
                at: host_port_start,
            });
        }

        for (offset, byte) in host_text.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_port_start + offset,
                });
            }
        }

        let port = match host_port_colon {
            Some(colon_offset) => Some(parse_port(
                &host_port[colon_offset + 1..],
                host_port_start + colon_offset + 1,
            )?),
            None => None,
        };

        (host_text.to_ascii_lowercase(), port, false)
    };

    let remainder = &input[authority_end..];
    let remainder_base = authority_end;

    let fragment_offset = remainder.as_bytes().iter().position(|&byte| byte == b'#');
    let before_fragment = fragment_offset
        .map(|offset| &remainder[..offset])
        .unwrap_or(remainder);

    let fragment = fragment_offset.map(|offset| remainder[offset + 1..].to_owned());

    let query_offset = before_fragment
        .as_bytes()
        .iter()
        .position(|&byte| byte == b'?');
    let path_text = query_offset
        .map(|offset| &before_fragment[..offset])
        .unwrap_or(before_fragment);

    let query = query_offset.map(|offset| before_fragment[offset + 1..].to_owned());

    let path = if path_text.is_empty() {
        "/".to_owned()
    } else {
        path_text.to_owned()
    };

    if let Some(userinfo_text) = userinfo.as_deref() {
        validate_component(userinfo_text, authority_start)?;
    }
    if !path_text.is_empty() {
        validate_component(path_text, remainder_base)?;
    }
    if let Some(offset) = query_offset {
        validate_component(&before_fragment[offset + 1..], remainder_base + offset + 1)?;
    }
    if let Some(offset) = fragment_offset {
        validate_component(&remainder[offset + 1..], remainder_base + offset + 1)?;
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

fn main() {}