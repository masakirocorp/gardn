# Speed up Rust development builds

Development builds now compile the vendored terminal engine without release optimization, omit routine debug information, exclude unused Ratatui features, and cache the stable Local API contract in a separate workspace crate. Release builds remain fully optimized, and the `debugging` profile still provides full LLDB symbols.
