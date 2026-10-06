//! Generators for interactive transaction construction flows.

use rand::{Rng, RngExt};

use super::Generator;
use crate::builder::ProgramBuilder;
use crate::{Operation, VariableType};

/// Generates an interactive transaction negotiation: propose an input and an
/// output, optionally restart it with an RBF attempt, conclude it, and
/// sometimes sign the result.
#[derive(Clone, Copy)]
pub struct TxNegotiationGenerator;

impl Generator for TxNegotiationGenerator {
    fn generate(&self, builder: &mut ProgramBuilder, rng: &mut impl Rng) {
        // Propose an input; half the time mark it as the shared channel
        // input via the shared_input_txid TLV.
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let serial_id = builder.generate_fresh(VariableType::U32, rng);
        let prevtx = builder.generate_fresh(VariableType::Bytes, rng);
        let prevtx_vout = builder.generate_fresh(VariableType::U32, rng);
        let sequence = builder.generate_fresh(VariableType::U32, rng);
        let shared_input_txid = builder.generate_fresh(VariableType::Bytes, rng);
        builder.append(
            Operation::SendTxAddInput {
                include_shared_input_txid: rng.random(),
            },
            &[
                channel_id,
                serial_id,
                prevtx,
                prevtx_vout,
                sequence,
                shared_input_txid,
            ],
        );

        // Propose an output.
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let serial_id = builder.generate_fresh(VariableType::U32, rng);
        let sats = builder.generate_fresh(VariableType::Amount, rng);
        let script = builder.generate_fresh(VariableType::Bytes, rng);
        builder.append(
            Operation::SendTxAddOutput,
            &[channel_id, serial_id, sats, script],
        );

        // Half the time, restart the negotiation with an RBF attempt.
        if rng.random() {
            let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
            let locktime = builder.generate_fresh(VariableType::BlockHeight, rng);
            let feerate = builder.generate_fresh(VariableType::FeeratePerKw, rng);
            builder.append(Operation::SendTxInitRbf, &[channel_id, locktime, feerate]);
        }

        // Conclude the negotiation and wait for the target's tx_complete.
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        builder.append(Operation::SendTxComplete, &[channel_id]);
        builder.append(Operation::RecvTxComplete, &[]);

        // Half the time, sign the negotiated transaction.
        if rng.random() {
            let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
            let txid = builder.generate_fresh(VariableType::Bytes, rng);
            let witness = builder.generate_fresh(VariableType::Bytes, rng);
            builder.append(Operation::SendTxSignatures, &[channel_id, txid, witness]);
        }
    }
}

/// Generates a `tx_abort` and waits for the target's echo (BOLT 2: upon
/// `tx_abort`, the receiving node MUST echo it back).
#[derive(Clone, Copy)]
pub struct TxAbortEchoGenerator;

impl Generator for TxAbortEchoGenerator {
    fn generate(&self, builder: &mut ProgramBuilder, rng: &mut impl Rng) {
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let data = builder.generate_fresh(VariableType::Bytes, rng);
        builder.append(Operation::SendTxAbort, &[channel_id, data]);
        builder.append(Operation::RecvTxAbort, &[]);
    }
}
