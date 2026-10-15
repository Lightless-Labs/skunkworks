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

fn encode_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn decode_varint(
    input: &[u8],
    cursor: &mut usize,
    end: usize,
    start: usize,
) -> Result<u64, Error> {
    let mut value = 0u64;

    for index in 0..10 {
        if *cursor >= end {
            return Err(Error::Truncated { offset: end, start });
        }

        let offset = *cursor;
        let byte = input[offset];
        *cursor += 1;
        let payload = byte & 0x7f;

        if index == 9 && (payload > 1 || byte & 0x80 != 0) {
            return Err(Error::Overflow { offset, start });
        }

        value |= (payload as u64) << (index * 7);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }

    unreachable!()
}

pub fn encode_message(message: &Message) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    let mut previous: Option<(u32, bool)> = None;

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

        if let Some((previous_tag, previous_bytes)) = previous {
            if tag < previous_tag {
                return Err(Error::OutOfOrder {
                    offset: index,
                    previous: previous_tag,
                    tag,
                });
            }
            if tag == previous_tag && !(previous_bytes && is_bytes) {
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

        previous = Some((tag, is_bytes));
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

    let mut cursor = start;
    let header_start = start;
    let frame_length = decode_varint(input, &mut cursor, input.len(), header_start)?;

    let remaining = input.len() - cursor;
    if frame_length > remaining as u64 {
        return Err(Error::LengthOutOfBounds {
            offset: header_start,
            length: frame_length,
        });
    }

    let frame_end = cursor + frame_length as usize;
    let mut fields = Vec::new();
    let mut previous: Option<(u32, u8)> = None;

    while cursor < frame_end {
        let key_start = cursor;
        let key = decode_varint(input, &mut cursor, frame_end, key_start)?;
        let tag_value = key >> 3;
        let wire = (key & 7) as u8;

        if !(1..=255).contains(&tag_value) {
            return Err(Error::InvalidTag {
                offset: key_start,
                tag: tag_value,
            });
        }
        let tag = tag_value as u32;

        if !matches!(wire, 0 | 1 | 2 | 5) {
            return Err(Error::InvalidWire {
                offset: key_start,
                wire,
            });
        }

        if let Some((previous_tag, previous_wire)) = previous {
            if tag < previous_tag {
                return Err(Error::OutOfOrder {
                    offset: key_start,
                    previous: previous_tag,
                    tag,
                });
            }
            if tag == previous_tag && !(previous_wire == 2 && wire == 2) {
                return Err(Error::Duplicate {
                    offset: key_start,
                    tag,
                });
            }
        }

        match wire {
            0 => {
                let value_start = cursor;
                let value = decode_varint(input, &mut cursor, frame_end, value_start)?;
                if tag <= 15 {
                    fields.push(Field::UInt { tag, value });
                }
            }
            1 | 5 => {
                let length = if wire == 1 { 8usize } else { 4usize };
                let remaining = frame_end - cursor;
                if remaining < length {
                    return Err(Error::LengthOutOfBounds {
                        offset: key_start,
                        length: length as u64,
                    });
                }
                cursor += length;
            }
            2 => {
                let length_start = cursor;
                let data_length = decode_varint(input, &mut cursor, frame_end, length_start)?;
                let remaining = frame_end - cursor;
                if data_length > remaining as u64 {
                    return Err(Error::LengthOutOfBounds {
                        offset: length_start,
                        length: data_length,
                    });
                }
                let data_length = data_length as usize;
                let data_end = cursor + data_length;

                if tag <= 15 {
                    if let Some(Field::Bytes { tag: last_tag, data }) = fields.last_mut() {
                        if *last_tag == tag {
                            data.extend_from_slice(&input[cursor..data_end]);
                        } else {
                            fields.push(Field::Bytes {
                                tag,
                                data: input[cursor..data_end].to_vec(),
                            });
                        }
                    } else {
                        fields.push(Field::Bytes {
                            tag,
                            data: input[cursor..data_end].to_vec(),
                        });
                    }
                }

                cursor = data_end;
            }
            _ => unreachable!(),
        }

        previous = Some((tag, wire));
    }

    Ok((Message { fields }, frame_end))
}

fn main() {}