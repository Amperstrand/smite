//! BOLT 2 `splice_ack` message.

use bitcoin::secp256k1::PublicKey;

use super::BoltError;
use super::tlv::TlvStream;
use super::types::ChannelId;
use super::wire::WireFormat;

/// TLV type for the `require_confirmed_inputs` flag.
const TLV_REQUIRE_CONFIRMED_INPUTS: u64 = 2;

/// BOLT 2 `splice_ack` message (type 81).
///
/// Sent in response to `splice_init` when the peer accepts the splice
/// attempt, carrying its own funding contribution and pubkey for the new
/// funding transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpliceAck {
    /// The channel being spliced.
    pub channel_id: ChannelId,
    /// Amount the sender adds (positive) or removes (negative) from its
    /// channel balance, in satoshis.  MAY be zero.
    pub funding_contribution_satoshis: i64,
    /// The sender's funding pubkey for the new funding output.
    pub funding_pubkey: PublicKey,
    /// Optional TLV extensions.
    pub tlvs: SpliceAckTlvs,
}

/// TLV extensions for the `splice_ack` message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpliceAckTlvs {
    /// When set, the sender requires the receiving node to only use confirmed
    /// inputs.
    pub require_confirmed_inputs: bool,
}

impl SpliceAckTlvs {
    /// Extracts TLVs from a parsed TLV stream.
    fn from_stream(stream: &TlvStream) -> Self {
        Self {
            require_confirmed_inputs: stream.get(TLV_REQUIRE_CONFIRMED_INPUTS).is_some(),
        }
    }
}

impl SpliceAck {
    /// Encodes to wire format (without message type prefix).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.channel_id.write(&mut out);
        self.funding_contribution_satoshis.write(&mut out);
        self.funding_pubkey.write(&mut out);

        let mut tlv_stream = TlvStream::new();
        if self.tlvs.require_confirmed_inputs {
            tlv_stream.add(TLV_REQUIRE_CONFIRMED_INPUTS, Vec::new());
        }
        out.extend(tlv_stream.encode());

        out
    }

    /// Decodes from wire format (without message type prefix).
    ///
    /// # Errors
    ///
    /// Returns `Truncated` if the payload is too short for any field,
    /// `InvalidPublicKey` if `funding_pubkey` does not deserialize, or a TLV
    /// error if the TLV stream is malformed.
    pub fn decode(payload: &[u8]) -> Result<Self, BoltError> {
        let mut cursor = payload;

        let channel_id = ChannelId::read(&mut cursor)?;
        let funding_contribution_satoshis = i64::read(&mut cursor)?;
        let funding_pubkey = WireFormat::read(&mut cursor)?;

        let tlv_stream = TlvStream::decode_with_known(cursor, &[TLV_REQUIRE_CONFIRMED_INPUTS])?;
        let tlvs = SpliceAckTlvs::from_stream(&tlv_stream);

        Ok(Self {
            channel_id,
            funding_contribution_satoshis,
            funding_pubkey,
            tlvs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CHANNEL_ID_SIZE, PUBLIC_KEY_SIZE};
    use super::*;
    use bitcoin::secp256k1::{Secp256k1, SecretKey};

    fn test_pubkey() -> PublicKey {
        let secp = Secp256k1::new();
        let secret_key =
            SecretKey::from_slice(&[0x43; 32]).expect("32 bytes is a valid secret key");
        PublicKey::from_secret_key(&secp, &secret_key)
    }

    #[test]
    fn encode_fixed_field_size() {
        let msg = SpliceAck {
            channel_id: ChannelId::new([0x42; CHANNEL_ID_SIZE]),
            funding_contribution_satoshis: 0,
            funding_pubkey: test_pubkey(),
            tlvs: SpliceAckTlvs::default(),
        };
        let encoded = msg.encode();
        // channel_id + s64 + pubkey + empty TLV stream.
        assert_eq!(encoded.len(), CHANNEL_ID_SIZE + 8 + PUBLIC_KEY_SIZE);
    }

    #[test]
    fn roundtrip_with_flag() {
        let original = SpliceAck {
            channel_id: ChannelId::new([0xab; CHANNEL_ID_SIZE]),
            funding_contribution_satoshis: -10_000,
            funding_pubkey: test_pubkey(),
            tlvs: SpliceAckTlvs {
                require_confirmed_inputs: true,
            },
        };
        let decoded = SpliceAck::decode(&original.encode()).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn roundtrip_zero_contribution_without_flag() {
        let original = SpliceAck {
            channel_id: ChannelId::new([0xcd; CHANNEL_ID_SIZE]),
            funding_contribution_satoshis: 0,
            funding_pubkey: test_pubkey(),
            tlvs: SpliceAckTlvs::default(),
        };
        let decoded = SpliceAck::decode(&original.encode()).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn decode_truncated_contribution() {
        assert_eq!(
            SpliceAck::decode(&[0x00; CHANNEL_ID_SIZE + 3]),
            Err(BoltError::Truncated {
                expected: 8,
                actual: 3
            })
        );
    }

    #[test]
    fn decode_empty() {
        assert_eq!(
            SpliceAck::decode(&[]),
            Err(BoltError::Truncated {
                expected: CHANNEL_ID_SIZE,
                actual: 0
            })
        );
    }
}
