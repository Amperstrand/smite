//! BOLT 2 `splice_init` message.

use bitcoin::secp256k1::PublicKey;

use super::BoltError;
use super::tlv::TlvStream;
use super::types::ChannelId;
use super::wire::WireFormat;

/// TLV type for the `require_confirmed_inputs` flag.
const TLV_REQUIRE_CONFIRMED_INPUTS: u64 = 2;

/// BOLT 2 `splice_init` message (type 80).
///
/// Sent by the quiescence initiator to begin a splice: adds or removes funds
/// from the channel balance and proposes parameters for the new shared
/// funding transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpliceInit {
    /// The channel to splice.
    pub channel_id: ChannelId,
    /// Amount the sender adds (positive, splice-in) or removes (negative,
    /// splice-out) from its channel balance, in satoshis.
    pub funding_contribution_satoshis: i64,
    /// Feerate for the splice transaction, in satoshis per kiloweight.
    pub funding_feerate_perkw: u32,
    /// Locktime for the splice transaction.
    pub locktime: u32,
    /// The sender's funding pubkey for the new funding output.
    pub funding_pubkey: PublicKey,
    /// Optional TLV extensions.
    pub tlvs: SpliceInitTlvs,
}

/// TLV extensions for the `splice_init` message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpliceInitTlvs {
    /// When set, the sender requires the receiving node to only use confirmed
    /// inputs.
    pub require_confirmed_inputs: bool,
}

impl SpliceInitTlvs {
    /// Extracts TLVs from a parsed TLV stream.
    fn from_stream(stream: &TlvStream) -> Self {
        Self {
            require_confirmed_inputs: stream.get(TLV_REQUIRE_CONFIRMED_INPUTS).is_some(),
        }
    }
}

impl SpliceInit {
    /// Encodes to wire format (without message type prefix).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.channel_id.write(&mut out);
        self.funding_contribution_satoshis.write(&mut out);
        self.funding_feerate_perkw.write(&mut out);
        self.locktime.write(&mut out);
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
        let funding_feerate_perkw = u32::read(&mut cursor)?;
        let locktime = u32::read(&mut cursor)?;
        let funding_pubkey = WireFormat::read(&mut cursor)?;

        let tlv_stream = TlvStream::decode_with_known(cursor, &[TLV_REQUIRE_CONFIRMED_INPUTS])?;
        let tlvs = SpliceInitTlvs::from_stream(&tlv_stream);

        Ok(Self {
            channel_id,
            funding_contribution_satoshis,
            funding_feerate_perkw,
            locktime,
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
            SecretKey::from_slice(&[0x42; 32]).expect("32 bytes is a valid secret key");
        PublicKey::from_secret_key(&secp, &secret_key)
    }

    #[test]
    fn encode_fixed_field_size() {
        let msg = SpliceInit {
            channel_id: ChannelId::new([0x42; CHANNEL_ID_SIZE]),
            funding_contribution_satoshis: 100_000,
            funding_feerate_perkw: 253,
            locktime: 0,
            funding_pubkey: test_pubkey(),
            tlvs: SpliceInitTlvs::default(),
        };
        let encoded = msg.encode();
        // channel_id + s64 + u32 + u32 + pubkey + empty TLV stream.
        assert_eq!(encoded.len(), CHANNEL_ID_SIZE + 8 + 4 + 4 + PUBLIC_KEY_SIZE);
    }

    #[test]
    fn roundtrip_splice_in() {
        let original = SpliceInit {
            channel_id: ChannelId::new([0xab; CHANNEL_ID_SIZE]),
            funding_contribution_satoshis: 250_000,
            funding_feerate_perkw: 1_000,
            locktime: 650_000,
            funding_pubkey: test_pubkey(),
            tlvs: SpliceInitTlvs {
                require_confirmed_inputs: true,
            },
        };
        let decoded = SpliceInit::decode(&original.encode()).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn roundtrip_splice_out_negative_contribution() {
        let original = SpliceInit {
            channel_id: ChannelId::new([0xcd; CHANNEL_ID_SIZE]),
            funding_contribution_satoshis: -50_000,
            funding_feerate_perkw: 253,
            locktime: 0,
            funding_pubkey: test_pubkey(),
            tlvs: SpliceInitTlvs::default(),
        };
        let decoded = SpliceInit::decode(&original.encode()).unwrap();
        assert_eq!(original, decoded);
        assert!(decoded.funding_contribution_satoshis < 0);
    }

    #[test]
    fn decode_truncated_pubkey() {
        let mut data = vec![0x00; CHANNEL_ID_SIZE + 8 + 4 + 4];
        data.push(0x02);
        assert_eq!(
            SpliceInit::decode(&data),
            Err(BoltError::Truncated {
                expected: PUBLIC_KEY_SIZE,
                actual: 1
            })
        );
    }

    #[test]
    fn decode_empty() {
        assert_eq!(
            SpliceInit::decode(&[]),
            Err(BoltError::Truncated {
                expected: CHANNEL_ID_SIZE,
                actual: 0
            })
        );
    }
}
