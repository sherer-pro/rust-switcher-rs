# Security boundaries of this fork

This document describes the hardened fork based on upstream 1.0.26.

## Runtime and updates

The runtime has no networking code, telemetry, updater, model downloads, or
external-process execution. The WinHTTP feature, updater helper, and update menu
item have been removed. Review and build this fork manually for each update;
upstream release binaries do not contain these fork-specific protections.

EN/RU models are bundled. Additional models can only be read from existing files
under `%LOCALAPPDATA%\RustSwitcher\models\<sha256>\<language>.fst`.
Size and SHA-256 are checked before parsing. Missing or invalid models remain
unavailable; the existing planner fallback and AutoConvert evidence rules apply.
The application does not write or repair model files.

## Password fields

Before journaling and text conversion, detection checks UI Automation
`IsPassword` and standard Win32 Edit/RichEdit `ES_PASSWORD`.
A positive signal clears the active journal, every cached context, and decoder
composition. Physical text is not recorded and Last Word, Last Sequence,
Selection, smart conversion, and AutoConvert conversion are blocked.
Checks also run after conversion delays, before selection reads, and before
sending replacement input. Pause toggle and Switch Layout are unaffected.

Unknown results fail open. Custom controls without either signal, integrity
boundaries, and focus races cannot be completely covered. Detection reads
properties, not field contents. UI Automation provider latency needs validation
in real applications.

## Memory, storage, and privileges

The global low-level keyboard hook is still active outside password fields.
Each journal holds at most 4,096 Unicode characters, only in RAM. The active
journal has no expiry timer. Per-context cache restoration has a two-minute TTL;
expiry is pruned on context changes rather than by a deletion timer. Multiple
contexts can retain more than 4,096 characters in total. Memory is not zeroized.
No journals are serialized or deliberately included in crash dumps.

Settings are stored at `%APPDATA%\RustSwitcher\config.json`; debug builds use
`%APPDATA%\RustSwitcherDebug\config.json`. Autostart uses only the current user's
`Software\Microsoft\Windows\CurrentVersion\Run` key. Administrator privileges
are not requested. Keyboard DLL registrations are read from HKLM, not modified.
Keyboard DLLs are loaded from System32 after offline signature verification.

## Clipboard and logs

The clipboard is not read or modified. Selection uses UI Automation or bounded
Win32 control messages. Original, selected, converted, suffix, and journal text
are excluded from tracing, including debug builds. Logs retain metadata only.

## Acceptance

Automated tests cover secure detection policy, native password styles, journal
and cache clearing, conversion-command blocking, offline model integrity, text
logging, and existing conversion/hotkey behavior. Real browser and application
password fields require separate Windows 11 manual acceptance.
