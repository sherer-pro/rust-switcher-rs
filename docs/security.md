# Security and privacy model

This document describes properties verified in the source tree and locked dependency graph. It is
not a security guarantee.

## Network

The runtime application has no network feature. The application source and the runtime crates in
`Cargo.lock` do not use networking APIs or network-capable HTTP, TLS, socket, WebSocket, telemetry,
analytics, crash-reporting, update, version-check, cloud-processing, or file-download libraries.

The Rust toolchain and development tools can access package registries while installing tools or
dependencies. That build-time activity is outside the runtime application.

Runtime coordination uses in-process channels plus a named single-instance mutex and a registered
window message. Those mechanisms do not carry journal or selected-text contents to another
process.

## Build-time code

`Cargo.lock` contains registry dependencies and the local `crates/rust-switcher-core` path crate;
there are no Git dependencies. The project `build.rs` calls `embed-resource` only to compile
`res/windows/app.rc`. On the supported MSVC build it invokes the toolchain resource compiler
(`rc.exe`); it does not download files, invoke PowerShell or `cmd.exe`, or modify persistence
settings.

## Keyboard input

The application installs a process-local callback for the global `WH_KEYBOARD_LL` hook. It decodes
physical key events to support Last Word, Last Sequence, and AutoConvert.

Recent decoded input is held in an in-memory `InputJournal`:

- the active journal is capped at 100 Unicode scalar values;
- focused controls can have separate cached journals keyed by foreground window, focused window,
  caret window, and caret rectangle;
- a cached journal is eligible for restoration for two minutes after it was cached; expired cache
  entries are pruned opportunistically on a later focus transition;
- the active journal can remain in memory longer than two minutes while the same context stays
  active;
- mouse button and wheel input invalidates the active journal; focus/window changes cache or clear
  it according to the key above;
- Left/Right/End update the journal caret model, Backspace removes the preceding character, and
  Delete, Home, Insert, Escape, Up, Down, Page Up, and Page Down clear the active journal;
- a Ctrl/Alt key combination with a non-modifier key, including paste shortcuts, clears the active
  journal. The clipboard is not read;
- injected keyboard input is ignored in normal builds. A debug build accepts it only when
  `RUST_SWITCHER_E2E_ALLOW_INJECTED` is set for the isolated end-to-end harness.

The journal and its cache are not serialized, written to configuration, or intentionally added to
diagnostic or crash-dump structures. They disappear when the process exits. The program does not
zeroize freed heap allocations, and an operating-system or third-party memory dump could contain
process memory.

### Password and secure fields

Before journaling a physical key and before executing any text-conversion command, the application
checks the focused UI Automation element's `IsPassword` property. Standard Win32 Edit/RichEdit
controls are also checked for the `ES_PASSWORD` style without reading their text.

When either check positively identifies a secure field, the application immediately clears the
active journal and every cached journal, does not record the key, and blocks:

- AutoConvert;
- Last Word and Smart Last Word;
- Last Sequence and Smart Last Sequence;
- Selection and Smart Selection.

The selection probes repeat the secure-field check before requesting selected text. AutoConvert
toggle and Switch Layout remain available because they do not read or replace journal text.

Detection is fail-open when neither UI Automation nor the Win32 control APIs can determine the
control type. This preserves normal typing in inaccessible or non-standard controls, but it means a
custom password control that does not expose `IsPassword` and is not a Win32 `ES_PASSWORD` control
cannot be recognized. Cross-integrity UI Automation access may also fail when the target runs at a
higher integrity level than Rust Switcher.

## Storage and persistence

The release configuration file is `%APPDATA%\RustSwitcher\config.json`. Debug builds use
`%APPDATA%\RustSwitcherDebug\config.json`. The configuration contains settings and hotkeys, not the
input journal.

Optional autostart uses only this per-user Registry value:

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run\RustSwitcher
```

The value contains the quoted current executable path followed by `--autostart`. The application
also detects and removes its legacy per-user Startup-folder shortcut,
`RustSwitcher Autostart.lnk`, while migrating an enabled legacy setting to the Registry value.

The runtime code contains no HKLM persistence, Windows service, Scheduled Task, driver, WMI
persistence, AppInit DLL, IFEO, or executable drop to a temporary/system directory.

## Privileges

The embedded application manifest does not request `requireAdministrator` or `highestAvailable`.
The application is intended to run as the current user and has no service or privileged helper.

## Clipboard and text replacement

The application does not call Windows clipboard APIs and does not synthesize Ctrl+C or Ctrl+V.
Selection conversion reads the selected range through UI Automation, with a fallback for supported
Win32 Edit/RichEdit controls. Converted text is inserted with `SendInput`; the user's clipboard is
not read, replaced, or restored.

## Logging

Runtime tracing records event names, lengths, directions, error categories, and other metadata. It
does not record source text, selected text, converted text, journal contents, or clipboard data.
Debug tracing writes to standard error only in debug builds built with the `debug-tracing` feature.
Release builds compile tracing out through `tracing/release_max_level_off`.

## Updates

The application does not check for or install updates. Updating requires replacing the executable
with a separately built or obtained version.

## Optional Windows Firewall defense in depth

The firewall rule is optional defense in depth; it is not a substitute for the absence of runtime
network code. From an elevated PowerShell, replace the placeholder with the absolute path to the
locally built executable. Do not point the rule at an upstream release binary.

```powershell
New-NetFirewallRule `
    -DisplayName "Rust Switcher - Block outbound" `
    -Direction Outbound `
    -Program "<ABSOLUTE_PATH_TO_RUST-SWITCHER.EXE>" `
    -Action Block
```

Remove the optional rule with:

```powershell
Remove-NetFirewallRule `
    -DisplayName "Rust Switcher - Block outbound"
```
