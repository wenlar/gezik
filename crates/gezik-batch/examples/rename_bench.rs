//! cargo run --release -p gezik-batch --example rename_bench
use gezik_batch::rename::{Item, check, compile, new_names};
use gezik_core::batch::case::{CaseMode, Lang};
use gezik_core::batch::rules::{NumberRule, ReplaceRule, Rule, RuleEntry};
use gezik_core::ops::names::NameRules;

fn main() {
    let list: Vec<Item> = (0..1000).map(|n| Item { name: format!("IMG_{n:04}.jpg"), ..Item::default() }).collect();
    let rules = vec![
        RuleEntry::new(Rule::Replace(ReplaceRule {
            find: r"IMG_(\d+)".into(),
            with: "Tatil $1".into(),
            regex: true,
            case_sensitive: false,
            all: true,
        })),
        RuleEntry::new(Rule::Number(NumberRule::default())),
        RuleEntry::new(Rule::Case(CaseMode::Title)),
    ];
    let mut best = std::time::Duration::MAX;
    for _ in 0..20 {
        let start = std::time::Instant::now();
        let compiled = compile(&rules);
        let new = new_names(&list, &compiled, false, Lang::Turkic);
        let _ = check(&list, &new, &|_, _| false, NameRules::Windows, true);
        best = best.min(start.elapsed());
    }
    println!("1000 items, 3 rules: {:.2} ms", best.as_secs_f64() * 1000.0);
}
