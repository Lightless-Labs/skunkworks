#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    UInt { tag: u32, value: u64 },
    Bytes { tag: u32, data: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StartOutOfBounds { offset: usize },
    Truncated { offset: usize, start: usize },
    Overflow { offset: usize, start: usize },
    LengthOutOfBounds { offset: usize, length: u64 },
    InvalidTag { offset: usize, tag: u64 },
    InvalidWire { offset: usize, wire: u8 },
    OutOfOrder { offset: usize, previous: u32, tag: u32 },
    Duplicate { offset: usize, tag: u32 },
}

fn encode_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn decode_varint(input: &[u8], mut pos: usize, end: usize) -> Result<(u64, usize), Error> {
    let start = pos;
    let mut value = 0u64;

    for index in 0..10 {
        if pos >= end {
            return Err(Error::Truncated { offset: end, start });
        }

        let byte = input[pos];
        let payload = byte & 0x7f;

        if index == 9 && (payload > 1 || byte & 0x80 != 0) {
            return Err(Error::Overflow { offset: pos, start });
        }

        value |= (payload as u64) << (index * 7);
        pos += 1;

        if byte & 0x80 == 0 {
            return Ok((value, pos));
        }
    }

    unreachable!()
}

pub fn encode_message(message: &Message) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    let mut previous_tag = None;
    let mut previous_was_bytes = false;

    for (index, field) in message.fields.iter().enumerate() {
        let (tag, is_bytes) = match field {
            Field::UInt { tag, .. } => (*tag, false),
            Field::Bytes { tag, .. } => (*tag, true),
        };

        if !(1..=15).contains(&tag) {
            return Err(Error::InvalidTag {
                offset: index,
                tag: tag as u64,
            });
        }

        if let Some(previous) = previous_tag {
            if tag < previous {
                return Err(Error::OutOfOrder {
                    offset: index,
                    previous,
                    tag,
                });
            }
            if tag == previous && !(previous_was_bytes && is_bytes) {
                return Err(Error::Duplicate { offset: index, tag });
            }
        }

        let wire = if is_bytes { 2 } else { 0 };
        encode_varint(((tag as u64) << 3) | wire, &mut body);

        match field {
            Field::UInt { value, .. } => encode_varint(*value, &mut body),
            Field::Bytes { data, .. } => {
                encode_varint(data.len() as u64, &mut body);
                body.extend_from_slice(data);
            }
        }

        previous_tag = Some(tag);
        previous_was_bytes = is_bytes;
    }

    let mut output = Vec::new();
    encode_varint(body.len() as u64, &mut output);
    output.extend_from_slice(&body);
    Ok(output)
}

pub fn decode_message(input: &[u8], start: usize) -> Result<(Message, usize), Error> {
    if start > input.len() {
        return Err(Error::StartOutOfBounds { offset: start });
    }

    let (body_length, body_start) = decode_varint(input, start, input.len())?;
    let remaining = input.len() - body_start;

    if body_length > remaining as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: start,
            length: body_length,
        });
    }

    let body_length = usize::try_from(body_length).map_err(|_| Error::LengthOutOfBounds {
        offset: start,
        length: body_length,
    })?;
    let body_end = body_start + body_length;
    let mut pos = body_start;
    let mut fields = Vec::new();
    let mut previous_tag: Option<u32> = None;
    let mut previous_wire: Option<u8> = None;

    while pos < body_end {
        let key_start = pos;
        let (key, after_key) = decode_varint(input, pos, body_end)?;
        pos = after_key;

        let tag_value = key >> 3;
        if !(1..=255).contains(&tag_value) {
            return Err(Error::InvalidTag {
                offset: key_start,
                tag: tag_value,
            });
        }

        let tag = tag_value as u32;
        let wire = (key & 7) as u8;

        if !matches!(wire, 0 | 1 | 2 | 5) {
            return Err(Error::InvalidWire {
                offset: key_start,
                wire,
            });
        }

        if let Some(previous) = previous_tag {
            if tag < previous {
                return Err(Error::OutOfOrder {
                    offset: key_start,
                    previous,
                    tag,
                });
            }
            if tag == previous && !(previous_wire == Some(2) && wire == 2) {
                return Err(Error::Duplicate {
                    offset: key_start,
                    tag,
                });
            }
        }

        let retained = tag <= 15;
        match wire {
            0 => {
                let (value, after_value) = decode_varint(input, pos, body_end)?;
                pos = after_value;
                if retained {
                    fields.push(Field::UInt { tag, value });
                }
            }
            1 | 5 => {
                let width = if wire == 1 { 8usize } else { 4usize };
                if body_end - pos < width {
                    return Err(Error::LengthOutOfBounds {
                        offset: key_start,
                        length: width as u64,
                    });
                }
                pos += width;
            }
            2 => {
                let length_start = pos;
                let (length, after_length) = decode_varint(input, pos, body_end)?;
                pos = after_length;

                let available = body_end - pos;
                if length > available as u64 {
                    return Err(Error::LengthOutOfBounds {
                        offset: length_start,
                        length,
                    });
                }

                let length = usize::try_from(length).map_err(|_| Error::LengthOutOfBounds {
                    offset: length_start,
                    length,
                })?;

                if retained {
                    if previous_tag == Some(tag) && previous_wire == Some(2) {
                        if let Some(Field::Bytes {
                            tag: last_tag,
                            data,
                        }) = fields.last_mut()
                        {
                            if *last_tag == tag {
                                data.extend_from_slice(&input[pos..pos + length]);
                            } else {
                                fields.push(Field::Bytes {
                                    tag,
                                    data: input[pos..pos + length].to_vec(),
                                });
                            }
                        } else {
                            fields.push(Field::Bytes {
                                tag,
                                data: input[pos..pos + length].to_vec(),
                            });
                        }
                    } else {
                        fields.push(Field::Bytes {
                            tag,
                            data: input[pos..pos + length].to_vec(),
                        });
                    }
                }

                pos += length;
            }
            _ => unreachable!(),
        }

        previous_tag = Some(tag);
        previous_wire = Some(wire);
    }

    Ok((Message { fields }, body_end))
}

fn main() {}