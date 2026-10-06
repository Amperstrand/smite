//! Generator for the splice negotiation flow.

use rand::Rng;

use super::Generator;
use crate::builder::ProgramBuilder;
use crate::{Operation, VariableType};

/// Generates a `splice_init` -> `splice_ack` exchange followed by the
/// `splice_locked` exchange.
///
/// Emits instructions to:
/// 1. Build and send `splice_init`, then receive `splice_ack`
/// 2. Build and send our `splice_ack`, then receive `splice_locked`
/// 3. Build and send our `splice_locked`
#[derive(Clone, Copy)]
pub struct SpliceFlowGenerator;

impl Generator for SpliceFlowGenerator {
    fn generate(&self, builder: &mut ProgramBuilder, rng: &mut impl Rng) {
        // Build and send splice_init.
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let amount = builder.generate_fresh(VariableType::Amount, rng);
        let feerate = builder.generate_fresh(VariableType::FeeratePerKw, rng);
        let locktime = builder.generate_fresh(VariableType::BlockHeight, rng);
        let funding_pubkey = builder.generate_fresh(VariableType::Point, rng);
        let sent_splice_init = builder.append(
            Operation::SendSpliceInit,
            &[channel_id, amount, feerate, locktime, funding_pubkey],
        );

        // Receive the target's splice_ack.
        builder.append(Operation::RecvSpliceAck, &[sent_splice_init]);

        // Build and send our splice_ack.
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let amount = builder.generate_fresh(VariableType::Amount, rng);
        let funding_pubkey = builder.generate_fresh(VariableType::Point, rng);
        let sent_splice_ack = builder.append(
            Operation::SendSpliceAck,
            &[channel_id, amount, funding_pubkey],
        );

        // Receive the target's splice_locked.
        builder.append(Operation::RecvSpliceLocked, &[sent_splice_ack]);

        // Build and send our splice_locked.
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let splice_txid = builder.generate_fresh(VariableType::Bytes, rng);
        builder.append(Operation::SendSpliceLocked, &[channel_id, splice_txid]);
    }
}
