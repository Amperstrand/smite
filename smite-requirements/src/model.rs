//! Extracted BOLT requirement model.

use serde::{Deserialize, Serialize};

/// Normative modality of a BOLT requirement bullet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Must,
    MustNot,
    Should,
    ShouldNot,
    May,
    Other,
}

impl Modality {
    /// Parses the leading modality keyword of a requirement bullet.
    #[must_use]
    pub fn from_bullet(text: &str) -> Self {
        let t = text.trim_start();
        if t.starts_with("MUST NOT") {
            Self::MustNot
        } else if t.starts_with("MUST") {
            Self::Must
        } else if t.starts_with("SHOULD NOT") {
            Self::ShouldNot
        } else if t.starts_with("SHOULD") {
            Self::Should
        } else if t.starts_with("MAY") {
            Self::May
        } else {
            Self::Other
        }
    }
}

/// One extracted requirement: a normative bullet under a role context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    /// Stable id: `bolt02:<message>:<role-slug>:<n>`.
    pub id: String,
    /// Nearest enclosing `The \`name\` Message` heading (or section name).
    pub message: String,
    /// Role context line above the bullet block ("The sending node:").
    pub role: String,
    /// Modality keyword.
    pub modality: Modality,
    /// Enclosing condition bullets, outermost first (may be empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<String>,
    /// Full bullet text (modality included), whitespace-normalized.
    pub text: String,
    /// Source file the requirement was extracted from.
    pub source: String,
    /// 1-based line number of the bullet in the source file.
    pub line: usize,
}

/// A violation-seed candidate derived from a requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seed {
    /// Stable id: `<requirement-id>:seed`.
    pub id: String,
    /// The requirement this seed exercises.
    pub requirement_id: String,
    /// How to violate or exercise the requirement.
    pub strategy: SeedStrategy,
    /// Message whose flow to construct.
    pub message: String,
}

/// Deterministic seed strategies inferred from requirement shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedStrategy {
    /// MUST NOT + condition: construct the precondition, then send anyway.
    SendDespitePrecondition,
    /// Receiving node MUST respond: send the trigger, assert the response.
    AssertResponse,
    /// Sending node MUST set a field: send with the field wrong or unset.
    InvalidField,
}

/// Disclosure state gate for findings-derived enrichment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingRef {
    pub id: String,
    #[serde(rename = "disclosure_state")]
    pub state: String,
}

impl FindingRef {
    /// Whether this finding may contribute public seed shapes.
    ///
    /// Everything else (embargoed, undisclosed, internal) stays sealed.
    #[must_use]
    pub fn seedable(&self) -> bool {
        self.state == "fixed-ours" || self.state == "fixed-upstream-disclosed"
    }
}
