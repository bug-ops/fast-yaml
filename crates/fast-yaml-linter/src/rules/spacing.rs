//! Anchor columns of the spacing rules, ported from yamllint's `spaces_after` and `spaces_before`.

use crate::{Finding, SourceContext, Span};

/// Which side of its limit a run of spaces fell on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceViolation {
    /// More spaces than the maximum.
    TooMany,
    /// Fewer spaces than the minimum.
    TooFew,
}

impl SpaceViolation {
    /// The byte offset yamllint reports for the gap that ends at `ending_token`.
    ///
    /// Too many spaces are reported at the last space, too few at the token itself. The byte
    /// before a gap that is too long is a space, so the offset is also a character boundary.
    pub(crate) const fn anchor(self, ending_token: usize) -> usize {
        match self {
            Self::TooMany => ending_token.saturating_sub(1),
            Self::TooFew => ending_token,
        }
    }

    /// A zero-width finding with `message` at [`anchor`](Self::anchor).
    pub(crate) fn finding(
        self,
        lines: &SourceContext<'_>,
        ending_token: usize,
        message: String,
    ) -> Finding {
        let at = lines.offset_to_location(self.anchor(ending_token));
        Finding::new(message, Span::new(at, at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findings_are_zero_width_at_the_anchor_in_characters() {
        let source = "é: [x,   y]";
        let lines = SourceContext::new(source);
        let many = SpaceViolation::TooMany.finding(&lines, 10, "many".to_owned());
        assert_eq!(
            (many.span().start.column().get(), many.span().start),
            (9, many.span().end)
        );
        let few = SpaceViolation::TooFew.finding(&lines, 7, "few".to_owned());
        assert_eq!(few.span().start.column().get(), 7);
    }

    #[test]
    fn too_many_points_at_the_last_space_and_too_few_at_the_token() {
        assert_eq!(SpaceViolation::TooMany.anchor(8), 7);
        assert_eq!(SpaceViolation::TooFew.anchor(8), 8);
        assert_eq!(SpaceViolation::TooMany.anchor(0), 0);
    }
}
