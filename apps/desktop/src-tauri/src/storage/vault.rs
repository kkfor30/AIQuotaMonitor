//! Windows Credential Manager 封装。
//!
//! SQLite 只持久化 `secret_ref`；秘密内容仅在 Rust 内短暂存在，绝不进入
//! ViewModel、日志或数据库。

const TARGET_PREFIX: &str = "AIQuotaMonitor/";

pub fn secret_ref(account_id: &str, source_id: &str) -> String {
    format!("{TARGET_PREFIX}{account_id}/{source_id}")
}

#[cfg(windows)]
mod platform {
    use super::TARGET_PREFIX;
    use std::ptr::{null_mut, NonNull};
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn validate_ref(secret_ref: &str) -> Result<(), String> {
        if secret_ref.starts_with(TARGET_PREFIX) && !secret_ref.contains('\0') {
            Ok(())
        } else {
            Err("无效的凭据引用".into())
        }
    }

    pub fn set(secret_ref: &str, secret: &str) -> Result<(), String> {
        validate_ref(secret_ref)?;
        if secret.is_empty() {
            return Err("凭据不能为空".into());
        }
        let mut target = wide(secret_ref);
        let mut username = wide("AIQuotaMonitor");
        let mut blob = secret.as_bytes().to_vec();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: username.as_mut_ptr(),
            ..Default::default()
        };
        let ok = unsafe { CredWriteW(&credential, 0) };
        blob.fill(0);
        if ok == 0 {
            return Err(format!("保存 Windows 凭据失败: {}", unsafe {
                GetLastError()
            }));
        }
        Ok(())
    }

    pub fn get(secret_ref: &str) -> Result<Option<String>, String> {
        validate_ref(secret_ref)?;
        let target = wide(secret_ref);
        let mut raw: *mut CREDENTIALW = null_mut();
        let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) };
        if ok == 0 {
            let error = unsafe { GetLastError() };
            if error == ERROR_NOT_FOUND {
                return Ok(None);
            }
            return Err(format!("读取 Windows 凭据失败: {error}"));
        }
        let pointer = NonNull::new(raw).ok_or_else(|| "Windows 凭据返回空指针".to_string())?;
        let credential = unsafe { pointer.as_ref() };
        let bytes = unsafe {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
        };
        let result =
            String::from_utf8(bytes.to_vec()).map_err(|_| "Windows 凭据不是有效 UTF-8".to_string());
        unsafe { CredFree(raw.cast()) };
        result.map(Some)
    }

    pub fn delete(secret_ref: &str) -> Result<(), String> {
        validate_ref(secret_ref)?;
        let target = wide(secret_ref);
        let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if ok == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_NOT_FOUND {
                return Err(format!("删除 Windows 凭据失败: {error}"));
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn set(_secret_ref: &str, _secret: &str) -> Result<(), String> {
        Err("Credential Vault 仅支持 Windows".into())
    }
    pub fn get(_secret_ref: &str) -> Result<Option<String>, String> {
        Err("Credential Vault 仅支持 Windows".into())
    }
    pub fn delete(_secret_ref: &str) -> Result<(), String> {
        Err("Credential Vault 仅支持 Windows".into())
    }
}

pub use platform::{delete, get, set};
