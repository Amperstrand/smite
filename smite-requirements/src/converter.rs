//! Converts program sketches into typed smite-ir `Program` instances.
//!
//! Each sketch maps to a `Program` that loads the required parameters,
//! then sends the splice-family message. The channel setup is handled by
//! the scenario harness; the program performs the interesting violation.

use smite_ir::operation::Operation;
use smite_ir::program::Program;

use crate::bridge::ProgramSketch;

/// Converts a sketch into an executable IR program builder sequence.
///
/// Returns `None` for sketches that reference messages without IR
/// operations yet (non-splice families).
#[must_use]
pub fn sketch_to_operations(sketch: &ProgramSketch) -> Option<Vec<(Operation, Vec<usize>)>> {
    let send_target = sketch
        .steps
        .iter()
        .find(|s| s.action == "send")
        .map(|s| s.target.as_str())?;

    match send_target {
        "stfu" => Some(vec![
            (Operation::LoadChannelId([0x42; 32]), vec![]),
            (Operation::LoadU8(1), vec![]),
            (Operation::SendStfu, vec![0, 1]),
        ]),
        "splice_init" => {
            let negative = sketch.goal.contains("negative") || sketch.goal.contains("splice-out");
            let amount: u64 = if negative { 50_000 } else { 250_000 };
            Some(vec![
                (Operation::LoadChannelId([0x42; 32]), vec![]),
                (Operation::LoadAmount(amount), vec![]),
                (Operation::LoadFeeratePerKw(253), vec![]),
                (Operation::LoadBlockHeight(0), vec![]),
                (Operation::LoadTargetPubkeyFromContext, vec![]),
                (Operation::SendSpliceInit, vec![0, 1, 2, 3, 4]),
            ])
        }
        "splice_ack" => Some(vec![
            (Operation::LoadChannelId([0x42; 32]), vec![]),
            (Operation::LoadAmount(0), vec![]),
            (Operation::LoadTargetPubkeyFromContext, vec![]),
            (Operation::SendSpliceAck, vec![0, 1, 2]),
        ]),
        "splice_locked" => Some(vec![
            (Operation::LoadChannelId([0x42; 32]), vec![]),
            (Operation::LoadBytes(vec![0xab; 32]), vec![]),
            (Operation::SendSpliceLocked, vec![0, 1]),
        ]),
        _ => None,
    }
}

/// Builds a `Program` from a sketch via `ProgramBuilder`.
#[must_use]
pub fn sketch_to_program(sketch: &ProgramSketch) -> Option<Program> {
    let ops = sketch_to_operations(sketch)?;
    let mut builder = smite_ir::builder::ProgramBuilder::new();
    for (op, inputs) in ops {
        builder.append(op, &inputs);
    }
    Some(builder.build())
}

/// Summary of what the converter can handle.
#[must_use]
pub fn convertible_count(sketches: &[ProgramSketch]) -> (usize, usize) {
    let convertible = sketches
        .iter()
        .filter(|s| sketch_to_operations(s).is_some())
        .count();
    (convertible, sketches.len())
}
