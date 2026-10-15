// R2, R3, R7, R8
#[test]
fn public_signed_encode() {
assert_eq!(signed_encode(2,SignedSystem::BalancedTernary,false),Ok("1T".into())); assert_eq!(signed_encode(2,SignedSystem::Negabinary,false),Ok("110".into()));
}

// R3, R21, R27, R30
#[test]
fn public_signed_decode() {
assert_eq!(signed_decode("1101",SignedSystem::Negabinary),Ok(-3)); assert_eq!(signed_decode("T1",SignedSystem::BalancedTernary),Ok(-2));
}

// R10-R12
#[test]
fn public_signed_checksum() {
assert_eq!(signed_encode(1,SignedSystem::BalancedTernary,true),Ok("1:1".into())); assert_eq!(signed_decode("1:1",SignedSystem::BalancedTernary),Ok(1));
}

// R28, R29
#[test]
fn public_signed_recode() {
assert_eq!(signed_recode("1T",SignedSystem::BalancedTernary,SignedSystem::Negabinary,true),Ok("110:3".into()));
}
