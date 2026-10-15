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
            let Some(high) = hex_value(bytes[i + 1]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            let Some(low) = hex_value(bytes[i + 2]) else {
                return Err(CodecError::BadEscape { at: i });
            };
            decoded.push((high << 4) | low);
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
            let decoded_offset = error.valid_up_to();
            let original_offset = offsets.get(decoded_offset).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: original_offset })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if is_safe(byte)
            || matches!(
                (component, byte),
                (Component::UserInfo, b':')
                    | (Component::Path, b'/')
                    | (Component::Query, b'/' | b'?')
                    | (Component::Fragment, b'/' | b'?' | b'#')
            )
        {
            result.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            result.push('%');
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    result
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
    let colon = match bytes.iter().position(|&b| b == b':') {
        Some(at) => at,
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

    if colon + 2 >= bytes.len()
        || bytes[colon + 1] != b'/'
        || bytes[colon + 2] != b'/'
    {
        return Err(ParseError::MissingAuthority { at: colon + 1 });
    }

    let authority_start = colon + 3;
    let authority_end = bytes[authority_start..]
        .iter()
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|offset| authority_start + offset)
        .unwrap_or(bytes.len());
    let authority = &input[authority_start..authority_end];
    let authority_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = authority_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();

    if at_positions.len() >= 2 {
        return Err(ParseError::InvalidUserInfo {
            at: authority_start + at_positions[1],
        });
    }
    if let Some(&userinfo_end) = at_positions.first() {
        if userinfo_end == 0 {
            return Err(ParseError::InvalidUserInfo { at: authority_start });
        }
    }

    let (userinfo, host_start_local) = if let Some(&userinfo_end) = at_positions.first() {
        (
            Some(authority[..userinfo_end].to_owned()),
            userinfo_end + 1,
        )
    } else {
        (None, 0)
    };
    let host_start = authority_start + host_start_local;
    let host_port = &authority[host_start_local..];

    let (host, port_text, port_start, ipv6) = if host_port.starts_with('[') {
        let open_at = host_start;
        let closing = host_port[1..].find(']').map(|i| i + 1);
        let Some(close_index) = closing else {
            return Err(ParseError::InvalidIpv6 { at: open_at });
        };
        let interior = &host_port[1..close_index];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: open_at });
        }

        let suffix = &host_port[close_index + 1..];
        let (port_text, port_start) = if suffix.is_empty() {
            (None, authority_end)
        } else if suffix.starts_with(':') {
            (
                Some(&suffix[1..]),
                host_start + close_index + 2,
            )
        } else {
            return Err(ParseError::InvalidHost {
                at: host_start + close_index + 1,
            });
        };
        (interior, port_text, port_start, true)
    } else {
        let split = host_port.as_bytes().iter().position(|&b| b == b':');
        if let Some(index) = split {
            (
                &host_port[..index],
                Some(&host_port[index + 1..]),
                host_start + index + 1,
                false,
            )
        } else {
            (host_port, None, authority_end, false)
        }
    };

    if !ipv6 {
        if host.is_empty() {
            return Err(ParseError::InvalidHost { at: host_start });
        }
        for (index, byte) in host.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: host_start + index,
                });
            }
        }
    }

    let port = if let Some(port_text) = port_text {
        if port_text.is_empty() {
            return Err(ParseError::InvalidPort { at: port_start });
        }
        if let Some(index) = port_text.bytes().position(|b| !b.is_ascii_digit()) {
            return Err(ParseError::InvalidPort {
                at: port_start + index,
            });
        }
        if port_text.len() >= 2 && port_text.starts_with('0') {
            return Err(ParseError::InvalidPort { at: port_start });
        }
        let mut value: u32 = 0;
        for byte in port_text.bytes() {
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
        Some(value as u16)
    } else {
        None
    };

    let tail = &input[authority_end..];
    let fragment_marker = tail.find('#');
    let before_fragment = fragment_marker.map(|i| &tail[..i]).unwrap_or(tail);
    let fragment = fragment_marker.map(|i| tail[i + 1..].to_owned());

    let query_marker = before_fragment.find('?');
    let path = query_marker
        .map(|i| &before_fragment[..i])
        .unwrap_or(before_fragment);
    let query = query_marker.map(|i| before_fragment[i + 1..].to_owned());
    let path = if path.is_empty() { "/" } else { path };

    validate_component(userinfo.as_deref(), Component::UserInfo, input, authority_start)
        .map_err(|error| rebase_codec_error(error, authority_start))?;

    let path_offset = authority_end;
    validate_component(Some(path), Component::Path, input, path_offset)
        .map_err(|error| rebase_codec_error(error, path_offset))?;

    let query_offset = query_marker.map(|i| authority_end + i + 1).unwrap_or(0);
    validate_component(query.as_deref(), Component::Query, input, query_offset)
        .map_err(|error| rebase_codec_error(error, query_offset))?;

    let fragment_offset = fragment_marker
        .map(|i| authority_end + i + 1)
        .unwrap_or(0);
    validate_component(
        fragment.as_deref(),
        Component::Fragment,
        input,
        fragment_offset,
    )
    .map_err(|error| rebase_codec_error(error, fragment_offset))?;

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo,
        host: host.to_ascii_lowercase(),
        port,
        path: path.to_owned(),
        query,
        fragment,
        ipv6,
    })
}

fn validate_component(
    component_text: Option<&str>,
    component: Component,
    _input: &str,
    _base: usize,
) -> Result<(), CodecError> {
    if let Some(text) = component_text {
        decode_component(component, text)?;
    }
    Ok(())
}

fn rebase_codec_error(error: CodecError, base: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: base + at },
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

fn is_safe(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn main() {}