use anyhow::Result;

#[cfg(windows)]
pub fn enabled() -> Result<bool> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let key = match RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
    {
        Ok(key) => key,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    let actual: String = key.get_value("NotesAI").unwrap_or_default();
    Ok(actual == command()?)
}
#[cfg(windows)]
fn command() -> Result<String> {
    Ok(format!(
        "\"{}\" --background",
        std::env::current_exe()?.display()
    ))
}
#[cfg(windows)]
pub fn set_enabled(enabled: bool) -> Result<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    anyhow::ensure!(
        std::env::var_os("NOTESAI_DATA_DIR").is_none(),
        "Startup changes are disabled in an isolated test instance"
    );
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")?;
    if enabled {
        key.set_value("NotesAI", &command()?)?;
    } else if let Err(e) = key.delete_value("NotesAI") {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(e.into());
        }
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn enabled() -> Result<bool> {
    Ok(false)
}
#[cfg(not(windows))]
pub fn set_enabled(_: bool) -> Result<()> {
    anyhow::bail!("Login startup is currently supported on Windows")
}
