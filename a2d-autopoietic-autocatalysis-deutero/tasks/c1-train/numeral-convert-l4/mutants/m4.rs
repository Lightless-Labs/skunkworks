#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordsError { Empty, InvalidByte { index: usize }, Separator { index: usize }, InvalidWord { index: usize }, Range, NonCanonical { index: usize } }
const WORD_LIMIT:i128=1_000_000_000_000_000_000;
const SMALL:[&str;20]=["zero","one","two","three","four","five","six","seven","eight","nine","ten","eleven","twelve","thirteen","fourteen","fifteen","sixteen","seventeen","eighteen","nineteen"];
const TENS:[&str;10]=["","","twenty","thirty","forty","fifty","sixty","seventy","eighty","ninety"];
const SCALES:[(u128,&str);6]=[(1_000_000_000_000_000_000,"quintillion"),(1_000_000_000_000_000,"quadrillion"),(1_000_000_000_000,"trillion"),(1_000_000_000,"billion"),(1_000_000,"million"),(1000,"thousand")];
fn under100(n:usize)->String {
    if n<20 { SMALL[n].into() }
    else if n%10==0 { TENS[n/10].into() }
    else { format!("{} {}",TENS[n/10],SMALL[n%10]) }
}
fn chunk(n:usize)->String {
    if n<100 { under100(n) }
    else {
        let mut s=format!("{} hundred",SMALL[n/100]);
        if n%100!=0 { s.push_str(" and "); s.push_str(&under100(n%100)); }
        s
    }
}
pub fn words_encode(value:i128)->Result<String,WordsError> {
    if value < -WORD_LIMIT || value > WORD_LIMIT { return Err(WordsError::Range); }
    if value==0 { return Ok("zero".into()); }
    let mut n=value.unsigned_abs();
    let mut s=String::new();
    for (scale,name) in SCALES {
        let g=(n/scale) as usize; n%=scale;
        if g!=0 {
            if !s.is_empty() { s.push(' '); }
            s.push_str(&chunk(g)); s.push(' '); s.push_str(name);
        }
    }
    if n!=0 {
        if !s.is_empty() { s.push_str(if n<100 { " and " } else { " " }); }
        s.push_str(&chunk(n as usize));
    }
    if value<0 { s.insert_str(0,"minus "); }
    Ok(s)
}
fn word_value(w:&str)->Option<u128> {
    if let Some(i)=SMALL.iter().position(|x| *x==w) { return Some(i as u128); }
    if let Some(i)=TENS.iter().position(|x| *x==w) { return Some((i*10) as u128); }
    None
}
pub fn words_decode(input:&str)->Result<i128,WordsError> {
    let b=input.as_bytes();
    if b.is_empty() { return Err(WordsError::Empty); }
    for (index,&c) in b.iter().enumerate() {
        if !c.is_ascii_lowercase() && c!=b' ' && c!=b'-' { return Err(WordsError::InvalidByte { index }); }
    }
    for (index,&c) in b.iter().enumerate() {
        if (c==b' ' || c==b'-') && (index==0 || index+1==b.len() || !b[index-1].is_ascii_lowercase() || !b[index+1].is_ascii_lowercase()) { return Err(WordsError::Separator { index }); }
    }
    let mut tokens=Vec::new();
    let mut start=0;
    for i in 0..=b.len() {
        if i==b.len() || b[i]==b' ' || b[i]==b'-' { tokens.push((start,&input[start..i])); start=i+1; }
    }
    for (i,&(index,w)) in tokens.iter().enumerate() {
        if word_value(w).is_none() && w!="hundred" && w!="and" && !SCALES.iter().any(|(_,name)| *name==w) && !(i==0 && w=="minus") { return Err(WordsError::InvalidWord { index }); }
    }
    let negative=tokens[0].1=="minus";
    let mut total=0u128;
    let mut group=0u128;
    for &(_,w) in tokens.iter().skip(if negative { 1 } else { 0 }) {
        if let Some(v)=word_value(w) { group+=v; }
        else if w=="hundred" { group*=100; }
        else if let Some((scale,_))=SCALES.iter().find(|(_,name)| *name==w) { total+=group*scale; group=0; }
        if total+group > WORD_LIMIT as u128 { return Err(WordsError::Range); }
    }
    let magnitude=(total+group) as i128;
    let value=if negative { -magnitude } else { magnitude };
    let canonical=words_encode(value)?;
    if input!=canonical {
        let index=b.iter().zip(canonical.as_bytes()).position(|(a,c)| a!=c).unwrap_or(b.len().min(canonical.len()));
        return Err(WordsError::NonCanonical { index });
    }
    Ok(value)
}
