//! Bridge: turn deterministic seed candidates into program sketches that
//! describe what a smite-ir `Program` would execute.
//!
//! A sketch is the contract between the deterministic extraction layer
//! (requirements → seeds) and the IR layer (skeleton generator → Programs).
//! Each sketch lists the ordered steps a program would take, referencing
//! the BOLT requirement it exercises and the message flow it constructs.

use serde::{Deserialize, Serialize};

use crate::model::{Requirement, Seed, SeedStrategy};

/// A program sketch: what an IR program would do, in executable prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramSketch {
    /// Stable id: `<seed-id>:sketch`.
    pub id: String,
    /// The seed this sketch realizes.
    pub seed_id: String,
    /// The BOLT requirement exercised.
    pub requirement_id: String,
    /// What the sketch demonstrates.
    pub goal: String,
    /// Ordered steps the program would execute.
    pub steps: Vec<SketchStep>,
}

/// One step in a program sketch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SketchStep {
    /// Action: build, send, mine, expect, or act.
    pub action: String,
    /// What to build/send/expect.
    pub target: String,
    /// Parameters or expectations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Derives program sketches from seeds + their source requirements.
#[must_use]
pub fn sketches(seeds: &[Seed], reqs: &[Requirement]) -> Vec<ProgramSketch> {
    let req_map: std::collections::HashMap<&str, &Requirement> =
        reqs.iter().map(|r| (r.id.as_str(), r)).collect();

    seeds
        .iter()
        .filter_map(|seed| {
            let req = req_map.get(seed.requirement_id.as_str())?;
            Some(sketch_for(seed, req))
        })
        .collect()
}

fn sketch_for(seed: &Seed, req: &Requirement) -> ProgramSketch {
    let steps = match seed.strategy {
        SeedStrategy::SendDespitePrecondition => {
            vec![
                SketchStep {
                    action: "build".into(),
                    target: "channel".into(),
                    note: Some("open a funded, confirmed channel".into()),
                },
                SketchStep {
                    action: "act".into(),
                    target: precondition_act(req),
                    note: Some(format!("precondition: {}", conditions_text(req))),
                },
                SketchStep {
                    action: "build".into(),
                    target: format!("{}_message", req.message),
                    note: Some(format!("violates: {}", req.text)),
                },
                SketchStep {
                    action: "send".into(),
                    target: req.message.clone(),
                    note: Some("send despite the precondition above".into()),
                },
                SketchStep {
                    action: "expect".into(),
                    target: "warning_or_error".into(),
                    note: Some("implementation MUST respond with warning/error, not crash".into()),
                },
            ]
        }
        SeedStrategy::AssertResponse => {
            vec![
                SketchStep {
                    action: "build".into(),
                    target: "channel".into(),
                    note: Some("open a funded, confirmed channel".into()),
                },
                SketchStep {
                    action: "send".into(),
                    target: trigger_message(req),
                    note: Some(format!("trigger: {}", trigger_condition(req))),
                },
                SketchStep {
                    action: "expect".into(),
                    target: expected_response(req),
                    note: Some(format!("per BOLT: {}", req.text)),
                },
            ]
        }
        SeedStrategy::InvalidField => {
            vec![
                SketchStep {
                    action: "build".into(),
                    target: "channel".into(),
                    note: Some("open a funded, confirmed channel".into()),
                },
                SketchStep {
                    action: "build".into(),
                    target: format!("{}_message", req.message),
                    note: Some(format!("set field wrong: {}", field_name(req))),
                },
                SketchStep {
                    action: "send".into(),
                    target: req.message.clone(),
                    note: Some("send with the invalid field".into()),
                },
                SketchStep {
                    action: "expect".into(),
                    target: "error_or_warning".into(),
                    note: Some("implementation SHOULD reject the invalid value".into()),
                },
            ]
        }
    };

    ProgramSketch {
        id: format!("{}:sketch", seed.id),
        seed_id: seed.id.clone(),
        requirement_id: req.id.clone(),
        goal: format!(
            "Exercise BOLT requirement: {} {}",
            modality_text(req),
            req.text
        ),
        steps,
    }
}

fn conditions_text(req: &Requirement) -> String {
    if req.conditions.is_empty() {
        if let Some(pos) = req.text.find(" if ") {
            return req.text[pos + 4..].trim_end_matches('.').to_owned();
        }
        "precondition (inline)".to_owned()
    } else {
        req.conditions.join("; ")
    }
}

fn precondition_act(req: &Requirement) -> String {
    let text = req.text.to_lowercase();
    if text.contains("quiescent") || text.contains("stfu") {
        "establish_quiescence".into()
    } else if text.contains("splice") {
        "start_splice".into()
    } else if text.contains("htlc") {
        "add_htlc".into()
    } else if text.contains("shutdown") {
        "initiate_shutdown".into()
    } else {
        "establish_precondition".into()
    }
}

fn trigger_message(req: &Requirement) -> String {
    if !req.message.is_empty() {
        return req.message.clone();
    }
    let text = req.text.to_lowercase();
    if text.contains("splice") {
        return "splice_init".into();
    }
    if text.contains("shutdown") {
        return "shutdown".into();
    }
    if text.contains("htlc") {
        return "update_add_htlc".into();
    }
    "generic_message".into()
}

fn trigger_condition(req: &Requirement) -> String {
    conditions_text(req)
}

fn expected_response(req: &Requirement) -> String {
    let text = req.text.to_lowercase();
    if text.contains("warning") {
        "warning".into()
    } else if text.contains("error") {
        "error".into()
    } else if text.contains("tx_abort") {
        "tx_abort".into()
    } else if text.contains("close") {
        "close_connection".into()
    } else {
        "protocol_response".into()
    }
}

fn field_name(req: &Requirement) -> String {
    let text = req.text.to_lowercase();
    if let Some(start) = text.find('`')
        && let Some(end) = text[start + 1..].find('`')
    {
        return text[start + 1..start + 1 + end].to_owned();
    }
    if text.contains("feerate") {
        "funding_feerate_perkw".into()
    } else if text.contains("satoshis") {
        "funding_contribution_satoshis".into()
    } else if text.contains("locktime") {
        "locktime".into()
    } else {
        "unknown_field".into()
    }
}

fn modality_text(req: &Requirement) -> String {
    match req.modality {
        crate::model::Modality::Must => "MUST".into(),
        crate::model::Modality::MustNot => "MUST NOT".into(),
        crate::model::Modality::Should => "SHOULD".into(),
        crate::model::Modality::ShouldNot => "SHOULD NOT".into(),
        crate::model::Modality::May => "MAY".into(),
        crate::model::Modality::Other => String::new(),
    }
}
