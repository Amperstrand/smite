//! Converts program sketches into typed smite-ir `Program` instances.
//!
//! Each sketch maps to a `Program` that loads representative parameters,
//! then sends the target message. The channel setup is handled by the
//! scenario harness; the program performs the interesting violation.
//! Values are deterministic baselines that the AFL++ custom mutator
//! then explores around.

use bitcoin::secp256k1::ecdsa::Signature;
use smite::bolt::{ChannelId, CommitmentSigned, CommitmentSignedTlvs, Message};
use smite_ir::operation::Operation;
use smite_ir::program::Program;

use crate::bridge::ProgramSketch;

/// BOLT 3 dual-funding opener's P2WPKH change scriptPubKey.
const P2WPKH_SCRIPT: [u8; 22] = [
    0x00, 0x14, 0x1c, 0xa1, 0xcc, 0xa8, 0x85, 0x5b, 0xad, 0x6b, 0xc1, 0xea, 0x54, 0x36, 0xed, 0xd8,
    0xcf, 0xf1, 0x0b, 0x7e, 0x44, 0x8b,
];

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
        "tx_add_input" => {
            // the-sending-node:3: sequence MUST be <= 0xFFFFFFFD; only
            // 0xFFFFFFFE and 0xFFFFFFFF violate it.
            let sequence = if sketch.goal.contains("sequence") {
                0xFFFF_FFFF
            } else {
                0xFFFF_FFFD
            };
            Some(vec![
                (Operation::LoadChannelId([0x42; 32]), vec![]),
                (Operation::LoadU32(42), vec![]),
                (Operation::LoadBytes(vec![0xde, 0xad, 0xbe, 0xef]), vec![]),
                (Operation::LoadU32(0), vec![]),
                (Operation::LoadU32(sequence), vec![]),
                (Operation::SendTxAddInput, vec![0, 1, 2, 3, 4]),
            ])
        }
        "tx_add_output" => Some(vec![
            (Operation::LoadChannelId([0x42; 32]), vec![]),
            (Operation::LoadU32(42), vec![]),
            (Operation::LoadAmount(1000), vec![]),
            (Operation::LoadBytes(P2WPKH_SCRIPT.to_vec()), vec![]),
            (Operation::SendTxAddOutput, vec![0, 1, 2, 3]),
        ]),
        "tx_complete" => Some(vec![
            (Operation::LoadChannelId([0x42; 32]), vec![]),
            (Operation::SendTxComplete, vec![0]),
        ]),
        "tx_abort" => Some(vec![
            (Operation::LoadChannelId([0x42; 32]), vec![]),
            (
                Operation::LoadBytes(b"smite: negotiation failed".to_vec()),
                vec![],
            ),
            (Operation::SendTxAbort, vec![0, 1]),
        ]),
        "open_channel" => Some(vec![
            (Operation::LoadChainHashFromContext, vec![]),
            (Operation::LoadChannelId([0x42; 32]), vec![]),
            (Operation::LoadAmount(100_000), vec![]),
            (Operation::LoadAmount(0), vec![]),
            (Operation::LoadAmount(546), vec![]),
            (Operation::LoadAmount(100_000_000), vec![]),
            (Operation::LoadAmount(10_000), vec![]),
            (Operation::LoadAmount(1_000), vec![]),
            (Operation::LoadFeeratePerKw(253), vec![]),
            (Operation::LoadU16(144), vec![]),
            (Operation::LoadU16(483), vec![]),
            (Operation::LoadTargetPubkeyFromContext, vec![]),
            (Operation::LoadU8(1), vec![]),
            (Operation::LoadBytes(Vec::new()), vec![]),
            (Operation::LoadFeatures(Vec::new()), vec![]),
            (
                Operation::BuildOpenChannel,
                vec![
                    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 11, 11, 11, 11, 11, 12, 13, 14,
                ],
            ),
            (Operation::SendOpenChannel, vec![15]),
        ]),
        "channel_ready" => Some(vec![
            (Operation::LoadChannelId([0x42; 32]), vec![]),
            (Operation::LoadTargetPubkeyFromContext, vec![]),
            (Operation::LoadShortChannelId(0), vec![]),
            (
                Operation::SendChannelReady {
                    include_alias: false,
                },
                vec![0, 1, 2],
            ),
        ]),
        "commitment_signed" => {
            let cs = CommitmentSigned {
                channel_id: ChannelId::new([0x42; 32]),
                signature: Signature::from_compact(&[0u8; 64])
                    .expect("zero bytes parse as a signature"),
                htlc_signatures: Vec::new(),
                tlvs: CommitmentSignedTlvs::default(),
            };
            let encoded = Message::CommitmentSigned(cs).encode();
            Some(vec![
                (Operation::LoadMessage(encoded), vec![]),
                (Operation::SendMessage, vec![0]),
            ])
        }
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
