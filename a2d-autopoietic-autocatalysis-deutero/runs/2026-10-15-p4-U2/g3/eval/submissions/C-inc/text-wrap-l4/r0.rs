#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapError {
    WidthZero,
    WidthTooLarge { width: usize },
    FirstIndentTooWide { indent: usize },
    RestIndentTooWide { indent: usize },
    HyphenCostTooLarge { cost: usize },
    InputTooLong { chars: usize },
    InvalidControl { index: usize },
    InvalidMarker { index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapOptions {
    pub width: usize,
    pub first_indent: usize,
    pub rest_indent: usize,
    pub hyphen_cost: usize,
    pub justify: bool,
}

#[derive(Clone)]
struct Word {
    chars: Vec<char>,
    markers: Vec<usize>,
}

#[derive(Clone)]
enum Ending {
    Complete,
    Marker,
    Raw,
}

#[derive(Clone)]
struct Line {
    start: usize,
    end: usize,
    pieces: Vec<String>,
    ending: Ending,
    consumed: usize,
    gap: usize,
}

#[derive(Clone)]
struct Plan {
    cost: u64,
    markers: usize,
    lines: Vec<Line>,
}

fn plan_better(a: &Plan, b: &Plan, words: &[Word], width: usize, first_indent: usize, rest_indent: usize) -> bool {
    if a.cost != b.cost {
        return a.cost < b.cost;
    }
    if a.markers != b.markers {
        return a.markers < b.markers;
    }
    if a.lines.len() != b.lines.len() {
        return a.lines.len() < b.lines.len();
    }
    for (la, lb) in a.lines.iter().zip(&b.lines) {
        if la.consumed != lb.consumed {
            return la.consumed > lb.consumed;
        }
    }
    for (i, (la, lb)) in a.lines.iter().zip(&b.lines).enumerate() {
        let indent = if i == 0 { first_indent } else { rest_indent };
        let sa = render_pre(la, indent);
        let sb = render_pre(lb, indent);
        if sa != sb {
            return sa < sb;
        }
    }
    let _ = (words, width);
    false
}

fn render_pre(line: &Line, indent: usize) -> String {
    let mut s = " ".repeat(indent);
    s.push_str(&line.pieces.join(" "));
    if matches!(line.ending, Ending::Marker) {
        s.push('-');
    }
    s
}

fn markers_before(word: &Word, offset: usize) -> usize {
    word.markers.iter().copied().filter(|&m| m <= offset).count()
}

fn solve(words: &[Word], options: &WrapOptions) -> Vec<String> {
    use std::collections::HashMap;

    type State = (usize, usize, bool);
    fn rec(
        state: State,
        words: &[Word],
        o: &WrapOptions,
        memo: &mut HashMap<State, Plan>,
    ) -> Plan {
        if let Some(p) = memo.get(&state) {
            return p.clone();
        }
        let (wi, off, first) = state;
        if wi >= words.len() {
            return Plan { cost: 0, markers: 0, lines: Vec::new() };
        }
        let indent = if first { o.first_indent } else { o.rest_indent };
        let cap = o.width - indent;
        let mut best: Option<Plan> = None;

        let mut pieces = Vec::<String>::new();
        let mut used = 0usize;
        let mut cur_w = wi;
        let mut cur_off = off;

        loop {
            let word = &words[cur_w];
            let remaining = word.chars.len() - cur_off;
            let sep = if pieces.is_empty() { 0 } else { 1 };
            if used + sep >= cap {
                break;
            }
            let room = cap - used - sep;
            if remaining <= room {
                let text: String = word.chars[cur_off..].iter().collect();
                pieces.push(text);
                used += sep + remaining;
                cur_w += 1;
                if cur_w == words.len() {
                    let gap = o.width - indent - used;
                    let cost = (gap * gap / 4) as u64 + if pieces.len() == 1 { 0 } else { 0 };
                    let line = Line { start: wi, end: cur_w, pieces: pieces.clone(), ending: Ending::Complete, consumed: used, gap };
                    let candidate = Plan { cost, markers: 0, lines: vec![line] };
                    choose(&mut best, candidate);
                    break;
                }

                let suffix = rec((cur_w, 0, false), words, o, memo);
                let gap = o.width - indent - used;
                let line_cost = (gap * gap) as u64 + if pieces.len() == 1 { 3 } else { 0 };
                let line = Line { start: wi, end: cur_w, pieces: pieces.clone(), ending: Ending::Complete, consumed: used, gap };
                let mut candidate = Plan { cost: line_cost + suffix.cost, markers: suffix.markers, lines: vec![line] };
                candidate.lines.extend(suffix.lines);
                choose(&mut best, candidate);

                if used + 1 < cap {
                    used += 1;
                    continue;
                }
                break;
            } else {
                let fit = room;
                for &m in word.markers.iter().filter(|&&m| m > cur_off && m < word.chars.len()) {
                    let plen = m - cur_off;
                    if plen + 1 <= fit {
                        let mut ps = pieces.clone();
                        ps.push(word.chars[cur_off..m].iter().collect());
                        let gap = o.width - indent - (used + sep + plen + 1);
                        let suffix = rec((cur_w, m, false), words, o, memo);
                        let line = Line { start: wi, end: cur_w + 1, pieces: ps, ending: Ending::Marker, consumed: used + sep + plen, gap };
                        let mut candidate = Plan {
                            cost: (gap * gap) as u64 + o.hyphen_cost as u64 + if line.pieces.len() == 1 { 3 } else { 0 } + suffix.cost,
                            markers: 1 + suffix.markers,
                            lines: vec![line],
                        };
                        candidate.lines.extend(suffix.lines);
                        choose(&mut best, candidate);
                    }
                }

                if pieces.is_empty() && remaining > cap {
                    let marker_fits = word.markers.iter().any(|&m| m > cur_off && m < word.chars.len() && m - cur_off + 1 <= cap);
                    if !marker_fits {
                        let ps = vec![word.chars[cur_off..cur_off + cap].iter().collect()];
                        let suffix = rec((cur_w, cur_off + cap, false), words, o, memo);
                        let line = Line { start: wi, end: cur_w + 1, pieces: ps, ending: Ending::Raw, consumed: cap, gap: o.width - indent - cap };
                        let mut candidate = Plan {
                            cost: (line.gap * line.gap) as u64 + (2 * o.width + 1) as u64 + 3 + suffix.cost,
                            markers: suffix.markers,
                            lines: vec![line],
                        };
                        candidate.lines.extend(suffix.lines);
                        choose(&mut best, candidate);
                    }
                }
                break;
            }
        }

        let result = best.unwrap_or_else(|| {
            let text: String = words[wi].chars[off..].iter().take(cap).collect();
            let consumed = text.chars().count();
            let line = Line {
                start: wi,
                end: wi + 1,
                pieces: vec![text],
                ending: Ending::Raw,
                consumed,
                gap: o.width - indent - consumed,
            };
            Plan { cost: (line.gap * line.gap) as u64 + (2 * o.width + 1) as u64 + 3, markers: 0, lines: vec![line] }
        });
        memo.insert(state, result.clone());
        result
    }

    fn choose(best: &mut Option<Plan>, candidate: Plan) {
        match best {
            Some(current) => {
                if candidate.cost < current.cost
                    || (candidate.cost == current.cost
                        && (candidate.markers < current.markers
                            || (candidate.markers == current.markers
                                && candidate.lines.len() < current.lines.len())))
                {
                    *best = Some(candidate);
                }
            }
            None => *best = Some(candidate),
        }
    }

    let mut memo = HashMap::new();
    let plan = rec((0, 0, true), words, options, &mut memo);
    let mut out = Vec::new();
    for (i, line) in plan.lines.iter().enumerate() {
        let indent = if i == 0 { options.first_indent } else { options.rest_indent };
        let mut s = " ".repeat(indent);
        let gaps = line.pieces.len().saturating_sub(1);
        let pre_width = indent + line.pieces.iter().map(|p| p.chars().count()).sum::<usize>() + gaps + usize::from(matches!(line.ending, Ending::Marker));
        if options.justify && i + 1 < plan.lines.len() && gaps > 0 {
            let extra = options.width - pre_width;
            let each = extra / gaps;
            let rem = extra % gaps;
            for (g, piece) in line.pieces.iter().enumerate() {
                if g > 0 {
                    let start = (i % gaps);
                    let relative = (g - 1 + gaps - start) % gaps;
                    s.push_str(&" ".repeat(1 + each + usize::from(relative < rem)));
                }
                s.push_str(piece);
            }
        } else {
            s.push_str(&line.pieces.join(" "));
        }
        if matches!(line.ending, Ending::Marker) {
            s.push('-');
        }
        out.push(s);
    }
    out
}

fn choose(best: &mut Option<Plan>, candidate: Plan) {
    if best.as_ref().map_or(true, |b| candidate.cost < b.cost) {
        *best = Some(candidate);
    }
}

pub fn wrap(input: &str, options: &WrapOptions) -> Result<Vec<String>, WrapError> {
    if options.width == 0 {
        return Err(WrapError::WidthZero);
    }
    if options.width > 256 {
        return Err(WrapError::WidthTooLarge { width: options.width });
    }
    if options.first_indent >= options.width {
        return Err(WrapError::FirstIndentTooWide { indent: options.first_indent });
    }
    if options.rest_indent >= options.width {
        return Err(WrapError::RestIndentTooWide { indent: options.rest_indent });
    }
    if options.hyphen_cost > 1_000_000 {
        return Err(WrapError::HyphenCostTooLarge { cost: options.hyphen_cost });
    }

    let chars = input.chars().count();
    if chars > 512 {
        return Err(WrapError::InputTooLong { chars });
    }

    for (index, ch) in input.char_indices() {
        if ch.is_control() && ch != '\t' && ch != '\r' && ch != '\n' {
            return Err(WrapError::InvalidControl { index });
        }
    }

    let source: Vec<(usize, char)> = input.char_indices().collect();
    let is_sep = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{00a0}');
    for i in 0..source.len() {
        if source[i].1 == '~' {
            let invalid = i == 0
                || i + 1 == source.len()
                || source[i - 1].1 == '~'
                || source[i + 1].1 == '~'
                || is_sep(source[i - 1].1)
                || is_sep(source[i + 1].1);
            if invalid {
                return Err(WrapError::InvalidMarker { index: source[i].0 });
            }
        }
    }

    let mut normalized = Vec::new();
    let mut it = input.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\r' {
            if it.peek() == Some(&'\n') {
                it.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(c);
        }
    }

    let mut paragraphs: Vec<Vec<char>> = Vec::new();
    let mut current = Vec::new();
    let mut i = 0;
    while i < normalized.len() {
        if normalized[i] == '\n' {
            let mut j = i;
            while j < normalized.len() && normalized[j] == '\n' {
                j += 1;
            }
            if j - i >= 2 {
                paragraphs.push(std::mem::take(&mut current));
                for _ in 1..(j - i) {
                    paragraphs.push(Vec::new());
                }
                i = j;
                continue;
            }
        }
        current.push(normalized[i]);
        i += 1;
    }
    paragraphs.push(current);

    let mut output = Vec::new();
    for (pi, paragraph) in paragraphs.iter().enumerate() {
        let mut words = Vec::<Word>::new();
        let mut cur = Word { chars: Vec::new(), markers: Vec::new() };
        for &c in paragraph {
            if is_sep(c) {
                if !cur.chars.is_empty() {
                    words.push(cur);
                    cur = Word { chars: Vec::new(), markers: Vec::new() };
                }
            } else if c == '~' {
                cur.markers.push(cur.chars.len());
            } else {
                cur.chars.push(c);
            }
        }
        if !cur.chars.is_empty() {
            words.push(cur);
        }
        if !words.is_empty() {
            output.extend(solve(&words, options));
        }
        if pi + 1 < paragraphs.len() {
            output.push(String::new());
        }
    }
    Ok(output)
}

fn main() {}