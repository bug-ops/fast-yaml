#![no_main]

use fast_yaml_linter::Linter;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &str| {
    let _ = Linter::with_all_rules().lint(input);
});
