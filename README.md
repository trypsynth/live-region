# live-region

This is a Rust crate for screen reader speech output in wxDragon applications using a live region.

Current platform behavior:

- Windows: native UI Automation live region announcements.
- macOS: native accessibility announcement notifications.
- Linux: attempts Orca announcement over D-Bus (`gdbus`).
