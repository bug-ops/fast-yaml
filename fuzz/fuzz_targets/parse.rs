#![no_main]

use fast_yaml_core::Parser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &str| {
    let _ = Parser::parse_str(input);
    let _ = Parser::parse_all(input);
});
