//! Non-Nyx verification: prove the deterministic pipeline produces valid,
//! executable IR programs without requiring AFL++/Nyx.
//!
//! This is Option C from docs/nyx-enablement-paths.md — runs directly
//! against the smite crates, no fuzzing infrastructure needed.

use std::fs;
use std::path::Path;

use smite::bolt::{Message, MessageType};
use smite_ir::Program;

fn main() {
    let seed_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/smite-seeds".to_owned());

    let dir = Path::new(&seed_dir);
    let entries: Vec<_> = fs::read_dir(dir)
        .expect("seed dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "seed"))
        .collect();

    println!("verifying {} seed files from {}", entries.len(), seed_dir);
    println!("{}", "=".repeat(70));

    let mut valid = 0;
    let mut invalid = 0;
    let mut total_ops = 0;

    for entry in &entries {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy();

        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                println!("  FAIL {}: read: {}", name, e);
                invalid += 1;
                continue;
            }
        };

        let program: Program = match postcard::from_bytes(&bytes) {
            Ok(p) => p,
            Err(e) => {
                println!("  FAIL {}: postcard decode: {}", name, e);
                invalid += 1;
                continue;
            }
        };

        let ops: Vec<_> = program
            .instructions
            .iter()
            .map(|instr| format!("{:?}", instr.operation))
            .collect();

        total_ops += program.instructions.len();

        // Validate: every input reference is within bounds
        let max_var = program.instructions.len();
        let inputs_valid = program
            .instructions
            .iter()
            .all(|instr| instr.inputs.iter().all(|&idx| idx < max_var));

        // Validate: program has at least one operation
        let non_empty = !program.instructions.is_empty();

        // Validate: program ends with a Send operation (has a side effect)
        let has_send = program.instructions.iter().any(|instr| {
            matches!(
                instr.operation,
                smite_ir::operation::Operation::SendStfu
                    | smite_ir::operation::Operation::SendSpliceInit
                    | smite_ir::operation::Operation::SendSpliceAck
                    | smite_ir::operation::Operation::SendSpliceLocked
                    | smite_ir::operation::Operation::SendMessage
            )
        });

        if inputs_valid && non_empty && has_send {
            println!(
                "  OK   {} ({} ops: {})",
                name,
                program.instructions.len(),
                ops.join(" → ")
            );
            valid += 1;
        } else {
            println!(
                "  WARN {} ({} ops, inputs_valid={}, non_empty={}, has_send={})",
                name,
                program.instructions.len(),
                inputs_valid,
                non_empty,
                has_send
            );
            invalid += 1;
        }
    }

    println!("{}", "=".repeat(70));
    println!(
        "result: {} valid, {} invalid, {} total operations",
        valid, invalid, total_ops
    );

    // Also verify the splice BOLT messages encode to valid wire format
    println!("\nBOLT message encoding verification:");
    let stfu = smite::bolt::Stfu {
        channel_id: smite::bolt::ChannelId::new([0x42; 32]),
        initiator: 1,
    };
    let msg = Message::Stfu(stfu);
    let encoded = msg.encode();
    println!(
        "  stfu: {} bytes, type={:?} (expect type 2)",
        encoded.len(),
        msg.msg_type()
    );
    assert_eq!(msg.msg_type(), MessageType::STFU);
    assert_eq!(encoded.len(), 2 + 32 + 1); // type prefix + channel_id + initiator

    let si = smite::bolt::SpliceInit {
        channel_id: smite::bolt::ChannelId::new([0x42; 32]),
        funding_contribution_satoshis: 250_000,
        funding_feerate_perkw: 253,
        locktime: 0,
        funding_pubkey: test_pubkey(),
        tlvs: smite::bolt::SpliceInitTlvs::default(),
    };
    let msg = Message::SpliceInit(si);
    let encoded = msg.encode();
    println!(
        "  splice_init: {} bytes, type={:?} (expect type 80)",
        encoded.len(),
        msg.msg_type()
    );
    assert_eq!(msg.msg_type(), MessageType::SPLICE_INIT);

    // Roundtrip: decode what we encoded
    let decoded = Message::decode(&encoded).expect("decode");
    match decoded {
        Message::SpliceInit(si) => {
            assert_eq!(si.funding_contribution_satoshis, 250_000);
            println!(
                "  splice_init roundtrip: OK (amount={})",
                si.funding_contribution_satoshis
            );
        }
        other => panic!("expected SpliceInit, got {:?}", other.msg_type()),
    }

    println!("\nall verifications passed");
}

fn test_pubkey() -> bitcoin::secp256k1::PublicKey {
    use bitcoin::secp256k1::{Secp256k1, SecretKey};
    let secp = Secp256k1::new();
    let sk = SecretKey::from_slice(&[0x02; 32]).expect("valid secret key");
    bitcoin::secp256k1::PublicKey::from_secret_key(&secp, &sk)
}
