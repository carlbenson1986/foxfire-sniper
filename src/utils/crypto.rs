use std::hash::{DefaultHasher, Hash, Hasher};

pub fn hash_i32_to_i32(value: i32) -> i32 {
    // Create a hasher
    let mut hasher = DefaultHasher::new();

    // Hash the i32 value
    value.hash(&mut hasher);

    // Get the hash as a u64
    let hash = hasher.finish();

    // Convert to i32 by using modulus or casting
    (hash as i32).abs() // Ensures the result is non-negative
}
