//! Bridges parser events to the core schema predicates, so rules share one definition of `<<`,
//! `!!set` and null.

use fast_yaml_core::events::Tag;
use fast_yaml_core::merge::is_set_tag;
use fast_yaml_core::{ResolvedScalar, ScalarStyle, resolve_scalar};
use saphyr_parser::{ScalarStyle as SaphyrStyle, Tag as SaphyrTag};

const fn core_style(style: SaphyrStyle) -> ScalarStyle {
    match style {
        SaphyrStyle::Plain => ScalarStyle::Plain,
        SaphyrStyle::SingleQuoted => ScalarStyle::SingleQuoted,
        SaphyrStyle::DoubleQuoted => ScalarStyle::DoubleQuoted,
        SaphyrStyle::Literal => ScalarStyle::Literal,
        SaphyrStyle::Folded => ScalarStyle::Folded,
    }
}

fn core_tag(tag: &SaphyrTag) -> Tag<'static> {
    Tag::new(tag.handle.clone(), tag.suffix.clone())
}

/// Whether the collection tag is the core `!!set` tag.
pub(super) fn is_set(tag: Option<&SaphyrTag>) -> bool {
    tag.is_some_and(|tag| is_set_tag(&core_tag(tag)))
}

/// Whether the scalar resolves to null.
pub(super) fn is_null(text: &str, style: SaphyrStyle, tag: Option<&SaphyrTag>) -> bool {
    resolve_scalar(text, core_style(style), tag.map(core_tag).as_ref()) == ResolvedScalar::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(suffix: &str) -> SaphyrTag {
        SaphyrTag {
            handle: "tag:yaml.org,2002:".to_owned(),
            suffix: suffix.to_owned(),
        }
    }

    #[test]
    fn set_tag_is_recognized() {
        assert!(is_set(Some(&tag("set"))));
        assert!(!is_set(Some(&tag("map"))));
        assert!(!is_set(None));
    }

    #[test]
    fn null_follows_core_resolution() {
        assert!(is_null("", SaphyrStyle::Plain, None));
        assert!(is_null("~", SaphyrStyle::Plain, None));
        assert!(is_null("", SaphyrStyle::SingleQuoted, Some(&tag("null"))));
        assert!(!is_null("x", SaphyrStyle::Plain, Some(&tag("null"))));
        assert!(!is_null("", SaphyrStyle::SingleQuoted, None));
        assert!(!is_null("1", SaphyrStyle::Plain, None));
    }
}
