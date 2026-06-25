//! Layout-aware translation for macOS global-shortcut keys.
//!
//! macOS registers global hotkeys by PHYSICAL key position (`RegisterEventHotKey`
//! takes a positional virtual keycode), so an accelerator like `Cmd+Alt+T`
//! fires on the key at the QWERTY-T position — which on Dvorak is not the key
//! that types "t". To honor the *character* the user configured, we find the
//! keycode that produces that character in the CURRENT layout, then express it
//! as the QWERTY/ANSI letter sitting at that same position (the token the
//! global-shortcut plugin maps back to that keycode).

/// The QWERTY/ANSI character at macOS virtual keycode `keycode` (letters and
/// digits only — the keys that appear in shortcuts). This is the fixed
/// `kVK_ANSI_*` table; punctuation/function keys return `None` and are left
/// untranslated.
pub(crate) fn ansi_char(keycode: u16) -> Option<char> {
    Some(match keycode {
        0 => 'a',
        1 => 's',
        2 => 'd',
        3 => 'f',
        4 => 'h',
        5 => 'g',
        6 => 'z',
        7 => 'x',
        8 => 'c',
        9 => 'v',
        11 => 'b',
        12 => 'q',
        13 => 'w',
        14 => 'e',
        15 => 'r',
        16 => 'y',
        17 => 't',
        18 => '1',
        19 => '2',
        20 => '3',
        21 => '4',
        22 => '6',
        23 => '5',
        25 => '9',
        26 => '7',
        28 => '8',
        29 => '0',
        31 => 'o',
        32 => 'u',
        34 => 'i',
        35 => 'p',
        37 => 'l',
        38 => 'j',
        40 => 'k',
        45 => 'n',
        46 => 'm',
        _ => return None,
    })
}

/// The ANSI token to register so a shortcut fires on the key that TYPES
/// `target` in the active layout, or `None` to leave the accelerator as-is
/// (character not on the main keyboard, or the layout already matches QWERTY
/// so no rewrite is needed). Suitable as the `map` for
/// [`super::rewrite_accelerator_key`].
#[cfg(target_os = "macos")]
pub(crate) fn layout_token(target: char) -> Option<char> {
    let keycode = imp::keycode_for_char(target)?;
    let ansi = ansi_char(keycode)?;
    // On QWERTY the character already sits at its own ANSI position; skip the
    // rewrite so nothing changes for the common case.
    (ansi != target).then_some(ansi)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::ansi_char;
    use std::os::raw::c_void;

    #[allow(non_upper_case_globals)]
    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        static kTISPropertyUnicodeKeyLayoutData: *const c_void; // CFStringRef
        fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut c_void;
        fn TISGetInputSourceProperty(source: *mut c_void, key: *const c_void) -> *mut c_void;
        fn LMGetKbdType() -> u8;
        #[allow(clippy::too_many_arguments)]
        fn UCKeyTranslate(
            key_layout_ptr: *const u8,
            virtual_key_code: u16,
            key_action: u16,
            modifier_key_state: u32,
            keyboard_type: u32,
            key_translate_options: u32,
            dead_key_state: *mut u32,
            max_string_length: usize,
            actual_string_length: *mut usize,
            unicode_string: *mut u16,
        ) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
        fn CFRelease(cf: *const c_void);
    }

    const K_UC_KEY_ACTION_DOWN: u16 = 0;
    const K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK: u32 = 1;

    /// The character produced by `keycode` with no modifiers under `layout`.
    /// # Safety: `layout` must point at the current layout's UCKeyboardLayout.
    unsafe fn char_at(layout: *const u8, kbd_type: u32, keycode: u16) -> Option<char> {
        let mut dead_state = 0u32;
        let mut buf = [0u16; 4];
        let mut len = 0usize;
        let status = UCKeyTranslate(
            layout,
            keycode,
            K_UC_KEY_ACTION_DOWN,
            0,
            kbd_type,
            K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK,
            &mut dead_state,
            buf.len(),
            &mut len,
            buf.as_mut_ptr(),
        );
        if status != 0 || len == 0 {
            return None;
        }
        char::from_u32(u32::from(buf[0]))
    }

    /// The virtual keycode whose unmodified character is `target` in the
    /// current keyboard layout, scanning the main keyboard range.
    pub(crate) fn keycode_for_char(target: char) -> Option<u16> {
        // SAFETY: standard Text Input Source / UCKeyTranslate sequence. The
        // input source is released before returning; the layout data pointer
        // is owned by the (still-retained) source for the loop's duration.
        unsafe {
            let source = TISCopyCurrentKeyboardLayoutInputSource();
            if source.is_null() {
                return None;
            }
            let data = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
            if data.is_null() {
                CFRelease(source);
                return None;
            }
            let layout = CFDataGetBytePtr(data);
            let kbd_type = u32::from(LMGetKbdType());
            let mut found = None;
            // Only positions that map to a shortcut-legal ANSI char matter.
            for keycode in 0u16..128 {
                if ansi_char(keycode).is_some()
                    && char_at(layout, kbd_type, keycode) == Some(target)
                {
                    found = Some(keycode);
                    break;
                }
            }
            CFRelease(source);
            found
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ansi_char;
    use crate::platform::rewrite_accelerator_key;

    #[test]
    fn ansi_table_maps_known_positions() {
        assert_eq!(ansi_char(17), Some('t')); // kVK_ANSI_T
        assert_eq!(ansi_char(0), Some('a'));
        assert_eq!(ansi_char(40), Some('k'));
        assert_eq!(ansi_char(18), Some('1'));
        assert_eq!(ansi_char(24), None); // '=' — not a shortcut letter/digit
        assert_eq!(ansi_char(200), None);
    }

    #[test]
    fn rewrite_replaces_only_the_key_token() {
        // Dvorak: the key that types 't' sits at the QWERTY-K position, so the
        // accelerator is registered as …+K.
        let out = rewrite_accelerator_key("CmdOrCtrl+Alt+T", |c| {
            assert_eq!(c, 't');
            Some('k')
        });
        assert_eq!(out, "CmdOrCtrl+Alt+K");
    }

    #[test]
    fn rewrite_is_identity_when_map_returns_same_or_none() {
        assert_eq!(
            rewrite_accelerator_key("CmdOrCtrl+Alt+T", |_| Some('t')),
            "CmdOrCtrl+Alt+T"
        );
        assert_eq!(
            rewrite_accelerator_key("CmdOrCtrl+Alt+T", |_| None),
            "CmdOrCtrl+Alt+T"
        );
    }

    #[test]
    fn rewrite_leaves_non_single_char_and_bare_keys_alone() {
        // Function keys and named keys aren't single characters.
        assert_eq!(
            rewrite_accelerator_key("CmdOrCtrl+F1", |_| Some('z')),
            "CmdOrCtrl+F1"
        );
        // A bare key with no modifiers is returned untouched.
        assert_eq!(rewrite_accelerator_key("T", |_| Some('k')), "T");
    }
}
