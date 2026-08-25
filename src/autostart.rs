use std::error::Error;

use auto_launch::{AutoLaunch, AutoLaunchBuilder, MacOSLaunchMode};

use crate::config::APP_DIR_NAME;

/// 用户级开机自启动(Windows: HKCU Run / macOS: LaunchAgent / Linux: XDG autostart)
fn auto() -> Result<AutoLaunch, Box<dyn Error>> {
    let exe = std::env::current_exe()?;
    let path = exe.to_string_lossy().to_string();
    Ok(AutoLaunchBuilder::new()
        .set_app_name(APP_DIR_NAME)
        .set_app_path(&path)
        .set_macos_launch_mode(MacOSLaunchMode::LaunchAgent)
        .build()?)
}

pub fn enable() -> Result<(), Box<dyn Error>> {
    auto()?.enable().map_err(Into::into)
}

pub fn disable() -> Result<(), Box<dyn Error>> {
    auto()?.disable().map_err(Into::into)
}
