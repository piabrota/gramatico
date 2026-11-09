// The PTC rules a program can measure, with the decision proved by Verus.
//
// A port of h0ffmann/proof's z3-scala/src/main/scala/Ptc.scala (commit 1bcbc91). There, each
// sentence's measure (its word count, its PTC-9 hits) becomes a Z3 constant, and Z3 looks for a
// model of "some measure > limit": unsat means no sentence breaks the rule. Here, Verus (on Z3)
// proves once, for every input, that `breaks` flags exactly the units whose measure passes the
// limit and that `holds` is true exactly when none does. The reader (reader.rs) is plain Rust and
// is not verified.
//
//     scripts/verus.sh                       verify, compile, self-test, check README.md
//     scripts/verus.sh FILE...               the same, on these Markdown files
//
// Ported: PTC-5 (at most 35 words in a sentence, required at level leve), PTC-9 (no chatbot
// artefact, meta-text or generic end, required), and at most 6 sentences in a paragraph
// (recommended). Not ported: PTC-1, PTC-6, PTC-7 and PTC-8, whose patterns need a regex engine.
use vstd::prelude::*;

mod reader;

verus! {

/// The rule holds: no unit's measure passes the limit.
pub open spec fn holds(measures: Seq<u64>, limit: u64) -> bool {
    forall|i: int| 0 <= i < measures.len() ==> measures[i] <= limit
}

/// One flag per unit: true when its measure passes the limit.
pub fn breaks(measures: &Vec<u64>, limit: u64) -> (flags: Vec<bool>)
    ensures
        flags.len() == measures.len(),
        forall|i: int| 0 <= i < measures.len() ==> flags@[i] == (measures@[i] > limit),
{
    let mut flags: Vec<bool> = Vec::new();
    let mut i: usize = 0;
    while i < measures.len()
        invariant
            i <= measures.len(),
            flags.len() == i,
            forall|k: int| 0 <= k < i ==> flags@[k] == (measures@[k] > limit),
        decreases measures.len() - i,
    {
        flags.push(measures[i] > limit);
        i = i + 1;
    }
    flags
}

/// True exactly when no flag is set, so with `breaks`: true exactly when the rule holds.
pub fn none(flags: &Vec<bool>) -> (r: bool)
    ensures
        r == (forall|i: int| 0 <= i < flags.len() ==> !flags@[i]),
{
    let mut i: usize = 0;
    while i < flags.len()
        invariant
            i <= flags.len(),
            forall|k: int| 0 <= k < i ==> !flags@[k],
        decreases flags.len() - i,
    {
        if flags[i] {
            assert(flags@[i as int]);
            return false;
        }
        i = i + 1;
    }
    true
}

/// The decision of Ptc.scala's `prove`: whether the rule holds, and the units that break it.
pub fn decide(measures: &Vec<u64>, limit: u64) -> (r: (bool, Vec<bool>))
    ensures
        r.0 == holds(measures@, limit),
        r.1.len() == measures.len(),
        forall|i: int| 0 <= i < measures.len() ==> r.1@[i] == (measures@[i] > limit),
{
    let flags = breaks(measures, limit);
    let ok = none(&flags);
    (ok, flags)
}

} // verus!

use reader::Unit;

struct Check {
    rule: &'static str,
    name: &'static str,
    required: bool,
    over_paragraphs: bool,
    measure: fn(&Unit) -> u64,
    limit: u64,
    explain: fn(&Unit) -> String,
}

fn sentence_words(u: &Unit) -> u64 {
    reader::words(&u.text)
}

fn noise(u: &Unit) -> u64 {
    reader::noise_hits(&u.prose).len() as u64
}

fn checks() -> Vec<Check> {
    vec![
        Check {
            rule: "PTC-5",
            name: "every sentence has at most 35 words",
            required: true,
            over_paragraphs: false,
            measure: sentence_words,
            limit: reader::MAX_WORDS,
            explain: |u| format!("{} words", reader::words(&u.text)),
        },
        Check {
            rule: "PTC-9",
            name: "no chatbot artefact, meta-text or generic conclusion",
            required: true,
            over_paragraphs: false,
            measure: noise,
            limit: 0,
            explain: |u| reader::noise_hits(&u.prose).join(", "),
        },
        Check {
            rule: "PTC",
            name: "at most 6 sentences in a paragraph (recommended)",
            required: false,
            over_paragraphs: true,
            measure: |u| u.sentences as u64,
            limit: reader::MAX_SENTENCES,
            explain: |u| format!("{} sentences", u.sentences),
        },
    ]
}

/// Runs one check. Returns whether it holds and the units that break it.
fn run<'a>(check: &Check, units: &'a [Unit]) -> (bool, Vec<&'a Unit>) {
    let measures: Vec<u64> = units.iter().map(|u| (check.measure)(u)).collect();
    let (ok, flags) = decide(&measures, check.limit);
    let broken = units.iter().zip(flags).filter(|(_, b)| *b).map(|(u, _)| u).collect();
    (ok, broken)
}

// Each required check rejects its bad text and accepts its good text (Ptc.scala's SELF_TEST).
fn self_test() -> i32 {
    let long = format!("Esta frase tem {}palavras.", "muitas ".repeat(33));
    let short = format!("Esta frase tem {}palavras.", "poucas ".repeat(30));
    let cases = [
        ("PTC-5", long.as_str(), short.as_str()),
        ("PTC-9", "Ótima pergunta! O futuro é promissor. Espero ter ajudado!", "O serviço reinicia em 5 s."),
    ];
    let mut bad = 0;
    for check in checks().iter().filter(|c| c.required) {
        let (_, reject, accept) =
            cases.iter().find(|c| c.0 == check.rule).expect("a self-test for each required check");
        for (text, should_hold) in [(*reject, false), (*accept, true)] {
            let (units, _) = reader::split_sentences(&[(1, text.to_string())], "self-test");
            let (ok, _) = run(check, &units);
            if ok != should_hold {
                bad += 1;
                println!("FAIL  self-test {}: {} {:?}", check.rule, if ok { "accepts" } else { "rejects" }, text);
            }
        }
    }
    if bad == 0 {
        println!("OK    self-test: every required check rejects its bad text and accepts its good text");
        0
    } else {
        println!("\n{bad} self-test failure(s)");
        1
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--self-test") {
        std::process::exit(self_test());
    }
    let (mut sentences, mut paras) = (Vec::new(), Vec::new());
    for file in &args {
        let source = std::fs::read_to_string(file).unwrap_or_else(|e| {
            eprintln!("ptc: {file}: {e}");
            std::process::exit(2)
        });
        let (s, p) = reader::read(file, &source);
        sentences.extend(s);
        paras.extend(p);
    }
    println!("PTC, level leve: {} sentences and {} paragraphs in {}", sentences.len(), paras.len(), args.join(", "));
    let mut failed = 0;
    for check in checks() {
        let units = if check.over_paragraphs { &paras } else { &sentences };
        let (ok, broken) = run(&check, units);
        let status = if ok {
            "OK  "
        } else if check.required {
            "FAIL"
        } else {
            "NOTE"
        };
        println!("{status}  {}: {}", check.rule, check.name);
        for u in broken {
            println!("        {}:{}: {}", u.file, u.line, (check.explain)(u));
        }
        if !ok && check.required {
            failed += 1;
        }
    }
    if failed > 0 {
        println!("\n{failed} required PTC check(s) fail");
        std::process::exit(1);
    }
}
