#![windows_subsystem = "windows"]
use dwmr_win32::*;
use windows::{
    core::*,
    Win32::{Foundation::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*},
};

/// <summary>
/// 程序入口函数，负责初始化窗口管理器并启动消息循环
/// 增加了错误捕获和提示，避免直接闪退
/// </summary>
fn run_app() -> Result<()> {
    unsafe {
        let hmodule = GetModuleHandleW(None)?;
        let hinstance: HINSTANCE = hmodule.into();
        let mut app = DwmrApp::default();
        app.setup(&hinstance)?;
        app.scan()?;
        app.arrange()?;
        DwmrApp::run()?;
    }
    Ok(())
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("程序发生了错误:\n{}\n\n请检查:\n1. 是否以管理员权限运行\n2. DWM(桌面窗口管理器)是否正常运行\n3. 是否有其他窗口管理器冲突", info);
        eprintln!("{}", msg);
        #[cfg(windows)]
        unsafe {
            MessageBoxW(
                None,
                windows::core::w!("程序启动失败，请查看控制台或日志获取详细信息"),
                windows::core::w!("dwmr-win32 错误"),
                MB_ICONERROR | MB_OK,
            );
        }
    }));

    if let Err(e) = run_app() {
        let msg = format!("程序启动失败: {:?}\n\n请检查:\n1. 是否以管理员权限运行\n2. DWM(桌面窗口管理器)是否正常运行\n3. 是否有其他窗口管理器冲突", e);
        eprintln!("{}", msg);
        #[cfg(windows)]
        unsafe {
            MessageBoxW(
                None,
                windows::core::w!("程序启动失败，请查看控制台或日志获取详细信息"),
                windows::core::w!("dwmr-win32 错误"),
                MB_ICONERROR | MB_OK,
            );
        }
    }
}
