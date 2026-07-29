// Generate golden snapshot for testing binary determinism
// Run with: rustc --edition 2021 -L data_migration/target/debug/deps generate_golden.rs

use std::collections::HashMap;

fn main() {
    // This is a minimal binary export that matches sample_savings_payload from tests
    let payload_json = r#"{"SavingsGoals":{"next_id":2,"goals":[{"id":1,"owner":"GOWNER","name":"Emergency Fund","target_amount":5000,"current_amount":1000,"target_date":2000000000,"locked":false}]}}"#;
    
    // Generate checksum
    let version = 1u32;
    let format = "binary";
    let mut hasher = sha2::Sha256::new();
    hasher.update(version.to_le_bytes());
    hasher.update(format.as_bytes());
    hasher.update(payload_json.as_bytes());
    let checksum = hex::encode(hasher.finalize().as_ref());
    
    println!("Checksum: {}", checksum);
    println!("Payload: {}", payload_json);
}

mod hex {
    const HEX: &[u8] = b"0123456789abcdef";

    pub fn encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for &byte in bytes {
            s.push(HEX[(byte >> 4) as usize] as char);
            s.push(HEX[(byte & 0x0f) as usize] as char);
        }
        s
    }
}
