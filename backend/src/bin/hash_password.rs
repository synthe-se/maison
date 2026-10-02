use std::io::{self, Read};

use argon2::{Argon2, PasswordHasher};

fn main() {
    let password = std::env::args()
        .nth(1)
        .unwrap_or_else(read_password_from_stdin);

    if password.trim().is_empty() {
        eprintln!("password cannot be empty");
        std::process::exit(1);
    }

    // The salt is drawn from the OS RNG by argon2 itself.
    let hash = Argon2::default()
        .hash_password(password.trim_end().as_bytes())
        .expect("password hashing should succeed")
        .to_string();

    println!("{hash}");
}

fn read_password_from_stdin() -> String {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .expect("stdin should be readable");
    buffer
}
