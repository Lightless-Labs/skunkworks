#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    UnexpectedClose { pos: usize },
    Unclosed { pos: usize },
    NestedOpen { pos: usize },
    EmptyExpression { pos: usize },
    UnsupportedOperator { pos: usize },
    EmptyVariable { pos: usize },
    InvalidName { pos: usize },
    DuplicateVariable { pos: usize },
    InvalidModifier { pos: usize },
    TypeMismatch { pos: usize },
    DuplicateKey { pos: usize },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value { Scalar(String), List(Vec<String>), Map(Vec<(String,String)>) }
struct Var { name: String, pos: usize, explode: bool, prefix: Option<usize> }
enum Piece { Literal(String), Expression(char, Vec<Var>) }
fn parse(t: &str) -> Result<Vec<Piece>, Error> {
    let b=t.as_bytes(); let mut i=0; let mut pieces=Vec::new(); let mut start=0;
    while i<b.len() {
        if b[i]==b'}' { return Err(Error::UnexpectedClose{pos:i}); }
        if b[i]!=b'{' { i+=1; continue; }
        if start<i { pieces.push(Piece::Literal(t[start..i].to_string())); }
        let open=i;
        let end=match t[i+1..].find('}') { Some(n)=>i+1+n, None=>return Err(Error::Unclosed{pos:open}) };
        if let Some(n)=t[i+1..end].find('{') { return Err(Error::NestedOpen{pos:i+1+n}); }
        i+=1; let mut op=' ';
        if i<end && b"+#./;?&".contains(&b[i]) {
            op=b[i] as char;
            if !"+#./;?&".contains(op) { return Err(Error::UnsupportedOperator{pos:i}); }
            i+=1;
        }
        if i==end { return Err(Error::EmptyExpression{pos:i}); }
        let mut vars:Vec<Var>=Vec::new();
        while i<=end {
            let stop=t[i..end].find(',').map(|n|i+n).unwrap_or(end);
            if stop==i { return Err(Error::EmptyVariable{pos:i}); }
            let token=&t[i..stop];
            let cut=if 4==4 { token.find(['*', ':']).unwrap_or(token.len()) } else {token.len()};
            let name=&token[..cut];
            if name.is_empty() { return Err(Error::InvalidName{pos:i}); }
            for (j,c) in name.bytes().enumerate() {
                if !(c.is_ascii_alphabetic() || c==b'_' || (j>0 && c.is_ascii_digit())) {
                    return Err(Error::InvalidName{pos:i+j});
                }
            }
            let mut explode=false; let mut prefix=None;
            if cut<token.len() {
                let suffix=&token[cut..];
                if suffix=="*" {explode=true;}
                else if suffix.starts_with(':') {
                    let digits=&suffix[1..];
                    if digits.is_empty() || digits.len()>4 || digits.starts_with('0') || !digits.bytes().all(|c|c.is_ascii_digit()) {
                        return Err(Error::InvalidModifier{pos:i+cut});
                    }
                    prefix=Some(digits.parse::<usize>().unwrap());
                } else {return Err(Error::InvalidModifier{pos:i+cut});}
            }
            if vars.iter().any(|v|v.name==name) { return Err(Error::DuplicateVariable{pos:i}); }
            vars.push(Var{name:name.to_string(),pos:i,explode,prefix});
            if stop==end { break; }
            i=stop+1;
        }
        pieces.push(Piece::Expression(op,vars)); i=end+1; start=i;
    }
    if start<t.len() { pieces.push(Piece::Literal(t[start..].to_string())); }
    Ok(pieces)
}
fn encode(s:&str, reserved:bool)->String {
    let b=s.as_bytes(); let mut out=String::new(); let mut i=0;
    while i<b.len() {
        let c=b[i];
        if c==b'%' && i+2<b.len() && b[i+1].is_ascii_hexdigit() && b[i+2].is_ascii_hexdigit() {
            out.push('%'); out.push((b[i+1] as char).to_ascii_uppercase()); out.push((b[i+2] as char).to_ascii_uppercase()); i+=3; continue;
        }
        if c.is_ascii_alphanumeric() || b"-._~".contains(&c) || (reserved && b":/?#[]@!$&'()*+,;=".contains(&c)) {
            out.push(c as char);
        } else { out.push_str(&format!("%{:02X}",c)); }
        i+=1;
    }
    out
}
fn field(op:char, name:&str, value:String)->String {
    if op==';' { if value.is_empty() {name.to_string()} else {format!("{name}={value}")} }
    else if op=='?' || op=='&' { format!("{name}={value}") }
    else { value }
}

pub fn expand(template:&str, vars:&[(&str, Value)]) -> Result<String,Error> {
    let pieces=parse(template)?; let mut out=String::new();
    for piece in pieces {
        match piece {
            Piece::Literal(s)=>out.push_str(&s),
            Piece::Expression(op, specifiers)=>{
                let reserved=op=='+' || op=='#'; let mut parts=Vec::new();
                for v in specifiers {
                    let value=match vars.iter().rev().find(|(k,_)|*k==v.name) {Some((_,value))=>value,None=>continue};
                    match value {
                        Value::Scalar(s)=>{
                            let s=match v.prefix { Some(n)=>s.chars().take(n).collect::<String>(),None=>s.clone() };
                            parts.push(field(op,&v.name,encode(&s,reserved)));
                        }
                        Value::List(items)=>{
                            if v.prefix.is_some() {return Err(Error::TypeMismatch{pos:v.pos});}
                            if items.is_empty() {continue;}
                            let encoded:Vec<String>=items.iter().map(|s|encode(s,reserved)).collect();
                            if v.explode {for s in encoded {parts.push(field(op,&v.name,s));}}
                            else {parts.push(field(op,&v.name,encoded.join(",")));}
                        }
                        Value::Map(items)=>{
                            if v.prefix.is_some() {return Err(Error::TypeMismatch{pos:v.pos});}
                            let mut seen=std::collections::HashSet::new();
                            for (key,_) in items {if !seen.insert(key) {return Err(Error::DuplicateKey{pos:v.pos});}}
                            if items.is_empty() {continue;}
                            if v.explode {
                                for (key,val) in items {
                                    let k=encode(key,reserved); let s=encode(val,reserved);
                                    if op==';' {parts.push(field(op,&k,s));}
                                    else {parts.push(format!("{k}={s}"));}
                                }
                            } else {
                                let mut flat=Vec::new();
                                for (key,val) in items {flat.push(encode(key,reserved));flat.push(encode(val,reserved));}
                                parts.push(field(op,&v.name,flat.join(",")));
                            }
                        }
                    }
                }
                if !parts.is_empty() {
                    let (prefix,sep)=match op {'#'=>("#",","),'.'=>(".","."),'/'=>("/","/"),';'=>(";",";"),'?'=>("?","&"),'&'=>("&","&"),_=>("",",")};
                    out.push_str(prefix); out.push_str(&parts.join(sep));
                }
            }
        }
    }
    Ok(out)
}

