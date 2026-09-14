//! Generate a valid postcard-serialized IR program seed for afl-fuzz.
//! (Raw-byte seeds fail decode in-target and register as Nyx crashes.)
use rand::seq::IteratorRandom;
use rand::SeedableRng;
use smite_ir::builder::ProgramBuilder;
use smite_ir::generators::{AnyGenerator, Generator};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "seed.bin".into());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let mut rng = rand::rngs::SmallRng::seed_from_u64(nanos);
    let mut builder = ProgramBuilder::new();
    AnyGenerator::ALL
        .iter()
        .choose(&mut rng)
        .expect("generators")
        .generate(&mut builder, &mut rng);
    let program = builder.build();
    let bytes = postcard::to_allocvec(&program).expect("serialize");
    std::fs::write(&out, bytes).expect("write");
    eprintln!("wrote {} bytes", std::fs::metadata(&out).unwrap().len());
}
