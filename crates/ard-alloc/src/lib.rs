/// This is a collection of custom allocators.
///
/// Since I'm trying to avoid requiring a nightly compiler, these allocators don't use the Rust
/// allocator API, and are instead more tailored for their specific use cases.
///
/// When custom allocators become stable, these should all use that API instead to improve
/// ergonomics.
///
/// (As an aside, it's my dream to be able to use normal Rust data structures like Vec but with
/// memory backed by a GPU buffer. It would make the rendering code SO much cleaner and cooler.)
pub mod buddy;
