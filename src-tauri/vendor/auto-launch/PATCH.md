# Only Monitor Windows patch

Upstream: auto-launch 0.5.0, MIT, https://github.com/zzzgydi/auto-launch .
Used through official tauri-plugin-autostart 2.5.1; Cargo patch fixes only src/windows.rs.

- Quote the executable path for fixed `--autostart` arguments.
- Only register HKCU Run; never alter Windows StartupApproved or HKLM.
- Query registration for this exact command, propagate errors instead of treating them as absent.
- Disable is idempotent and refuses to delete a same-name entry owned by another path.

Other platform modules and API remain upstream. This application only supports Windows;
the command formatting is intentionally scoped to its fixed argument, not an arbitrary CLI encoder.
Run `cargo test --manifest-path src-tauri/vendor/auto-launch/Cargo.toml --lib` for the Windows regression.
Registry integration, approval-byte preservation and uninstall ownership are validated separately.
Future dependency updates must compare these behaviors before removing this patch.
