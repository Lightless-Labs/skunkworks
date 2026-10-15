use std::iter::Peekable;
use std::str::CharIndices;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    
    
}


struct Parser<'a, 'e> {
    chars: Peekable<CharIndices<'a>>,
    env: &'e [(&'e str, &'e str)],
}

fn append(text: &mut String, glob: &mut Vec<usize>, c: char, active: bool) {
    if active && matches!(c, '*' | '?' | '[') { glob.push(text.len()); }
    text.push(c);
}

pub fn split(input: &str) -> Result<Vec<String>, SplitError> {
    if let Some(pos) = input.find('\0') { return Err(SplitError::Nul { pos }); }
    let mut parser = Parser { chars: input.char_indices().peekable(), env: &[] };
    let mut result = Vec::new();
    let mut text = String::new();
    let mut glob = Vec::new();
    let mut active = false;
    let mut quote: Option<(char,usize)> = None;
    while let Some((pos,c)) = parser.chars.next() {
        if let Some(('\'',_)) = quote {
            if c == '\'' { quote = None; }
            else { append(&mut text, &mut glob, c, false); }
            continue;
        }
        let double = quote.is_some();
        if c == '\\' {
            let Some(&(_, next)) = parser.chars.peek() else {
                return Err(SplitError::TrailingEscape { pos });
            };
            if !double || matches!(next, '"' | '\\' | '$' | '\n') {
                parser.chars.next();
                if next != '\n' {
                    active = true;
                    append(&mut text, &mut glob, next, false);
                }
            } else { active = true; append(&mut text, &mut glob, c, false); }
            continue;
        }
        if c == '"' {
            active = true;
            if double { quote = None; } else { quote = Some(('"',pos)); }
            continue;
        }
        if !double && c == '\'' { active = true; quote = Some(('\'',0)); continue; }
        if !double && matches!(c, ' ' | '\t' | '\r' | '\n') {
            if active {
                result.push(text);
                text = String::new(); glob = Vec::new(); active = false;
            }
            continue;
        }
        
        
        active = true;
        append(&mut text, &mut glob, c, !double);
    }
    if let Some((q,pos)) = quote {
        return Err(if q == '\'' { SplitError::UnterminatedSingle { pos } } else { SplitError::UnterminatedDouble { pos } });
    }
    if active { result.push(text);
    }
    Ok(result)
}
