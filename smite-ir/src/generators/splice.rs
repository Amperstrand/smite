//! Generator for the splice negotiation flow.

use rand::{Rng, RngExt};

use super::Generator;
use super::funding_flow::FundingFlowGenerator;
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

        // Build and send our splice_locked. The txid must be exactly 32
        // bytes: `generate_fresh(Bytes)` draws lengths 0..=256, and the
        // executor's txid copy would panic on shorter payloads.
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let splice_txid = builder.append(Operation::LoadBytes(vec![0x00; 32]), &[]);
        builder.append(Operation::SendSpliceLocked, &[channel_id, splice_txid]);
    }
}

/// Generates a splice attempt against a live channel: the full v1 funding
/// flow (so a real negotiated channel id exists and the funding is
/// confirmed), quiescence, then the splice negotiation and interactive
/// transaction construction on that channel.
///
/// `pick_variable(ChannelId)` chains off `RecvFundingSigned`'s output, so
/// the splice messages reference the channel the target actually knows
/// instead of an invented id — the difference between reaching the splice
/// state machine and dying at its unknown-channel guard.
#[derive(Clone, Copy)]
pub struct SpliceOnLiveChannelGenerator;

impl Generator for SpliceOnLiveChannelGenerator {
    fn generate(&self, builder: &mut ProgramBuilder, rng: &mut impl Rng) {
        FundingFlowGenerator.generate(builder, rng);

        // Quiesce the channel (BOLT 2: splicing requires quiescence).
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let initiator = builder.generate_fresh(VariableType::U8, rng);
        builder.append(Operation::SendStfu, &[channel_id, initiator]);

        // splice_init on the negotiated channel, then the target's ack.
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let amount = builder.generate_fresh(VariableType::Amount, rng);
        let feerate = builder.generate_fresh(VariableType::FeeratePerKw, rng);
        let locktime = builder.generate_fresh(VariableType::BlockHeight, rng);
        let funding_pubkey = builder.generate_fresh(VariableType::Point, rng);
        let sent_splice_init = builder.append(
            Operation::SendSpliceInit,
            &[channel_id, amount, feerate, locktime, funding_pubkey],
        );
        builder.append(Operation::RecvSpliceAck, &[sent_splice_init]);

        // Interactive transaction construction on the same channel: propose
        // an input (randomly the shared channel input) and an output, then
        // conclude and sign.
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let serial_id = builder.generate_fresh(VariableType::U32, rng);
        let prevtx = builder.generate_fresh(VariableType::Bytes, rng);
        let prevtx_vout = builder.generate_fresh(VariableType::U32, rng);
        let sequence = builder.generate_fresh(VariableType::U32, rng);
        let shared_input_txid = builder.append(Operation::LoadBytes(vec![0x00; 32]), &[]);
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

        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let serial_id = builder.generate_fresh(VariableType::U32, rng);
        let sats = builder.generate_fresh(VariableType::Amount, rng);
        let script = builder.generate_fresh(VariableType::Bytes, rng);
        builder.append(
            Operation::SendTxAddOutput,
            &[channel_id, serial_id, sats, script],
        );

        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        builder.append(Operation::SendTxComplete, &[channel_id]);
        builder.append(Operation::RecvTxComplete, &[]);

        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let txid = builder.generate_fresh(VariableType::Bytes, rng);
        let witness = builder.generate_fresh(VariableType::Bytes, rng);
        builder.append(Operation::SendTxSignatures, &[channel_id, txid, witness]);
    }
}
