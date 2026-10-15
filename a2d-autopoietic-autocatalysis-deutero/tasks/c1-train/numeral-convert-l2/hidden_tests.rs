// R2, R24
#[test]
fn hidden_base_invalid_radices() {
assert_eq!(convert_base("", 1, 0, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::BadBase {which:0,value:1})); assert_eq!(convert_base("!", 10, 37, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::BadBase {which:1,value:37})); assert_eq!(add_base("", "", 255, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::BadBase {which:0,value:255}));
}

// R3, R24
#[test]
fn hidden_base_style_precedence() {
assert_eq!(convert_base("", 10, 10, BaseStyle {upper:false,group:9,min_width:0}), Err(BaseError::BadStyle)); assert_eq!(add_base("!", "?", 10, BaseStyle {upper:false,group:0,min_width:257}), Err(BaseError::BadStyle));
}

// R4, R5
#[test]
fn hidden_base_empty_operands() {
assert_eq!(convert_base("", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::Empty {operand:0})); assert_eq!(add_base("1", "", 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::Empty {operand:1}));
}

// R6-R8
#[test]
fn hidden_base_empty_tag_payload() {
assert_eq!(convert_base("10#", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::Empty {operand:0})); assert_eq!(add_base("1", "10#", 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::Empty {operand:1}));
}

// R6, R7, R24
#[test]
fn hidden_base_wrong_tag() {
assert_eq!(convert_base("010#?", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidTag {operand:0,index:0})); assert_eq!(convert_base("2#12", 3, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidTag {operand:0,index:0})); assert_eq!(convert_base("#", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidTag {operand:0,index:0}));
}

// R7, R9, R12
#[test]
fn hidden_base_duplicate_tag() {
assert_eq!(convert_base("10#1#2", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidDigit {operand:0,index:4}));
}

// R10-R12
#[test]
fn hidden_base_bad_digit() {
assert_eq!(convert_base("A", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidDigit {operand:0,index:0})); assert_eq!(convert_base("36#z!", 36, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidDigit {operand:0,index:4}));
}

// R9, R11, R12
#[test]
fn hidden_base_utf8_offsets() {
assert_eq!(convert_base("16#fñ", 16, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidDigit {operand:0,index:4}));
}

// R5, R11
#[test]
fn hidden_base_no_trimming_signs() {
assert_eq!(convert_base(" 5", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidDigit {operand:0,index:0})); assert_eq!(convert_base("+5", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidDigit {operand:0,index:0})); assert_eq!(convert_base("5 ", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidDigit {operand:0,index:1}));
}

// R13
#[test]
fn hidden_base_leading_separator() {
assert_eq!(convert_base("_1", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::Separator {operand:0,index:0}));
}

// R9, R13
#[test]
fn hidden_base_trailing_separator() {
assert_eq!(convert_base("10#1_", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::Separator {operand:0,index:4}));
}

// R13, R15
#[test]
fn hidden_base_doubled_separator() {
assert_eq!(convert_base("1__2", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::Separator {operand:0,index:1})); assert_eq!(convert_base("__", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::Separator {operand:0,index:0}));
}

// R12, R24
#[test]
fn hidden_base_lexical_before_separator() {
assert_eq!(convert_base("_1?", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidDigit {operand:0,index:2})); assert_eq!(convert_base("00x", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidDigit {operand:0,index:2}));
}

// R14, R15
#[test]
fn hidden_base_leading_zero() {
assert_eq!(convert_base("00", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::LeadingZero {operand:0,index:0})); assert_eq!(convert_base("16#0_f", 16, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::LeadingZero {operand:0,index:3}));
}

// R13-R15, R24
#[test]
fn hidden_base_separator_before_zero() {
assert_eq!(convert_base("00_", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::Separator {operand:0,index:2}));
}

// R4, R24
#[test]
fn hidden_base_operand_precedence() {
assert_eq!(add_base("0_1", "?", 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::LeadingZero {operand:0,index:0})); assert_eq!(add_base("2", "_?", 10, BaseStyle {upper:false,group:0,min_width:0}), Err(BaseError::InvalidDigit {operand:1,index:1}));
}

// R18-R21
#[test]
fn hidden_base_zero_format() {
assert_eq!(convert_base("0", 2, 36, BaseStyle {upper:true,group:3,min_width:7}), Ok("0_000_000".into()));
}

// R20, R21, R23
#[test]
fn hidden_base_padding_before_grouping() {
assert_eq!(convert_base("31", 10, 16, BaseStyle {upper:false,group:3,min_width:8}), Ok("00_000_01f".into()));
}

// R10, R19
#[test]
fn hidden_base_mixed_case() {
assert_eq!(convert_base("aZ", 36, 36, BaseStyle {upper:true,group:0,min_width:0}), Ok("AZ".into())); assert_eq!(convert_base("Zz", 36, 36, BaseStyle {upper:false,group:0,min_width:0}), Ok("zz".into()));
}

// R13, R16
#[test]
fn hidden_base_irregular_input_groups() {
assert_eq!(convert_base("1_23_4_567", 10, 10, BaseStyle {upper:false,group:0,min_width:0}), Ok("1234567".into()));
}

// R3, R21
#[test]
fn hidden_base_group_edges() {
assert_eq!(convert_base("12345678", 10, 10, BaseStyle {upper:false,group:8,min_width:0}), Ok("12345678".into())); assert_eq!(convert_base("123456789", 10, 10, BaseStyle {upper:false,group:8,min_width:0}), Ok("1_23456789".into()));
}

// R3, R18, R20
#[test]
fn hidden_base_width_boundary() {
let s=convert_base("1", 10, 10, BaseStyle {upper:false,group:0,min_width:256}).unwrap(); assert_eq!(s.len(),256); assert!(s.ends_with("01")); assert_eq!(convert_base("12345", 10, 10, BaseStyle {upper:false,group:0,min_width:2}), Ok("12345".into()));
}

// R1, R16, R25
#[test]
fn hidden_base_beyond_u128() {
assert_eq!(convert_base("340282366920938463463374607431768211456", 10, 16, BaseStyle {upper:false,group:0,min_width:0}), Ok("100000000000000000000000000000000".into()));
}

// R1, R17, R25
#[test]
fn hidden_base_large_add_carry() {
assert_eq!(add_base(&"z".repeat(150),"1",36,BaseStyle {upper:false,group:0,min_width:0}),Ok(format!("1{}","0".repeat(150))));
}

// R1, R16, R25
#[test]
fn hidden_base_large_binary() {
assert_eq!(convert_base(&format!("1{}","0".repeat(512)),2,16,BaseStyle {upper:true,group:0,min_width:0}),Ok(format!("1{}","0".repeat(128))));
}

// R6, R17, R19-R22, R26
#[test]
fn hidden_base_add_tag_format() {
assert_eq!(add_base("16#f_f", "16#1", 16, BaseStyle {upper:true,group:2,min_width:6}), Ok("00_01_00".into())); assert_eq!(add_base("0", "0", 10, BaseStyle {upper:false,group:0,min_width:0}), Ok("0".into()));
}
