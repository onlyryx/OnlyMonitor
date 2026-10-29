// Local Windows reliability patch; upstream auto-launch 0.5.0, MIT.
use crate::{AutoLaunch, Result};
use std::io;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

const AL_REGKEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";

impl AutoLaunch {
    pub fn new(app_name: &str, app_path: &str, args: &[impl AsRef<str>]) -> AutoLaunch {
        AutoLaunch { app_name: app_name.into(), app_path: app_path.into(),
            args: args.iter().map(|s| s.as_ref().to_string()).collect() }
    }

    fn command(&self) -> String {
        format!("\"{}\" {}", self.app_path, self.args.join(" "))
    }

    fn matches_registration(&self, value: io::Result<String>) -> Result<bool> {
        match value {
            Ok(value) if value == self.command() => Ok(true),
            Ok(_) => Err(io::Error::other("同名启动项指向其他位置；可显式启用以更新路径").into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn enable(&self) -> Result<()> {
        // Register only on explicit user action. Never change StartupApproved.
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(AL_REGKEY)?;
        key.set_value(&self.app_name, &self.command())?;
        Ok(())
    }

    pub fn disable(&self) -> Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = match hkcu.open_subkey_with_flags(AL_REGKEY, KEY_READ | KEY_SET_VALUE) {
            Ok(key) => key,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if self.matches_registration(key.get_value(&self.app_name))? {
            match key.delete_value(&self.app_name) {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub fn is_enabled(&self) -> Result<bool> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        match hkcu.open_subkey_with_flags(AL_REGKEY, KEY_READ) {
            Ok(key) => self.matches_registration(key.get_value(&self.app_name)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod reliability_tests {
    use super::*;
    #[test]
    fn path_with_spaces_is_quoted_and_read_errors_stay_errors() {
        let app = AutoLaunch::new("Only Monitor", r"C:\Program Files\Only Monitor\only-monitor.exe", &["--autostart"]);
        assert_eq!(app.command(), r#""C:\Program Files\Only Monitor\only-monitor.exe" --autostart"#);
        assert!(app.matches_registration(Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))).is_err());
        assert!(!app.matches_registration(Err(std::io::Error::from(std::io::ErrorKind::NotFound))).unwrap());
        assert!(app.matches_registration(Ok(app.command())).unwrap());
        assert!(app.matches_registration(Ok("another copy".into())).is_err());
    }
}
