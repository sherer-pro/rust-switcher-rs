# Windows 11 manual test plan

Run this plan against the locally built release executable from the reviewed source tree. Record
the Git commit, `Cargo.lock` SHA-256, executable SHA-256, Windows build, application version, and
result for every tested application. Do not use an upstream release executable.

## Preconditions

1. Build with `scripts/build-release.ps1` from the repository root.
2. Confirm AutoConvert starts paused.
3. If debug-output inspection is required, build a separate debug binary with `debug-tracing`; do
   not substitute its keyboard-hook behavior for release acceptance.
4. Use non-sensitive synthetic text only.

Test at minimum in:

- Notepad;
- Chrome or Edge;
- Telegram Desktop;
- VS Code;
- Microsoft Word, if available.

## Existing conversion behavior

For each application, record the exact before/after text and pass/fail. These checks characterize
the existing behavior; they do not authorize changing Last Word, Last Sequence, Selection,
direction selection, or fallback behavior.

### Last Word

1. Type `ghbdtn` and invoke Last Word (default: double Left Shift).
2. Repeat with `Ghbdtn` and `GHBDTN`.
3. Repeat Russian-layout examples `руддщ`, `Руддщ`, and `РУДДЩ`.
4. Record caret, selection, layout, and resulting text after each action.

### Last Sequence

Type each input and invoke Last Sequence (default: double Left Alt):

```text
ghbdtn rfr ltkf
Ghbdtn rfr ltkf
GHBDTN RFR LTKF
```

Record the complete result and whether whitespace, punctuation, case, and caret position are
preserved.

### Selection

1. Enter `ghbdtn rfr ltkf`.
2. Select the full string.
3. Invoke Selection (default: double Left Shift).
4. Record the complete result, the final selection, caret position, and keyboard layout.

### AutoConvert

1. Enable AutoConvert from the tray or with the default double Right Ctrl sequence.
2. Verify its current normal behavior with synthetic RU/EN mistypes.
3. Disable it and verify no automatic conversion occurs.
4. Re-enable it with the hotkey and confirm the toggle still works.
5. Confirm feature settings and hotkeys persist after restart.

### Hotkey conflicts

1. Verify every configured action still fires once.
2. Verify the existing shared Last Word/Selection default follows the current priority rules.
3. Verify no new conflict warning appears after loading and saving unchanged settings.
4. Do not combine Selection and Last Sequence as part of this test.

## Password and secure fields

Use the unique synthetic value below; never use a real password:

```text
SecureTest-48291
```

Test at least:

- an HTML `<input type="password">` in Chrome or Edge;
- a standard Win32 password Edit control, when available;
- a browser login/password field.

For each field:

1. Enable AutoConvert.
2. Type the synthetic value and delimiters that would normally trigger AutoConvert.
3. Invoke Last Word, Last Sequence, Selection, Smart variants if enabled, and the AutoConvert
   toggle.
4. Leave the secure field and focus a normal text field.
5. Invoke Last Word and Last Sequence again.
6. Confirm the secure value was not converted, restored, inserted, or selected.
7. Search captured debug output for the exact synthetic value; it must be absent.
8. Search `%APPDATA%\RustSwitcher\config.json` and
   `%APPDATA%\RustSwitcherDebug\config.json`, if present; the value must be absent.
9. Confirm Switch Layout and the AutoConvert toggle still work in the secure field without
   converting text.

Record any custom control for which secure-field detection is unavailable. An inaccessible custom
password control is a known fail-open boundary and must not be marked as protected.

## Clipboard

1. Put a recognizable synthetic value in the clipboard.
2. Run Last Word, Last Sequence, and Selection conversions.
3. Paste afterward and confirm the original clipboard value is unchanged.
4. Confirm normal Ctrl+V clears the prior journal context instead of importing clipboard contents.

## Runtime network observation

Start the reviewed release executable and identify its PID. During ordinary typing, every
conversion action, AutoConvert enable/disable, settings save, and autostart toggle, run:

```powershell
Get-NetTCPConnection -OwningProcess <PID> -ErrorAction SilentlyContinue
```

Expected result: no TCP connection owned by the process. Optionally corroborate with TCPView,
Process Explorer, or Resource Monitor. Record the observation interval and tool; a point-in-time
observation is supporting evidence, not proof of permanent network absence.

## Storage, autostart, and privilege checks

1. Save settings and confirm only the documented config path changes.
2. Enable autostart and confirm only
   `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\RustSwitcher` is created or updated.
3. Disable autostart and confirm that value is removed.
4. Confirm the process runs without elevation and does not display a UAC prompt.
5. Confirm no service, Scheduled Task, driver, HKLM Run value, or executable in a temporary/system
   directory is created.

## Acceptance record

For each test record `Pass`, `Fail`, or `Not tested`. A source audit or automated unit test must not
be recorded as manual Windows acceptance. Any failed password-field case blocks release use in
that application until it is understood.
