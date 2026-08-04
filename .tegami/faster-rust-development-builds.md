# Speed up Rust development builds

Development builds now compile the vendored terminal engine without release optimization, omit routine debug information, and exclude unused Ratatui features. Release builds remain fully optimized, and the `debugging` profile still provides full LLDB symbols.
