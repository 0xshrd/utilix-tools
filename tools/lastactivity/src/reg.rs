//! Schlanker, nur lesender Zugriff auf die Registry.

use crate::util::{filetime_to_utc, from_wide, wide};
use chrono::{DateTime, Utc};
use windows::Win32::Foundation::{ERROR_SUCCESS, FILETIME};
use windows::Win32::System::Registry::{
    HKEY, KEY_READ, REG_ROUTINE_FLAGS, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegCloseKey,
    RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};
use windows::core::{PCWSTR, PWSTR};

pub use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

pub struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn to_utc(ft: FILETIME) -> Option<DateTime<Utc>> {
    filetime_to_utc(((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64)
}

impl Key {
    pub fn open(root: HKEY, path: &str) -> Option<Key> {
        let path_w = wide(path);
        let mut key = HKEY::default();
        let status =
            unsafe { RegOpenKeyExW(root, PCWSTR(path_w.as_ptr()), None, KEY_READ, &mut key) };
        (status == ERROR_SUCCESS).then_some(Key(key))
    }

    pub fn open_sub(&self, path: &str) -> Option<Key> {
        Key::open(self.0, path)
    }

    /// Namen der Unterschlüssel mit ihrer letzten Schreibzeit.
    pub fn subkeys(&self) -> Vec<(String, Option<DateTime<Utc>>)> {
        let mut keys = Vec::new();
        for index in 0.. {
            let mut name = [0u16; 256];
            let mut name_len = name.len() as u32;
            let mut written = FILETIME::default();
            let status = unsafe {
                RegEnumKeyExW(
                    self.0,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    None,
                    None,
                    Some(&mut written),
                )
            };
            if status != ERROR_SUCCESS {
                break;
            }
            keys.push((from_wide(&name), to_utc(written)));
        }
        keys
    }

    fn raw(&self, sub: &str, value: &str, flags: REG_ROUTINE_FLAGS) -> Option<Vec<u8>> {
        let (sub_w, value_w) = (wide(sub), wide(value));
        let sub_ptr = if sub.is_empty() {
            PCWSTR::null()
        } else {
            PCWSTR(sub_w.as_ptr())
        };
        let mut size = 0u32;
        let status = unsafe {
            RegGetValueW(
                self.0,
                sub_ptr,
                PCWSTR(value_w.as_ptr()),
                flags,
                None,
                None,
                Some(&mut size),
            )
        };
        if status != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let status = unsafe {
            RegGetValueW(
                self.0,
                sub_ptr,
                PCWSTR(value_w.as_ptr()),
                flags,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        buf.truncate(size as usize);
        (status == ERROR_SUCCESS).then_some(buf)
    }

    pub fn binary(&self, value: &str) -> Option<Vec<u8>> {
        self.raw("", value, RRF_RT_REG_BINARY)
    }

    /// Zeichenkette aus einem Unterschlüssel (`sub` leer = dieser Schlüssel).
    pub fn string(&self, sub: &str, value: &str) -> Option<String> {
        let bytes = self.raw(sub, value, RRF_RT_REG_SZ)?;
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        Some(from_wide(&units).trim().to_string()).filter(|s| !s.is_empty())
    }
}
