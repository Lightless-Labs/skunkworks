use std::iter::Peekable;
use std::str::CharIndices;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Nul { pos: usize },
    TrailingEscape { pos: usize },
    UnterminatedSingle { pos: usize },
    UnterminatedDouble { pos: usize },
    UnterminatedExpansion { pos: usize },
    InvalidExpansion { pos: usize },
    CommandSubstitution { pos: usize },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word { pub text: String, pub glob: Vec<usize> }

struct Parser<'a, 'e> {
    chars: Peekable<CharIndices<'a>>,
    env: &'e [(&'e str, &'e str)],
}

impl<'a, 'e> Parser<'a, 'e> {
    fn expansion(&mut self, pos: usize) -> Result<String, SplitError> {
        if self.chars.peek().map(|x| x.1) == Some('{') {
            self.chars.next();
            let mut content = String::new();
            let mut closed = false;
            while let Some((_, c)) = self.chars.next() {
                if c == '}' { closed = true; break; }
                content.push(c);
                if c == '\\' {
                    if let Some((_, next)) = self.chars.next() { content.push(next); }
                }
            }
            if !closed { return Err(SplitError::UnterminatedExpansion { pos }); }
            let n = content.char_indices().take_while(|&(i,c)| {
                if i == 0 { start(c) } else { rest(c) }
            }).map(|(i,c)| i+c.len_utf8()).last().unwrap_or(0);
            if n == 0 { return Err(SplitError::InvalidExpansion { pos }); }
            let name = &content[..n];
            let tail = &content[n..];
            let value = self.env.iter().find(|(k,_)| *k == name).map(|(_,v)| *v).unwrap_or("");
            if tail.is_empty() { return Ok(value.to_string()); }
            if !tail.starts_with(":-") { return Err(SplitError::InvalidExpansion { pos }); }
            if !value.is_empty() { return Ok(value.to_string()); }
            let mut out = String::new();
            let mut cs = tail[2..].chars();
            while let Some(c) = cs.next() {
                if c == '\\' { if let Some(d) = cs.next() { out.push(d); } }
                else { out.push(c); }
            }
            Ok(out)
        } else {
            let mut name = String::new();
            if self.chars.peek().is_some_and(|x| start(x.1)) {
                while self.chars.peek().is_some_and(|x| rest(x.1)) {
                    name.push(self.chars.next().unwrap().1);
                }
                Ok(self.env.iter().find(|(k,_)| *k == name).map(|(_,v)| (*v).to_string()).unwrap_or_default())
            } else { Ok("$".to_string()) }
        }
    }
}
fn start(c: char) -> bool { c.is_ascii_alphabetic() || c == '_' }
fn rest(c: char) -> bool { start(c) || c.is_ascii_digit() }
fn append(text: &mut String, glob: &mut Vec<usize>, c: char, active: bool) {
    if active && matches!(c, '*' | '?' | '[') { glob.push(text.len()); }
    text.push(c);
}

pub fn split(input: &str, env: &[(&str, &str)]) -> Result<Vec<Word>, SplitError> {
    if let Some(pos) = input.find('\0') { return Err(SplitError::Nul { pos }); }
    let mut parser = Parser { chars: input.char_indices().peekable(), env: env };
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
        if !double && c == '\'' { active = true; quote = Some(('\'',pos)); continue; }
        if !double && matches!(c, ' ' | '\t' | '\r' | '\n') {
            if active {
                result.push(Word { text, glob });
                text = String::new(); glob = Vec::new(); active = false;
            }
            continue;
        }
        if c == '`' || (c == '$' && parser.chars.peek().is_some_and(|x| x.1 == '(')) { return Err(SplitError::CommandSubstitution { pos }); }
        if c == '$' {
            active = true;
            let value = parser.expansion(pos)?;
            for d in value.chars() { append(&mut text, &mut glob, d, !double); }
            continue;
        }
        active = true;
        append(&mut text, &mut glob, c, !double);
    }
    if let Some((q,pos)) = quote {
        return Err(if q == '\'' { SplitError::UnterminatedSingle { pos } } else { SplitError::UnterminatedDouble { pos } });
    }
    if active { result.push(Word { text, glob });
    }
    Ok(result)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError { Nul { word: usize, pos: usize } }

pub fn join(words: &[String]) -> Result<String, JoinError> {
    for (word, text) in words.iter().enumerate() {
        if let Some(pos) = text.find('\0') { return Err(JoinError::Nul { word, pos }); }
    }
    let mut encoded = Vec::new();
    for word in words {
        let mut candidates = Vec::new();
        if !word.is_empty() && !word.contains('\n') {
            let mut bare = String::new();
            for c in word.chars() {
                if !(c.is_alphanumeric() || "_-./:=,+@%".contains(c)) { bare.push('\\'); }
                bare.push(c);
            }
            candidates.push(bare);
        }
        if !word.contains('\'') { candidates.push(format!("'{}'", word)); }
        if !word.contains('`') {
            let mut double = String::from("\"");
            for c in word.chars() {
                if matches!(c, '\\' | '"' | '$' | '`') { double.push('\\'); }
                double.push(c);
            }
            double.push('"');
            candidates.push(double);
        }
        if candidates.is_empty() {
            candidates.push(format!("'{}'", word.replace('\'', "'\\''")));
        }
        let best = candidates.into_iter().enumerate().min_by_key(|(i,s)| (s.len(), *i)).unwrap().1;
        encoded.push(best);
    }
    Ok(encoded.join(" "))
}
