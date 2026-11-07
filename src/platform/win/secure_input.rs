use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, RPC_E_CHANGED_MODE, SetLastError, WIN32_ERROR},
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        UI::{
            Accessibility::{CUIAutomation, IUIAutomation},
            WindowsAndMessaging::{
                ES_PASSWORD, GUITHREADINFO, GWL_STYLE, GetClassNameW, GetForegroundWindow,
                GetGUIThreadInfo, GetWindowLongPtrW, GetWindowThreadProcessId,
            },
        },
    },
};

use crate::{
    app::RuntimeCommand,
    input::hotkeys::HotkeyAction,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SecureInputStatus {
    Secure,
    NotSecure,
    Unknown,
}

impl SecureInputStatus {
    pub(crate) const fn is_secure(self) -> bool {
        matches!(self, Self::Secure)
    }
}

struct CoUninitGuard(bool);

impl Drop for CoUninitGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

fn classify_secure_input(
    uia_is_password: Option<bool>,
    win32_is_password: Option<bool>,
) -> SecureInputStatus {
    if uia_is_password == Some(true) || win32_is_password == Some(true) {
        SecureInputStatus::Secure
    } else if uia_is_password == Some(false) || win32_is_password == Some(false) {
        SecureInputStatus::NotSecure
    } else {
        SecureInputStatus::Unknown
    }
}

fn uia_is_password() -> Option<bool> {
    let should_uninit = match unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok() {
        Ok(()) => true,
        Err(e) if e.code() == RPC_E_CHANGED_MODE => false,
        Err(_) => return None,
    };
    let _com_guard = CoUninitGuard(should_uninit);

    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let focused = unsafe { automation.GetFocusedElement() }.ok()?;
    unsafe { focused.CurrentIsPassword() }
        .ok()
        .map(|value| value.as_bool())
}

pub(crate) fn focused_hwnd() -> Option<HWND> {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.0.is_null() {
        return None;
    }

    let thread_id = unsafe { GetWindowThreadProcessId(foreground, None) };
    if thread_id == 0 {
        return None;
    }

    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetGUIThreadInfo(thread_id, &mut info) }.is_err() || info.hwndFocus.0.is_null() {
        return None;
    }

    Some(info.hwndFocus)
}

fn is_supported_edit_control(hwnd: HWND) -> bool {
    let mut class_name = [0u16; 128];
    let len = unsafe { GetClassNameW(hwnd, &mut class_name) };
    if len <= 0 {
        return false;
    }

    let class = String::from_utf16_lossy(&class_name[..len as usize]);
    class == "Edit" || class.starts_with("RichEdit") || class.starts_with("RICHEDIT")
}

pub(crate) fn win32_password_state(hwnd: HWND) -> Option<bool> {
    if !is_supported_edit_control(hwnd) {
        return None;
    }

    unsafe { SetLastError(WIN32_ERROR::default()) };
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
    if style == 0 && unsafe { GetLastError() } != WIN32_ERROR::default() {
        return None;
    }
    Some((style & ES_PASSWORD as isize) != 0)
}

pub(crate) fn detect_current() -> SecureInputStatus {
    let win32_is_password = focused_hwnd().and_then(win32_password_state);
    if win32_is_password == Some(true) {
        return SecureInputStatus::Secure;
    }

    classify_secure_input(uia_is_password(), win32_is_password)
}

pub(crate) fn protect_current_context() -> SecureInputStatus {
    let status = detect_current();
    if status.is_secure() {
        crate::input::ring_buffer::clear_secure_input_data();
    }
    status
}

pub(crate) const fn should_record_input(status: SecureInputStatus) -> bool {
    !status.is_secure()
}

pub(crate) const fn blocks_command(
    status: SecureInputStatus,
    command: RuntimeCommand,
) -> bool {
    status.is_secure()
        && matches!(
            command,
            RuntimeCommand::AutoconvertLastWord
                | RuntimeCommand::Hotkey(HotkeyAction::ConvertLastWord)
                | RuntimeCommand::Hotkey(HotkeyAction::ConvertLastSequence)
                | RuntimeCommand::Hotkey(HotkeyAction::ConvertSelection)
                | RuntimeCommand::Hotkey(HotkeyAction::SmartConvertLastWord)
                | RuntimeCommand::Hotkey(HotkeyAction::SmartConvertLastSequence)
                | RuntimeCommand::Hotkey(HotkeyAction::SmartConvertSelection)
        )
}

pub(crate) const fn command_kind(command: RuntimeCommand) -> &'static str {
    match command {
        RuntimeCommand::AutoconvertLastWord => "autoconvert_last_word",
        RuntimeCommand::Hotkey(HotkeyAction::ConvertLastWord) => "convert_last_word",
        RuntimeCommand::Hotkey(HotkeyAction::ConvertLastSequence) => "convert_last_sequence",
        RuntimeCommand::Hotkey(HotkeyAction::PauseToggle) => "autoconvert_toggle",
        RuntimeCommand::Hotkey(HotkeyAction::ConvertSelection) => "convert_selection",
        RuntimeCommand::Hotkey(HotkeyAction::SwitchLayout) => "switch_layout",
        RuntimeCommand::Hotkey(HotkeyAction::SmartConvertLastWord) => "smart_convert_last_word",
        RuntimeCommand::Hotkey(HotkeyAction::SmartConvertLastSequence) => {
            "smart_convert_last_sequence"
        }
        RuntimeCommand::Hotkey(HotkeyAction::SmartConvertSelection) => "smart_convert_selection",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_password_style_is_detected_without_reading_control_text() -> windows::core::Result<()> {
        use windows::{
            Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WINDOW_STYLE,
            },
            core::{PCWSTR, w},
        };

        for password in [false, true] {
            // Hidden, isolated controls: no foreground changes or synthetic typing.
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("Edit"),
                    PCWSTR::null(),
                    WINDOW_STYLE(if password { ES_PASSWORD.cast_unsigned() } else { 0 }),
                    0, 0, 100, 24,
                    None, None, None, None,
                )
            }?;
            let observed = win32_password_state(hwnd);
            unsafe { DestroyWindow(hwnd) }?;
            assert_eq!(observed, Some(password));
        }
        Ok(())
    }

    #[test]
    fn detection_prefers_any_positive_secure_signal() {
        assert_eq!(
            classify_secure_input(Some(false), Some(true)),
            SecureInputStatus::Secure
        );
        assert_eq!(
            classify_secure_input(Some(true), Some(false)),
            SecureInputStatus::Secure
        );
        assert_eq!(
            classify_secure_input(None, None),
            SecureInputStatus::Unknown
        );
    }

    #[test]
    fn journaling_is_blocked_only_for_confirmed_secure_input() {
        assert!(!should_record_input(SecureInputStatus::Secure));
        assert!(should_record_input(SecureInputStatus::NotSecure));
        assert!(should_record_input(SecureInputStatus::Unknown));
    }

    #[test]
    fn secure_input_blocks_all_text_conversion_commands() {
        let secure = SecureInputStatus::Secure;
        for command in [
            RuntimeCommand::AutoconvertLastWord,
            RuntimeCommand::Hotkey(HotkeyAction::ConvertLastWord),
            RuntimeCommand::Hotkey(HotkeyAction::ConvertLastSequence),
            RuntimeCommand::Hotkey(HotkeyAction::ConvertSelection),
            RuntimeCommand::Hotkey(HotkeyAction::SmartConvertLastWord),
            RuntimeCommand::Hotkey(HotkeyAction::SmartConvertLastSequence),
            RuntimeCommand::Hotkey(HotkeyAction::SmartConvertSelection),
        ] {
            assert!(blocks_command(secure, command), "{command:?}");
        }

        assert!(!blocks_command(
            secure,
            RuntimeCommand::Hotkey(HotkeyAction::PauseToggle)
        ));
        assert!(!blocks_command(
            secure,
            RuntimeCommand::Hotkey(HotkeyAction::SwitchLayout)
        ));
    }

    #[test]
    fn non_secure_and_unknown_contexts_preserve_existing_commands() {
        for status in [SecureInputStatus::NotSecure, SecureInputStatus::Unknown] {
            assert!(!blocks_command(
                status,
                RuntimeCommand::AutoconvertLastWord
            ));
            assert!(!blocks_command(
                status,
                RuntimeCommand::Hotkey(HotkeyAction::ConvertLastWord)
            ));
        }
    }
}
