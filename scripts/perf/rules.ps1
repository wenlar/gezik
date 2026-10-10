# Times the view rules (spec 10 §8.3, §12): 20 rules (10 path, 5 kind, 5 content) against a
# 100,000-entry folder, release build (crates/gezik-core/examples/rules_bench.rs): the content
# count, one evaluation with the count made, and the heap the 20 rules hold. Budgets: count
# 2 ms, evaluation 50 µs, rules 20 KB; pass/fail goes by the medians (the worst evaluation is
# printed but is one sample of 10,000, open to OS preemption). Exits 1 when one is over. Any OS
# with PowerShell (on macOS and Linux the cargo line below works as it is).
# Usage: scripts/perf/rules.ps1 [-Entries 100000]
param([int]$Entries = 100000)

$root = Resolve-Path "$PSScriptRoot\..\.."
& cargo run -q -p gezik-core --release --example rules_bench --manifest-path "$root\Cargo.toml" -- $Entries
exit $LASTEXITCODE
