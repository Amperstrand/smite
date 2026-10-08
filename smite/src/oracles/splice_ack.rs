//! Oracle for the BOLT 2 `splice_ack` balance rule.
//!
//! A `splice_init` whose `funding_contribution_satoshis` splices out more
//! than the sender's channel balance is a negotiation the receiving node
//! MUST reject; acknowledging it with `splice_ack` is a spec violation.

use super::Oracle;
use crate::bolt::SpliceAck;
use crate::channel_tx::{ChannelState, Side};
use crate::violation::Violation;

/// Context for the `splice_ack` balance check.
pub struct SpliceAckContext<'a> {
    /// The `splice_ack` the target sent.
    pub splice_ack: &'a SpliceAck,
    /// The `funding_contribution_satoshis` of the `splice_init` this ack
    /// answers, if we recorded one for its channel.
    pub our_contribution_satoshis: Option<i64>,
    /// Tracked state for the channel, if any.
    pub channel_state: Option<&'a ChannelState>,
}

impl SpliceAckContext<'_> {
    /// The holder's channel balance in millisatoshis, if trackable.
    #[must_use]
    fn holder_balance_msat(&self) -> Option<u64> {
        let state = self.channel_state?;
        match state.holder.side {
            Side::Opener => Some(state.commitment.opener.balance_msat),
            Side::Acceptor => Some(state.commitment.acceptor.balance_msat),
        }
    }

    /// The counterparty's channel balance in millisatoshis, if trackable.
    #[must_use]
    fn counterparty_balance_msat(&self) -> Option<u64> {
        let state = self.channel_state?;
        match state.holder.side {
            Side::Opener => Some(state.commitment.acceptor.balance_msat),
            Side::Acceptor => Some(state.commitment.opener.balance_msat),
        }
    }
}

/// Judges `splice_ack` messages against the splice balance rules.
pub struct SpliceAckOracle;

impl Oracle<SpliceAckContext<'_>> for SpliceAckOracle {
    fn evaluate(&self, context: &SpliceAckContext<'_>) -> Result<(), Violation> {
        // Symmetric check first: the ack's own contribution is the
        // target's declared delta, independent of ours. Splicing out
        // beyond its own tracked balance is an impossible negotiation on
        // its side.
        let ack_contribution = context.splice_ack.funding_contribution_satoshis;
        if ack_contribution < 0
            && let Some(counterparty_msat) = context.counterparty_balance_msat()
            && u64::try_from(-ack_contribution)
                .expect("negative i64 magnitude fits in u64")
                .saturating_mul(1000)
                > counterparty_msat
        {
            return Err(Violation::InvalidSpliceAck(
                context.splice_ack.channel_id,
                format!(
                    "declared its own splice-out of {} msat against its balance of {counterparty_msat} msat",
                    u64::try_from(-ack_contribution)
                        .expect("negative i64 magnitude fits in u64")
                        .saturating_mul(1000)
                ),
            ));
        }

        let Some(contribution) = context.our_contribution_satoshis else {
            return Ok(());
        };
        // Only a splice-out can overdraw; positive contributions are
        // bounded by the contributor's wallet, not the channel balance.
        if contribution >= 0 {
            return Ok(());
        }
        let Some(balance_msat) = context.holder_balance_msat() else {
            return Ok(());
        };
        let splice_out_msat = u64::try_from(-contribution)
            .expect("negative i64 magnitude fits in u64")
            .saturating_mul(1000);
        if splice_out_msat > balance_msat {
            return Err(Violation::InvalidSpliceAck(
                context.splice_ack.channel_id,
                format!(
                    "acknowledged a splice-out of {splice_out_msat} msat against a balance of {balance_msat} msat"
                ),
            ));
        }
        Ok(())
    }
}
