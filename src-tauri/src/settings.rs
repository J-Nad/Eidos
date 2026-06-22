use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{ERROR_SUCCESS, HLOCAL, LocalFree},
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
            CRYPTPROTECT_UI_FORBIDDEN,
        },
        System::Registry::{
            RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
            KEY_SET_VALUE, REG_SZ,
        },
    },
};

const PROTECTED_PREFIX: &str = "dpapi:v1:";

fn protect_secret(secret: &str) -> Result<String, String> {
    if secret.is_empty() {
        return Ok(String::new());
    }
    let mut bytes = secret.as_bytes().to_vec();
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().map_err(|_| "API key is too long.".to_string())?,
        pbData: bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| format!("Windows could not protect the API key: {error}"))?;
        let protected = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let encoded = STANDARD.encode(protected);
        let _ = LocalFree(HLOCAL(output.pbData.cast()));
        Ok(format!("{PROTECTED_PREFIX}{encoded}"))
    }
}

fn unprotect_secret(value: &str) -> Result<String, String> {
    let Some(encoded) = value.strip_prefix(PROTECTED_PREFIX) else {
        return Ok(value.to_string());
    };
    let mut bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "The stored API key is corrupted.".to_string())?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().map_err(|_| "Stored API key is too long.".to_string())?,
        pbData: bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None::<*mut PWSTR>,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| format!("Windows could not unlock the stored API key: {error}"))?;
        let clear = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(HLOCAL(output.pbData.cast()));
        String::from_utf8(clear)
            .map_err(|_| "The stored API key is not valid UTF-8.".to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub selected_drives: Vec<String>,
    pub default_save_location: String,
    pub autostart: bool,
    pub gemini_api_key: String,
}

impl Settings {
    pub fn default_for(drives: &[PathBuf]) -> Self {
        let c_drive = drives
            .iter()
            .find(|path| path.to_string_lossy().to_ascii_uppercase().starts_with("C:"))
            .or_else(|| drives.first())
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "C:\\".to_string());
        Self {
            selected_drives: vec![normalize_drive(&c_drive)],
            default_save_location: dirs::desktop_dir()
                .or_else(dirs::document_dir)
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| PathBuf::from("C:\\"))
                .to_string_lossy()
                .into_owned(),
            autostart: false,
            gemini_api_key: String::new(),
        }
    }
}

pub fn normalize_drive(drive: &str) -> String {
    let trimmed = drive.trim().replace('/', "\\");
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        format!("{trimmed}\\")
    } else if trimmed.len() == 1 && trimmed.as_bytes()[0].is_ascii_alphabetic() {
        format!("{}:\\", trimmed.to_ascii_uppercase())
    } else if trimmed.ends_with('\\') {
        trimmed
    } else {
        format!("{trimmed}\\")
    }
}

pub fn load_settings(path: &Path, available_drives: &[PathBuf]) -> Result<Settings, String> {
    if !path.exists() {
        let settings = Settings::default_for(available_drives);
        save_settings(path, &settings)?;
        return Ok(settings);
    }
    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut settings = serde_json::from_str::<Settings>(&raw).map_err(|error| error.to_string())?;
    let needs_secret_migration = !settings.gemini_api_key.is_empty()
        && !settings.gemini_api_key.starts_with(PROTECTED_PREFIX);
    settings.gemini_api_key = unprotect_secret(&settings.gemini_api_key)?;
    if settings.selected_drives.is_empty() {
        settings.selected_drives = Settings::default_for(available_drives).selected_drives;
    }
    settings.selected_drives = settings
        .selected_drives
        .iter()
        .map(|drive| normalize_drive(drive))
        .collect();
    if settings.default_save_location.trim().is_empty() {
        settings.default_save_location = Settings::default_for(available_drives).default_save_location;
    }
    if needs_secret_migration {
        save_settings(path, &settings)?;
    }
    Ok(settings)
}

pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut persisted = settings.clone();
    persisted.gemini_api_key = protect_secret(settings.gemini_api_key.trim())?;
    let raw = serde_json::to_string_pretty(&persisted).map_err(|error| error.to_string())?;
    fs::write(path, raw).map_err(|error| error.to_string())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn set_autostart(enabled: bool, exe_path: &Path) -> Result<(), String> {
    let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let mut key = HKEY::default();
    let open_result = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_SET_VALUE,
            &mut key,
        )
    };
    if open_result != ERROR_SUCCESS {
        return Err(format!("Could not open Windows autostart registry key: {open_result:?}"));
    }

    let name = wide("Eidos");
    let result = if enabled {
        let quoted = format!("\"{}\"", exe_path.display());
        let data = wide(&quoted);
        unsafe {
            RegSetValueExW(
                key,
                PCWSTR(name.as_ptr()),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    data.as_ptr().cast::<u8>(),
                    data.len() * std::mem::size_of::<u16>(),
                )),
            )
        }
    } else {
        unsafe { RegDeleteValueW(key, PCWSTR(name.as_ptr())) }
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    if !enabled || result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("Could not update Windows autostart entry: {result:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_api_keys_with_windows_dpapi() {
        let protected = protect_secret("demo-secret-value").expect("protect secret");
        assert!(protected.starts_with(PROTECTED_PREFIX));
        assert!(!protected.contains("demo-secret-value"));
        assert_eq!(unprotect_secret(&protected).expect("unprotect secret"), "demo-secret-value");
    }
}
