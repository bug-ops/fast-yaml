#![no_main]

use fast_yaml_core::limits::{ParseLimits, StreamBudget};
use fast_yaml_core::{DuplicateMergeKeys, LoadOptions, NormalizedInput, Parser, SetValues};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &str| {
    let Ok(normalized) = NormalizedInput::new(input) else {
        return;
    };
    let options = LoadOptions::new()
        .with_duplicate_merge_keys(DuplicateMergeKeys::LastWins)
        .with_set_values(SetValues::Ignore);
    let limits = ParseLimits::default();

    let mut loaded_events = 0_usize;
    let loaded = Parser::parse_normalized_observed(
        &normalized,
        &StreamBudget::new(limits),
        options,
        |_| loaded_events += 1,
    );
    let mut validated_events = 0_usize;
    let validated = Parser::validate_normalized_observed(
        &normalized,
        &StreamBudget::new(limits),
        options,
        |_| validated_events += 1,
    );

    assert_eq!(
        loaded.err().map(|error| format!("{error:?}")),
        validated.err().map(|error| format!("{error:?}"))
    );
    assert_eq!(loaded_events, validated_events);
});
