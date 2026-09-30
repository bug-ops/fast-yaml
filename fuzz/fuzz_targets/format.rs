#![no_main]

use fast_yaml_core::Emitter;
use libfuzzer_sys::fuzz_target;

// TODO(#429, #430, #431): assert `Parser::parse_all(&formatted).is_ok()` once the formatter no
// longer drops `...` (#429), hoists `%` continuation lines as directives (#430) or emits
// control-char anchors (#431).
fuzz_target!(|input: &str| {
    if let Ok(formatted) = Emitter::format(input) {
        let _ = Emitter::format(&formatted);
    }
});
