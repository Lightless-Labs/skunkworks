// R10, R16-R19
#[test]
fn public_base_conversion() {
assert_eq!(convert_base("255",10,16,BaseStyle {upper:true,group:0,min_width:0}),Ok("FF".into()));
}

// R17, R18
#[test]
fn public_base_addition() {
assert_eq!(add_base("101", "11",2,BaseStyle {upper:false,group:0,min_width:0}),Ok("1000".into()));
}

// R6, R13, R19-R22
#[test]
fn public_base_format() {
assert_eq!(convert_base("16#a_b",16,2,BaseStyle {upper:false,group:4,min_width:8}),Ok("1010_1011".into()));
}
