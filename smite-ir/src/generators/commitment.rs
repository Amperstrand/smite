//! Generator for the post-establishment commitment dance.

use rand::{Rng, RngExt};

use super::Generator;
use super::funding_flow::FundingFlowGenerator;
use crate::builder::ProgramBuilder;
use crate::{Operation, VariableType};

/// Generates HTLC offers followed by `commitment_signed` (and half the time
/// a `revoke_and_ack`) against a live, funded, confirmed channel.
///
/// The funding prelude chains the real negotiated channel id into the
/// commitment messages, so the target's channel state machine — HTLC
/// validation, commitment numbering, revocation windows — is what actually
/// gets exercised, rather than its unknown-channel guard.
#[derive(Clone, Copy)]
pub struct CommitmentFlowGenerator;

impl Generator for CommitmentFlowGenerator {
    fn generate(&self, builder: &mut ProgramBuilder, rng: &mut impl Rng) {
        FundingFlowGenerator.generate(builder, rng);

        // Offer one to three HTLCs.
        for _ in 0..rng.random_range(1..=3) {
            let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
            let id = builder.generate_fresh(VariableType::U32, rng);
            let amount_msat = builder.generate_fresh(VariableType::Amount, rng);
            let payment_hash = builder.generate_fresh(VariableType::Bytes, rng);
            let cltv_expiry = builder.generate_fresh(VariableType::BlockHeight, rng);
            let onion_routing_packet = builder.generate_fresh(VariableType::Bytes, rng);
            builder.append(
                Operation::SendUpdateAddHtlc,
                &[
                    channel_id,
                    id,
                    amount_msat,
                    payment_hash,
                    cltv_expiry,
                    onion_routing_packet,
                ],
            );
        }

        // Resolve or fail an HTLC before signing. Failing needs no target
        // cooperation, so weight it 2:1. The id mostly chains off a
        // previously offered HTLC (pick_variable's most-recent bias);
        // fresh ids resolve never-offered HTLCs, which is itself a
        // violation shape worth reaching.
        let roll: u32 = rng.random_range(0..3);
        if roll < 2 {
            let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
            let id = builder.pick_variable(VariableType::U32, rng);
            let reason = builder.generate_fresh(VariableType::Bytes, rng);
            builder.append(Operation::SendUpdateFailHtlc, &[channel_id, id, reason]);
        } else {
            let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
            let id = builder.pick_variable(VariableType::U32, rng);
            let payment_preimage = builder.generate_fresh(VariableType::Bytes, rng);
            builder.append(
                Operation::SendUpdateFulfillHtlc,
                &[channel_id, id, payment_preimage],
            );
        }

        // Sign the new commitment.
        let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
        let signature = builder.generate_fresh(VariableType::Bytes, rng);
        builder.append(Operation::SendCommitmentSigned, &[channel_id, signature]);

        // Half the time, answer with a revocation (the BOLT 2 dance:
        // commitment_signed and revoke_and_ack interleave).
        if rng.random() {
            let channel_id = builder.pick_variable(VariableType::ChannelId, rng);
            let per_commitment_secret = builder.generate_fresh(VariableType::Bytes, rng);
            let next_per_commitment_point = builder.generate_fresh(VariableType::Point, rng);
            builder.append(
                Operation::SendRevokeAndAck,
                &[channel_id, per_commitment_secret, next_per_commitment_point],
            );
        }
    }
}
