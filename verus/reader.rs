// Reads a Markdown file into sentences and paragraphs, and measures each one for a PTC rule.
//
// A port of the reader of h0ffmann/proof's z3-scala/src/main/scala/Ptc.scala (commit 1bcbc91), with
// std only: the regular expressions there are written out by hand here. Plain Rust: Verus does not
// verify this file (ptc.rs proves only the decision, `breaks`).

/// A sentence or a paragraph. `text`: the words, code and URLs masked as one word each; `prose`:
/// the same with quotations out (what the rules apply to); `sentences`: for a paragraph, how many.
pub struct Unit {
    pub file: String,
    pub line: usize,
    pub text: String,
    pub prose: String,
    pub sentences: usize,
}

pub const MAX_WORDS: u64 = 35; // PTC-5 at level leve
pub const MAX_SENTENCES: u64 = 6; // sentences per paragraph, recommended at level leve

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Replaces every `open..close` span on one line (both ends included) with `with`.
fn replace_spans(s: &str, open: char, close: char, with: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(a) = rest.find(open) {
        let after = &rest[a + open.len_utf8()..];
        match after.find(close) {
            Some(b) if !after[..b].contains('\n') => {
                out.push_str(&rest[..a]);
                out.push_str(with);
                rest = &after[b + close.len_utf8()..];
            }
            _ => {
                out.push_str(&rest[..a + open.len_utf8()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Inline code and URLs become one word (X); a citation key is not a word.
fn mask(s: &str) -> String {
    let t = replace_spans(s, '`', '`', " X ");
    let mut out: Vec<String> = Vec::new();
    for tok in t.split(' ') {
        let bare = tok.trim_start_matches(['<', '(']);
        if bare.starts_with("http://") || bare.starts_with("https://") {
            out.push("X".to_string());
        } else if tok.starts_with("[@")
            || (tok.starts_with('@') && tok[1..].starts_with(|c: char| c.is_ascii_alphabetic()))
        {
            out.push(String::new());
        } else {
            out.push(tok.to_string());
        }
    }
    out.join(" ")
}

/// `**x**` becomes x.
fn unbold(s: &str) -> String {
    s.replace("**", "")
}

/// Quotations and `*italic*` become one word (Q): a quotation or a foreign word is not prose.
fn unquote(s: &str) -> String {
    let t = replace_spans(s, '"', '"', " Q ");
    let t = replace_spans(&t, '“', '”', " Q ");
    replace_spans(&t, '*', '*', " Q ")
}

pub fn words(text: &str) -> u64 {
    text.split(|c: char| c.is_whitespace() || "|·—–⚠*".contains(c)).filter(|w| w.chars().any(is_word)).count() as u64
}

const END: &str = ".!?…";
const CLOSE: &str = "`\"”)]";
const NEXT: &str = "\"“([*`⚠ABCDEFGHIJKLMNOPQRSTUVWXYZÁÉÍÓÚÂÊÔÃÕÇπ0123456789";
const ABBREVIATIONS: [&str; 11] = ["p", "ex", "sr", "sra", "dr", "dra", "etc", "vs", "cf", "n", "nº"];

fn is_abbreviation(piece: &str) -> bool {
    let Some(head) = piece.strip_suffix('.') else {
        return false;
    };
    let word: String = head.chars().rev().take_while(|c| is_word(*c)).collect::<Vec<_>>().into_iter().rev().collect();
    let before = head[..head.len() - word.len()].chars().last();
    ABBREVIATIONS.contains(&word.to_lowercase().as_str()) && before.map_or(true, |c| !is_word(c))
}

/// Byte ranges of the sentences of a block: a sentence ends at . ! ? or … (and up to two closing
/// marks) before a space and a capital letter, a digit, a quotation mark, a bracket or a code mark.
fn sentence_ranges(text: &str, from: usize) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().filter(|(i, _)| *i >= from).collect();
    let mut out = Vec::new();
    let mut start = from;
    let mut k = 0;
    while k < chars.len() {
        if END.contains(chars[k].1) {
            let mut j = k + 1;
            while j < chars.len() && j <= k + 2 && CLOSE.contains(chars[j].1) {
                j += 1;
            }
            let end = if j < chars.len() { chars[j].0 } else { text.len() };
            let mut n = j;
            while n < chars.len() && chars[n].1.is_whitespace() {
                n += 1;
            }
            if n > j && n < chars.len() && NEXT.contains(chars[n].1) && !is_abbreviation(text[start..end].trim()) {
                out.push((start, end));
                start = chars[n].0;
                k = n;
                continue;
            }
        }
        k += 1;
    }
    out.push((start, text.len()));
    out
}

/// The sentences of one block, each with its line, and the number of sentences of the block.
pub fn split_sentences(block: &[(usize, String)], file: &str) -> (Vec<Unit>, usize) {
    let mut text = String::new();
    let mut starts = Vec::new();
    for (n, line) in block {
        starts.push((text.len(), *n));
        text.push_str(line);
        text.push(' ');
    }
    // A label opens a block: **Fala.**, **Tela.**
    let mut offset = 0;
    if let Some(rest) = text.strip_prefix("**") {
        if let Some(e) = rest.find("**") {
            if rest[..e].ends_with('.') && !rest[..e].contains('*') {
                offset = 2 + e + 2;
                offset += text[offset..].len() - text[offset..].trim_start().len();
            }
        }
    }
    let mut units = Vec::new();
    let mut count = 0;
    for (a, b) in sentence_ranges(&text, offset) {
        let piece = text[a..b].trim();
        if piece.is_empty() {
            continue;
        }
        count += 1;
        let line = starts.iter().filter(|(s, _)| *s <= a).map(|(_, n)| *n).max().unwrap_or(0);
        for item in piece.split(" · ") {
            let masked = mask(&unbold(item));
            units.push(Unit {
                file: file.to_string(),
                line,
                prose: unquote(&masked),
                text: masked.replace('*', ""),
                sentences: 1,
            });
        }
    }
    (units, count)
}

fn is_list_item(line: &str) -> Option<&str> {
    let t = line.trim_start();
    for m in ["- ", "* ", "+ "] {
        if let Some(r) = t.strip_prefix(m) {
            return Some(r.trim_start());
        }
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        if let Some(r) = t[digits..].strip_prefix(". ") {
            return Some(r.trim_start());
        }
    }
    None
}

fn is_table_rule(s: &str) -> bool {
    s.chars().all(|c| " \t:|-".contains(c))
}

/// Blocks of prose lines, with line numbers. A heading, a list item and a table cell are blocks;
/// code fences, HTML, images and `:::` divs are skipped; a YAML front matter gives its title and
/// subtitle.
pub fn paragraphs(source: &str) -> Vec<Vec<(usize, String)>> {
    let lines: Vec<&str> = source.lines().collect();
    let mut blocks: Vec<Vec<(usize, String)>> = Vec::new();
    let mut current: Vec<(usize, String)> = Vec::new();
    let mut start = 0;
    if lines.first().map(|l| l.trim()) == Some("---") {
        if let Some(end) = lines.iter().skip(1).position(|l| l.trim() == "---") {
            for (i, l) in lines[1..=end].iter().enumerate() {
                for key in ["title:", "subtitle:"] {
                    if let Some(v) = l.strip_prefix(key) {
                        let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
                        if !v.is_empty() {
                            blocks.push(vec![(i + 2, format!("{v}."))]);
                        }
                    }
                }
            }
            start = end + 2;
        }
    }
    let mut in_code = false;
    let flush = |current: &mut Vec<(usize, String)>, blocks: &mut Vec<Vec<(usize, String)>>| {
        if !current.is_empty() {
            blocks.push(std::mem::take(current));
        }
    };
    for (i, line) in lines.iter().enumerate().skip(start) {
        let n = i + 1;
        let s = line.trim();
        if s.starts_with("```") || s.starts_with("~~~") {
            in_code = !in_code;
            flush(&mut current, &mut blocks);
        } else if in_code || s.starts_with(":::") || s.starts_with('<') || s.starts_with("![") {
        } else if s.is_empty() {
            flush(&mut current, &mut blocks);
        } else if s.starts_with('#') {
            flush(&mut current, &mut blocks);
            let title = s.trim_start_matches('#').trim();
            let title = match title.find("{#").or_else(|| title.find("{.")) {
                Some(a) => title[..a].trim(),
                None => title,
            };
            if !title.is_empty() {
                blocks.push(vec![(n, title.to_string())]);
            }
        } else if s.starts_with('|') {
            flush(&mut current, &mut blocks);
            if !is_table_rule(s) {
                for cell in s.trim_matches('|').split('|') {
                    if !cell.trim().is_empty() {
                        blocks.push(vec![(n, cell.trim().to_string())]);
                    }
                }
            }
        } else if let Some(item) = is_list_item(line) {
            flush(&mut current, &mut blocks);
            current.push((n, item.to_string()));
        } else if let Some(caption) = s.strip_prefix(": ") {
            flush(&mut current, &mut blocks);
            current.push((n, caption.trim().to_string()));
        } else {
            current.push((n, s.to_string()));
        }
    }
    flush(&mut current, &mut blocks);
    blocks
}

/// (sentences, paragraphs) of one file. A paragraph's `sentences` is its sentence count.
pub fn read(file: &str, source: &str) -> (Vec<Unit>, Vec<Unit>) {
    let mut sentences = Vec::new();
    let mut paras = Vec::new();
    for block in paragraphs(source) {
        let (units, count) = split_sentences(&block, file);
        if let Some(first) = units.first() {
            paras.push(Unit {
                file: file.to_string(),
                line: first.line,
                text: units.iter().map(|u| u.text.as_str()).collect::<Vec<_>>().join(" "),
                prose: String::new(),
                sentences: count,
            });
        }
        sentences.extend(units);
    }
    (sentences, paras)
}

// PTC-9, the part that leaves at every level: chatbot artefact, meta-text, model limit, generic end.
const NOISE_ALWAYS: [&str; 34] = [
    "claro!",
    "com certeza!",
    "perfeito!",
    "ótima pergunta",
    "boa pergunta",
    "excelente pergunta",
    "excelente observação",
    "espero ter ajudado",
    "espero que ajude",
    "espero que isso ajude",
    "espero que tenha ajudado",
    "espero que isso tenha ajudado",
    "fique à vontade para",
    "qualquer dúvida, estou à disposição",
    "vamos lá",
    "vamos explorar",
    "vamos entender",
    "vamos mergulhar",
    "neste documento, vamos",
    "a seguir, veremos",
    "em resumo, vimos",
    "com base nas informações disponíveis",
    "até onde sei",
    "até onde eu sei",
    "o futuro é promissor",
    "só o tempo dirá",
    "uma coisa é certa",
    "vou analisar passo a passo",
    "pensando nisso de forma estruturada",
    "como modelo de linguagem",
    "como um modelo de linguagem",
    "como uma ia",
    "como ia",
    "como um ia",
];

/// The PTC-9 phrases in a sentence's prose, case-insensitive, at word boundaries.
pub fn noise_hits(prose: &str) -> Vec<&'static str> {
    let p = prose.to_lowercase();
    let mut hits = Vec::new();
    for phrase in NOISE_ALWAYS {
        let mut from = 0;
        while let Some(a) = p[from..].find(phrase) {
            let a = from + a;
            let b = a + phrase.len();
            let left = p[..a].chars().last().map_or(true, |c| !is_word(c));
            let ends_in_word = phrase.chars().last().is_some_and(is_word);
            let right = !ends_in_word || p[b..].chars().next().map_or(true, |c| !is_word(c));
            if left && right {
                hits.push(phrase);
            }
            from = b;
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> Vec<Unit> {
        split_sentences(&[(1, text.to_string())], "t").0
    }

    #[test]
    fn splits_at_a_capital_and_not_after_an_abbreviation() {
        let u = one("O laço roda. Ele para no fim, p. ex. na linha 3. A saída é 0,5.");
        assert_eq!(u.len(), 3, "{:?}", u.iter().map(|x| &x.text).collect::<Vec<_>>());
    }

    #[test]
    fn code_and_urls_are_one_word() {
        let u = one("Rode `just quality --all now` em https://example.com/a/b agora.");
        assert_eq!(words(&u[0].text), 5);
    }

    #[test]
    fn noise_at_word_boundaries() {
        assert_eq!(noise_hits("Ótima pergunta! Vamos lá."), vec!["ótima pergunta", "vamos lá"]);
        assert!(noise_hits("O vamos lado a lado.").is_empty());
        assert!(noise_hits(&unquote("Ele disse \"vamos lá\".")).is_empty());
    }

    #[test]
    fn blocks_skip_code_and_split_tables() {
        let src = "---\ntitle: Um título\n---\n# Cabeçalho\n\nUma frase. Outra frase.\n\n```\ncódigo. Aqui.\n```\n| a | b |\n|---|---|\n| c | d |\n- item um\n- item dois\n";
        let (s, p) = read("t", src);
        let texts: Vec<_> = s.iter().map(|u| u.text.trim().to_string()).collect();
        assert_eq!(
            texts,
            ["Um título.", "Cabeçalho", "Uma frase.", "Outra frase.", "a", "b", "c", "d", "item um", "item dois"]
        );
        assert_eq!(p.iter().map(|u| u.sentences).collect::<Vec<_>>(), [1, 1, 2, 1, 1, 1, 1, 1, 1]);
    }
}
