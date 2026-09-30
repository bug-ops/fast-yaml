#![no_main]

use fast_yaml_core::Emitter;
use libfuzzer_sys::fuzz_target;

// TODO(#ISSUE): assert `Parser::parse_all(&formatted).is_ok()` once the formatter no longer drops
// `...` (#ISSUE), hoists `%` continuation lines as directives (#ISSUE) or emits control-char
// anchors (#ISSUE).
fuzz_target!(|input: &str| {
    if let Ok(formatted) = Emitter::format(input) {
        let _ = Emitter::format(&formatted);
    }
});
