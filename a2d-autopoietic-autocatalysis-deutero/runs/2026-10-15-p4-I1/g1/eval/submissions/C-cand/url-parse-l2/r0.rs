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
    let source = input.as_bytes();
    let mut decoded = Vec::with_capacity(source.len());
    let mut offsets = Vec::with_capacity(source.len());
    let mut i = 0;

    while i < source.len() {
        if source[i] == b'%' {
            if i + 2 >= source.len() {
                return Err(CodecError::BadEscape { at: i });
            }
            let hi = hex_value(source[i + 1]);
            let lo = hex_value(source[i + 2]);
            match (hi, lo) {
                (Some(hi), Some(lo)) => {
                    decoded.push((hi << 4) | lo);
                    offsets.push(i);
                    i += 3;
                }
                _ => return Err(CodecError::BadEscape { at: i }),
            }
        } else {
            decoded.push(source[i]);
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

fn is_scheme_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
}

fn is_scheme_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')
}

fn invalid_component_error(error: CodecError, base: usize) -> ParseError {
    match error {
        CodecError::BadEscape { at } => ParseError::BadEscape { at: base + at },
        CodecError::InvalidUtf8 { at } => ParseError::InvalidUtf8 { at: base + at },
    }
}

fn parse_port(port: &str, port_start: usize) -> Result<u16, ParseError> {
    if port.is_empty() {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let bytes = port.as_bytes();
    for (i, &byte) in bytes.iter().enumerate() {
        if !byte.is_ascii_digit() {
            return Err(ParseError::InvalidPort {
                at: port_start + i,
            });
        }
    }

    if bytes.len() >= 2 && bytes[0] == b'0' {
        return Err(ParseError::InvalidPort { at: port_start });
    }

    let mut value: u32 = 0;
    for &byte in bytes {
        value = match value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
        {
            Some(v) => v,
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
    for (i, &byte) in bytes.iter().enumerate() {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(ParseError::InvalidByte { at: i });
        }
    }

    let colon = match bytes.iter().position(|&byte| byte == b':') {
        Some(at) => at,
        None => return Err(ParseError::InvalidScheme { at: input.len() }),
    };

    if colon == 0 || !is_scheme_start(bytes[0]) {
        return Err(ParseError::InvalidScheme { at: 0 });
    }
    for (i, &byte) in bytes[..colon].iter().enumerate().skip(1) {
        if !is_scheme_byte(byte) {
            return Err(ParseError::InvalidScheme { at: i });
        }
    }

    let authority_start = colon + 1;
    if bytes.get(authority_start) != Some(&b'/')
        || bytes.get(authority_start + 1) != Some(&b'/')
    {
        return Err(ParseError::MissingAuthority { at: authority_start });
    }

    let authority_start = authority_start + 2;
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
        .filter_map(|(i, &byte)| (byte == b'@').then_some(i))
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
        (
            Some(&authority[..at]),
            at + 1,
        )
    } else {
        (None, 0)
    };

    let hostport = &authority[hostport_start..];
    let hostport_absolute_start = authority_start + hostport_start;

    let (host, port, ipv6) = if hostport.as_bytes().first() == Some(&b'[') {
        let opening = hostport_absolute_start;
        let close = match hostport.as_bytes()[1..]
            .iter()
            .position(|&byte| byte == b']')
        {
            Some(offset) => offset + 1,
            None => return Err(ParseError::InvalidIpv6 { at: opening }),
        };

        let interior = &hostport[1..close];
        if interior.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ParseError::InvalidIpv6 { at: opening });
        }

        let suffix = &hostport[close + 1..];
        let parsed_port = if suffix.is_empty() {
            None
        } else if suffix.as_bytes()[0] == b':' {
            let port_start = hostport_absolute_start + close + 2;
            Some(parse_port(&suffix[1..], port_start)?)
        } else {
            return Err(ParseError::InvalidHost {
                at: hostport_absolute_start + close + 1,
            });
        };

        (interior.to_ascii_lowercase(), parsed_port, true)
    } else {
        let hostport_bytes = hostport.as_bytes();
        let split = hostport_bytes.iter().position(|&byte| byte == b':');
        let (host, parsed_port) = match split {
            Some(colon_at) => {
                let port_start = hostport_absolute_start + colon_at + 1;
                (
                    &hostport[..colon_at],
                    Some(parse_port(&hostport[colon_at + 1..], port_start)?),
                )
            }
            None => (hostport, None),
        };

        if host.is_empty() {
            return Err(ParseError::InvalidHost {
                at: hostport_absolute_start,
            });
        }
        for (i, &byte) in host.as_bytes().iter().enumerate() {
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
                return Err(ParseError::InvalidHost {
                    at: hostport_absolute_start + i,
                });
            }
        }

        (host.to_ascii_lowercase(), parsed_port, false)
    };

    let after_authority = &input[authority_end..];
    let fragment_pos = after_authority.find('#');
    let before_fragment = fragment_pos
        .map(|at| &after_authority[..at])
        .unwrap_or(after_authority);
    let fragment = fragment_pos.map(|at| &after_authority[at + 1..]);

    let query_pos = before_fragment.find('?');
    let path = query_pos
        .map(|at| &before_fragment[..at])
        .unwrap_or(before_fragment);
    let query = query_pos.map(|at| &before_fragment[at + 1..]);

    let path_absolute_start = authority_end;
    let query_absolute_start = query_pos.map(|at| authority_end + at + 1);
    let fragment_absolute_start = fragment_pos.map(|at| authority_end + at + 1);

    if let Some(userinfo_text) = userinfo {
        let userinfo_start = authority_start;
        decode_component(Component::UserInfo, userinfo_text)
            .map_err(|error| invalid_component_error(error, userinfo_start))?;
    }

    if !path.is_empty() {
        decode_component(Component::Path, path)
            .map_err(|error| invalid_component_error(error, path_absolute_start))?;
    }

    if let Some(query_text) = query {
        let base = query_absolute_start.unwrap_or(authority_end);
        decode_component(Component::Query, query_text)
            .map_err(|error| invalid_component_error(error, base))?;
    }

    if let Some(fragment_text) = fragment {
        let base = fragment_absolute_start.unwrap_or(authority_end);
        decode_component(Component::Fragment, fragment_text)
            .map_err(|error| invalid_component_error(error, base))?;
    }

    let scheme = input[..colon].to_ascii_lowercase();

    Ok(Url {
        scheme,
        userinfo: userinfo.map(str::to_owned),
        host,
        port,
        path: if path.is_empty() {
            "/".to_owned()
        } else {
            path.to_owned()
        },
        query: query.map(str::to_owned),
        fragment: fragment.map(str::to_owned),
        ipv6,
    })
}

fn main() {}