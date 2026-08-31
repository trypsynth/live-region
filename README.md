# live-region

This is a Rust crate for screen reader speech output in wxDragon applications using a live region.

Current platform behavior:

- Windows: native UI Automation notification events. Priorities are supported and map onto the notification's processing mode: low queues behind everything pending, medium lets the current utterance finish before superseding anything staler, and high interrupts speech in progress.
- macOS: native accessibility announcement notifications. Priorities are supported, although VoiceOver's implementation is buggy. See caveats in the documentation of the `Priority` enum.
- Linux: native ATK `notification` signal, delivered to any AT-SPI screen reader. Priorities are supported, mapped onto ATK's polite/assertive politeness. Falls back to the older `announcement` signal, which carries no priority, on ATK older than 2.50.
