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
        Err(err) => {
            let decoded_at = err.valid_up_to();
            let input_at = offsets.get(decoded_at).copied().unwrap_or(input.len());
            Err(CodecError::InvalidUtf8 { at: input_at })
        }
    }
}

pub fn encode_component(component: Component, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
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
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }

    out
}

fn is_scheme_alpha(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
}

fn is_host_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
}

fn parse_port(port: &str, port_start: usize) -> Result<u16, ParseError> {
    let bytes = port.as_bytes();
    if bytes.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    for (i, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort { at: port_start + i });
        }
    }

    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for &byte in bytes {
        value = match value.checked_mul(10).and_then(|v| v.checked_add((byte - b'0') as u32)) {
            Some(v) => v,
            None => return Err(ParseError::InvalidPort { at: port_start }),
        };
        if value > u16::MAX as u32 {
            return Err(ParseError::InvalidPort { at: port_start });
        }
    }

    if value == 0 {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    Ok(value as u16)
}

fn validate_component(component: Component, raw: &str, start: usize) -> Result<(), ParseError> {
    match decode_component(component, raw) {
        Ok(_) => Ok(()),
        Err(CodecError::BadEscape { at }) => Err(ParseError::BadEscape { at: start + at }),
        Err(CodecError::InvalidUtf8 { at }) => Err(ParseError::InvalidUtf8 { at: start + at }),
    }
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
        Some(pos) => pos,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !is_scheme_alpha(bytes[0]) {
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
        .position(|&b| matches!(b, b'/' | b'?' | b'#'))
        .map(|offset| authority_start + offset)
        .unwrap_or(input.len());
    let authority = &input[authority_start..authority_end];
    let auth_bytes = authority.as_bytes();

    let at_positions: Vec<usize> = auth_bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
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
        let close_rel = match hostport.as_bytes()[1..].iter().position(|&b| b == b']') {
            Some(pos) => pos + 1,
            None => return Err(ParseError::InvalidIpv6 { at: open_abs }),
        };

        let interior = &hostport[1..close_rel];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: open_abs });
        }

        let suffix = &hostport[close_rel + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            let port_start = hostport_abs + close_rel + 2;
            Some(parse_port(&suffix[1..], port_start)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_abs + close_rel + 1,
            });
        };

        (interior.to_ascii_lowercase(), parsed_port, true)
    } else {
        let host_colon = hostport.as_bytes().iter().position(|&b| b == b':');
        let (host_text, port_text, port_start) = match host_colon {
            Some(pos) => (
                &hostport[..pos],
                Some(&hostport[pos + 1..]),
                hostport_abs + pos + 1,
            ),
            None => (hostport, None, 0),
        };

        if host_text.is_empty() {
            return Err(ParseError::InvalidHost { at: hostport_abs });
        }

        for (i, &byte) in host_text.as_bytes().iter().enumerate() {
            if !is_host_byte(byte) {
                return Err(ParseError::InvalidHost {
                    at: hostport_abs + i,
                });
            }
        }

        let parsed_port = match port_text {
            Some(text) => Some(parse_port(text, port_start)?),
            None => None,
        };
        (host_text.to_ascii_lowercase(), parsed_port, false)
    };

    let remainder = &input[authority_end..];
    let fragment_rel = remainder.find('#');
    let before_fragment = fragment_rel.map(|pos| &remainder[..pos]).unwrap_or(remainder);
    let fragment = fragment_rel.map(|pos| &remainder[pos + 1..]);

    let query_rel = before_fragment.find('?');
    let path_raw = query_rel
        .map(|pos| &before_fragment[..pos])
        .unwrap_or(before_fragment);
    let query = query_rel.map(|pos| &before_fragment[pos + 1..]);

    let path_start = authority_end;
    let query_start = query_rel.map(|pos| authority_end + pos + 1);
    let fragment_start = fragment_rel.map(|pos| authority_end + pos + 1);

    if let Some(raw) = userinfo {
        validate_component(Component::UserInfo, raw, authority_start)?;
    }
    if !path_raw.is_empty() {
        validate_component(Component::Path, path_raw, path_start)?;
    }
    if let (Some(raw), Some(start)) = (query, query_start) {
        validate_component(Component::Query, raw, start)?;
    }
    if let (Some(raw), Some(start)) = (fragment, fragment_start) {
        validate_component(Component::Fragment, raw, start)?;
    }

    Ok(Url {
        scheme: input[..colon].to_ascii_lowercase(),
        userinfo: userinfo.map(str::to_owned),
        host,
        port,
        path: if path_raw.is_empty() {
            "/".to_owned()
        } else {
            path_raw.to_owned()
        },
        query: query.map(str::to_owned),
        fragment: fragment.map(str::to_owned),
        ipv6,
    })
}

fn main() {}