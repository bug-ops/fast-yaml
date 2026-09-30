#![no_main]

use fast_yaml_core::{Emitter, Parser};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &str| {
    if let Ok(formatted) = Emitter::format(input) {
        assert!(
            Parser::parse_all(&formatted).is_ok(),
            "formatter output does not parse: {formatted:?}"
        );
        let _ = Emitter::format(&formatted);
    }
});
