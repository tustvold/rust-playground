# Credential

A crate for hashing and verifying passwords. 

Credentials are hashed with a user-provided salt using PBKDF2_HMAC_SHA256 with a default of 100,000 iterations. Cryptographic operations are performed using [ring](https://crates.io/crates/ring), which is largely a wrapper of BoringSSL and is well regarded for being hard to use incorrectly. It forms the basis of [webpki](https://crates.io/crates/webpki) and by extension [rustls](https://crates.io/crates/rustls).
