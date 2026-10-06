//! Oracle for the BOLT 2 quiescence rules.
//!
//! Once a channel is quiescent, a node MUST NOT send any message other
//! than `stfu`, `warning`, `error`, and the messages of the pending
//! operation (splicing / interactive transaction construction, including
//! the dual-funding `open_channel2` family).

use super::Oracle;
use crate::bolt::{ChannelId, Message};
use crate::violation::Violation;

/// Context for the quiescence check.
pub struct QuiescenceContext<'a> {
    /// The message the target sent.
    pub message: &'a Message,
    /// A channel that is quiescent, if any.
    pub quiescent_channel: Option<ChannelId>,
}

/// Judges received messages against the quiescence-allowed set.
pub struct QuiescenceOracle;

impl Oracle<QuiescenceContext<'_>> for QuiescenceOracle {
    fn evaluate(&self, context: &QuiescenceContext<'_>) -> Result<(), Violation> {
        use Message as M;
        if matches!(
            context.message,
            // Always allowed while quiescent.
            M::Stfu(_)
                | M::Warning(_)
                | M::Error(_)
                // The pending splice operation.
                | M::SpliceInit(_)
                | M::SpliceAck(_)
                | M::SpliceLocked(_)
                // Interactive transaction construction (splice and
                // dual funding share it).
                | M::TxAddInput(_)
                | M::TxAddOutput(_)
                | M::TxRemoveInput(_)
                | M::TxRemoveOutput(_)
                | M::TxComplete(_)
                | M::TxSignatures(_)
                | M::TxInitRbf(_)
                | M::TxAckRbf(_)
                | M::TxAbort(_)
                // Dual-funding channel establishment.
                | M::OpenChannel2(_)
                | M::AcceptChannel2(_)
        ) {
            return Ok(());
        }
        let channel = context
            .quiescent_channel
            .expect("caller only evaluates with a quiescent channel");
        Err(Violation::QuiescenceBroken(
            channel,
            context.message.to_string(),
        ))
    }
}
