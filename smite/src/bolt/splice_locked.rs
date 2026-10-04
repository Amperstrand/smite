//! BOLT 2 `splice_locked` message.

use bitcoin::hashes::sha256;

use super::BoltError;
use super::types::ChannelId;
use super::wire::WireFormat;

/// BOLT 2 `splice_locked` message (type 77).
///
/// Sent once a splice transaction has reached sufficient depth, carrying the
/// txid of the confirmed transaction.  Replaces the previous funding
/// transaction once both peers have sent and received `splice_locked`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpliceLocked {
    /// The channel being spliced.
    pub channel_id: ChannelId,
    /// Txid of the confirmed splice transaction.
    pub splice_txid: sha256::Hash,
}

impl SpliceLocked {
    /// Encodes to wire format (without message type prefix).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.channel_id.write(&mut out);
        self.splice_txid.write(&mut out);
        out
    }

    /// Decodes from wire format (without message type prefix).
    ///
    /// # Errors
    ///
    /// Returns `Truncated` if the payload is too short.
    pub fn decode(payload: &[u8]) -> Result<Self, BoltError> {
        let mut cursor = payload;
        let channel_id = ChannelId::read(&mut cursor)?;
        let splice_txid = WireFormat::read(&mut cursor)?;

        Ok(Self {
            channel_id,
            splice_txid,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CHANNEL_ID_SIZE, SHA256_HASH_SIZE};
    use super::*;
    use bitcoin::hashes::Hash;

    #[test]
    fn encode_fixed_field_size() {
        let msg = SpliceLocked {
            channel_id: ChannelId::new([0x42; CHANNEL_ID_SIZE]),
            splice_txid: sha256::Hash::hash(&[0x51]),
        };
        let encoded = msg.encode();
        assert_eq!(encoded.len(), CHANNEL_ID_SIZE + SHA256_HASH_SIZE);
        assert_eq!(encoded[CHANNEL_ID_SIZE..], sha256::Hash::hash(&[0x51])[..]);
    }

    #[test]
    fn roundtrip() {
        let original = SpliceLocked {
            channel_id: ChannelId::new([0xab; CHANNEL_ID_SIZE]),
            splice_txid: sha256::Hash::hash(b"splice tx"),
        };
        let decoded = SpliceLocked::decode(&original.encode()).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn decode_truncated_channel_id() {
        assert_eq!(
            SpliceLocked::decode(&[0x00; 20]),
            Err(BoltError::Truncated {
                expected: CHANNEL_ID_SIZE,
                actual: 20
            })
        );
    }

    #[test]
    fn decode_truncated_txid() {
        assert_eq!(
            SpliceLocked::decode(&[0x00; CHANNEL_ID_SIZE + 10]),
            Err(BoltError::Truncated {
                expected: SHA256_HASH_SIZE,
                actual: 10
            })
        );
    }

    #[test]
    fn decode_empty() {
        assert_eq!(
            SpliceLocked::decode(&[]),
            Err(BoltError::Truncated {
                expected: CHANNEL_ID_SIZE,
                actual: 0
            })
        );
    }
}
