# live-region

This is a Rust crate for screen reader speech output in wxDragon applications using a live region.

Current platform behavior:

- Windows: native UI Automation live region announcements. Ignores announcement priorities; while UIA supports polite and assertive notifications in theory, they're ignored by screen readers in practice.
- macOS: native accessibility announcement notifications. Priorities are supported, although VoiceOver's implementation is buggy. See caveats in the documentation of the `Priority` enum.
- Linux: native ATK `notification` signal, delivered to any AT-SPI screen reader. Priorities are supported, mapped onto ATK's polite/assertive politeness. Falls back to the older `announcement` signal, which carries no priority, on ATK older than 2.50.
