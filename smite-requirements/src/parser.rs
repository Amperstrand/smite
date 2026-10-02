//! BOLT markdown requirement parser: deterministic, no LLM.
//!
//! Structure recognized (BOLT 02-style):
//!
//! ```text
//! ### The `splice_init` Message
//! ...
//! #### Requirements
//!
//! The sending node:
//!   - MUST NOT send `splice_init` if the channel is not quiescent.
//!   - If it is splicing funds out of the channel:
//!     - MUST set `funding_contribution_satoshis` to a negative value ...
//! ```
//!
//! Role lines are non-bullet lines ending in `:`.  Conditional bullets
//! (ending in `:`, no modality keyword) push onto a condition stack while
//! deeper bullets follow.

use thiserror::Error;

use crate::model::{Modality, Requirement, Seed, SeedStrategy};

#[derive(Debug, Error)]
pub enum ParseError {
}

/// Parses BOLT markdown text into requirements.
///
/// # Errors
///
/// Returns [`ParseError::TooDeep`] on bullets nested past the
/// condition-stack depth.
pub fn parse(source: &str, text: &str) -> Result<Vec<Requirement>, ParseError> {
    let mut out = Vec::new();
    let mut message = String::new();
    let mut role = String::new();
    let mut conditions: Vec<String> = Vec::new();
    let mut counters: Vec<(String, String, usize)> = Vec::new();

    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw.trim_start();

        if let Some(name) = message_heading(raw) {
            message = name;
            role.clear();
            conditions.clear();
            continue;
        }
        if trimmed.starts_with("## ") && !trimmed.starts_with("###") {
            // Major section (##): resets the message context.
            message.clear();
            role.clear();
            conditions.clear();
            continue;
        }
        // Sub-headings (###, ####): preserve message context, reset role.

        if let Some(indent) = bullet_indent(raw) {
            let bullet = trimmed.trim_start_matches("- ").trim();
            if is_condition(bullet) {
                conditions.truncate(indent / 2);
                conditions.push(normalize(bullet));
            } else {
                let modality = Modality::from_bullet(bullet);
                if modality == Modality::Other {
                    // Non-normative bullet: no requirement.
                    continue;
                }
                let id = next_id(&mut counters, &message, &role);
                out.push(Requirement {
                    id,
                    message: message.clone(),
                    role: role.clone(),
                    modality,
                    conditions: conditions.clone(),
                    text: normalize(bullet),
                    source: source.to_owned(),
                    line: line_no,
                });
            }
        } else if trimmed.ends_with(':') && !trimmed.is_empty() {
            // Role or sub-role context line.
            if !conditions.is_empty() {
                conditions.clear();
            }
            role = normalize(trimmed.trim_end_matches(':'));
        }
    }
    Ok(out)
}

/// Derives deterministic seed candidates from extracted requirements.
#[must_use]
pub fn seeds(reqs: &[Requirement]) -> Vec<Seed> {
    reqs.iter()
        .filter_map(|r| {
            let strategy = seed_strategy(r)?;
            Some(Seed {
                id: format!("{}:seed", r.id),
                requirement_id: r.id.clone(),
                strategy,
                message: r.message.clone(),
            })
        })
        .collect()
}

fn seed_strategy(r: &Requirement) -> Option<SeedStrategy> {
    if r.message.is_empty() {
        return None;
    }
    match r.modality {
        Modality::MustNot if !r.conditions.is_empty() || r.text.contains(" if ") => {
            Some(SeedStrategy::SendDespitePrecondition)
        }
        Modality::Must if r.role.contains("receiving") => Some(SeedStrategy::AssertResponse),
        Modality::Must if r.text.contains("set") => Some(SeedStrategy::InvalidField),
        _ => None,
    }
}

fn message_heading(line: &str) -> Option<String> {
    let t = line.trim();
    let stripped = t.trim_start_matches('#').trim();
    let hash_count = t.len() - t.trim_start_matches('#').len();
    if hash_count < 3 || hash_count > 4 {
        return None;
    }
    if stripped.starts_with("The `") && stripped.ends_with(" Message") {
        return stripped.split('`').nth(1).filter(|s| !s.is_empty()).map(str::to_owned);
    }
    if stripped.starts_with('`') && stripped.ends_with('`') && stripped.len() > 2 {
        return Some(stripped[1..stripped.len() - 1].to_owned());
    }
    None
}

fn bullet_indent(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start().len();
    let t = line.trim_start();
    if t.starts_with("- ") && indent % 2 == 0 {
        Some(indent / 2)
    } else {
        None
    }
}

fn is_condition(bullet: &str) -> bool {
    bullet.ends_with(':') && Modality::from_bullet(bullet) == Modality::Other
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn next_id(
    counters: &mut Vec<(String, String, usize)>,
    message: &str,
    role: &str,
) -> String {
    let role_slug = role
        .to_lowercase()
        .replace([' ', '/'], "-")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>();
    let key_msg = if message.is_empty() {
        String::from("general")
    } else {
        message.to_owned()
    };
    let key = (key_msg, role_slug);
    for entry in counters.iter_mut() {
        if entry.0 == key.0 && entry.1 == key.1 {
            entry.2 += 1;
            return format!("bolt02:{}:{}:{}", key.0, key.1, entry.2);
        }
    }
    counters.push((key.0.clone(), key.1.clone(), 1));
    format!("bolt02:{}:{}:1", key.0, key.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
### The `splice_init` Message

1. type: 80 (`splice_init`)

#### Requirements

The sending node:
  - MUST NOT send `splice_init` if the channel is not quiescent.
  - MUST set `funding_feerate_perkw` to the feerate for the splice transaction.
  - If it is splicing funds out of the channel:
    - MUST set `funding_contribution_satoshis` to a negative value.

The receiving node:
  - If the channel is not quiescent:
    - MUST send a `warning` and close the connection.
";

    #[test]
    fn extracts_requirements_with_roles_and_modalities() {
        let reqs = parse("fixture.md", FIXTURE).unwrap();
        assert_eq!(reqs.len(), 4);
        assert_eq!(reqs[0].message, "splice_init");
        assert_eq!(reqs[0].role, "The sending node");
        assert_eq!(reqs[0].modality, Modality::MustNot);
        assert!(reqs[0].text.contains("MUST NOT send"));
        assert_eq!(reqs[0].line, 8);
    }

    #[test]
    fn captures_condition_stack() {
        let reqs = parse("fixture.md", FIXTURE).unwrap();
        let negative = &reqs[2];
        assert_eq!(negative.conditions, vec!["If it is splicing funds out of the channel:"]);
        let warn = &reqs[3];
        assert_eq!(warn.role, "The receiving node");
        assert_eq!(warn.conditions, vec!["If the channel is not quiescent:"]);
    }

    #[test]
    fn ids_are_stable_and_unique() {
        let reqs = parse("fixture.md", FIXTURE).unwrap();
        let ids: Vec<&str> = reqs.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids.len(), usize_from_iter(ids.iter()));
        assert_eq!(reqs[0].id, "bolt02:splice_init:the-sending-node:1");
        assert_eq!(reqs[3].id, "bolt02:splice_init:the-receiving-node:1");
    }

    #[test]
    fn seeds_derive_strategies() {
        let reqs = parse("fixture.md", FIXTURE).unwrap();
        let seeds = seeds(&reqs);
        assert_eq!(seeds.len(), 4);
        assert!(seeds.iter().any(|s| s.strategy == SeedStrategy::SendDespitePrecondition));
        assert!(seeds.iter().any(|s| s.strategy == SeedStrategy::AssertResponse));
        assert!(seeds.iter().any(|s| s.strategy == SeedStrategy::InvalidField));
    }

    #[test]
    fn verify_roundtrip_by_construction() {
        let reqs = parse("fixture.md", FIXTURE).unwrap();
        let source_lines: Vec<&str> = FIXTURE.lines().collect();
        for r in &reqs {
            let line = source_lines[r.line - 1].trim().trim_start_matches("- ").trim();
            assert_eq!(normalize(line), r.text, "drift at {}", r.id);
        }
    }

    fn usize_from_iter<'a>(mut it: impl Iterator<Item = &'a &'a str>) -> usize {
        let mut seen = std::collections::HashSet::new();
        let mut n = 0;
        while let Some(s) = it.next() {
            if seen.insert(*s) {
                n += 1;
            }
        }
        n
    }
}
