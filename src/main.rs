use windows::{
    core::*,
    Win32::{
        Foundation::*,
        System::LibraryLoader::*,
        UI::WindowsAndMessaging::*,
        Graphics::{
            Dwm::*,
            Gdi::*
        }
    }
};
use std::{
    sync::*, 
    collections::*,
    mem::size_of,
    usize,
    cmp::*,
    ops::*
};

mod test;

// a macro to check bit flags for u32
macro_rules! has_flag {
    ($flags:expr, $flag:expr) => {
        $flags & $flag == $flag
    };
}

#[macro_use]
extern  crate lazy_static;

const W_APP_NAME: PCWSTR = w!("dwmr-win32");
const S_APP_NAME: PCSTR = s!("dwmr-win32");

const BAR_HEIGHT: i32 = 20;


#[derive(Default, Clone, Debug)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Rect {
    fn from_win_rect(rect: &RECT) -> Rect {
        Rect {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top
        }
    }
}

#[derive(Default, Debug)]
struct Monitor {
    //LPCWSTR type
    name: [u16; 32],
    master_count: u32,
    master_factor: f32,
    index: u32,
    bar_y: i32,
    rect: Rect,
    client_area: Rect,
    selected_client: RwLock<Option<HWND>>,
    clients: RwLock<Vec<Client>>
}

impl Monitor {
    unsafe fn arrangemon(&self) -> Result<()> {
        tile(self)?;
        Ok(())
    }
}

unsafe fn arrange() -> Result<()> {
    let monitors = DWMR_APP.monitors.read().unwrap();
    for monitor in monitors.iter() {
        monitor.arrangemon()?;
    }

    Ok(())
}

#[derive(Default, Clone, Debug)]
struct Client {
    hwnd: HWND,
    parent: HWND,
    root: HWND,
    rect: Rect,
    bw: i32,
    tags: u32,
    is_minimized: bool,
    is_floating: bool,
    is_ignored: bool,
    ignore_borders: bool,
    border: bool,
    was_visible: bool,
    is_fixed: bool,
    is_urgent: bool,
    is_cloaked: bool,
    monitor: std::sync::Weak<Monitor>,
}

#[derive(Default, Debug)]
struct DwmrApp {
    hwnd: RwLock<Option<HWND>>,
    monitors: RwLock<Vec<Arc<Monitor>>>,
    selected_monitor: RwLock<std::sync::Weak<Monitor>>,
}

lazy_static! {
    static ref DWMR_APP: DwmrApp = DwmrApp::default();
    static ref DISALLOWED_TITLE: HashSet<String> = HashSet::from([
        "Windows Shell Experience Host".to_string(),
        "Microsoft Text Input Application".to_string(),
        "Action center".to_string(),
        "New Notification".to_string(),
        "Date and Time Information".to_string(),
        "Volume Control".to_string(),
        "Network Connections".to_string(),
        "Cortana".to_string(),
        "Start".to_string(),
        "Windows Default Lock Screen".to_string(),
        "Search".to_string(),
        "WinUI Desktop".to_string()
    ]);

    static ref DISALLOWED_CLASS: HashSet<String> = HashSet::from([
        "Windows.UI.Core.CoreWindow".to_string(),
        "ForegroundStaging".to_string(),
        "ApplicationManager_DesktopShellWindow".to_string(),
        "Static".to_string(),
        "Scrollbar".to_string(),
        "Progman".to_string(),
    ]);
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT
{
    LRESULT::default()
}

unsafe extern "system" fn update_geom(hmonitor: HMONITOR, _: HDC, rect: *mut RECT, _: LPARAM) -> BOOL {
    let mut monitor_info = MONITORINFOEXW{
        monitorInfo: MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    if GetMonitorInfoW(hmonitor, &mut monitor_info.monitorInfo) == FALSE {
        return TRUE;
    }

    //unsigned shot to str
    let _monitor_name = PCWSTR::from_raw(monitor_info.szDevice.as_ptr()).to_string().unwrap();

    let monitor = Arc::new(Monitor{
        name: monitor_info.szDevice,
        index: DWMR_APP.monitors.read().unwrap().len() as u32,
        rect: Rect::from_win_rect(&monitor_info.monitorInfo.rcMonitor),
        client_area: Rect::from_win_rect(&monitor_info.monitorInfo.rcWork),
        master_count: 1,
        master_factor: 0.5,
        ..Default::default()
    });

    DWMR_APP.monitors.write().unwrap().push(monitor);
    TRUE
}

unsafe fn request_update_geom() -> Result<()> {
    let monitors = GetSystemMetrics(SM_CMONITORS) as usize;
    DWMR_APP.monitors.write().unwrap().reserve(monitors);


    if EnumDisplayMonitors(None, None, Some(update_geom), None) == FALSE {
        return Ok(());
    }
    Ok(())
}

unsafe fn is_cloaked(hwnd: &HWND) -> Result<bool> {
    let mut cloaked_val = 0;
    DwmGetWindowAttribute(*hwnd, DWMWA_CLOAKED, (&mut cloaked_val) as *const _ as *mut _, size_of::<u32>() as u32)?;
    let is_cloaked = cloaked_val > 0;

    Ok(is_cloaked)
}

pub unsafe fn is_manageable(hwnd: &HWND) -> Result<bool>
{
    let style = GetWindowLongW(*hwnd, GWL_STYLE) as u32;
    if has_flag!(style, WS_DISABLED.0) {
        return Ok(false);
    }

    let exstyle = GetWindowLongW(*hwnd, GWL_EXSTYLE) as u32;
    if has_flag!(exstyle, WS_EX_NOACTIVATE.0) {
        return Ok(false);
    }

    SetLastError(WIN32_ERROR(0));
    let name_length = GetWindowTextLengthW(*hwnd);
    if name_length == 0 {
        GetLastError()?;
        return Ok(false);
    }

    if is_cloaked(hwnd)? {
        return Ok(false);
    }

    let mut client_name_buf = [0u16; 256];
    SetLastError(WIN32_ERROR(0));
    if GetWindowTextW(*hwnd, client_name_buf.as_mut()) == 0 {
        GetLastError()?;
    }
    let client_name = PCWSTR::from_raw(client_name_buf.as_ptr()).to_string().unwrap();
    if DISALLOWED_TITLE.contains(&client_name) {
        return Ok(false);
    }

    let mut class_name_buf = [0u16; 256];
    SetLastError(WIN32_ERROR(0));
    if GetClassNameW(*hwnd, class_name_buf.as_mut()) == 0 {
        GetLastError()?;
    }
    let class_name = PCWSTR::from_raw(class_name_buf.as_ptr()).to_string().unwrap();
    if DISALLOWED_CLASS.contains(&class_name) {
        return Ok(false);
    }

    let parent = GetParent(*hwnd);
    let parent_exist = parent.0 != 0;
    let is_tool = has_flag!(exstyle, WS_EX_TOOLWINDOW.0);

    if !parent_exist {
        if is_tool {
            return Ok(false);
        } else {
            let result = IsWindowVisible(*hwnd) == TRUE;
            return Ok(result);
        }
    }

    if is_manageable(&parent)? == false {
        return Ok(false);
    }

    let is_app = has_flag!(exstyle, WS_EX_APPWINDOW.0);
    if is_tool || is_app {
        return Ok(true);
    }

    Ok(false)
}

unsafe fn get_root(hwnd: &HWND) -> Result<HWND> {
    let desktop_window = GetDesktopWindow();
    let mut current = hwnd.clone();
    let mut parent = GetWindow(current, GW_OWNER);

    while (parent.0 != 0) && (parent != desktop_window) {
        current = parent;
        parent = GetWindow(current, GW_OWNER);
    }

    Ok(current)
}

unsafe fn manage(hwnd: &HWND) -> Result<Client> {
    let mut window_info = WINDOWINFO {
        cbSize: size_of::<WINDOWINFO>() as u32,
        ..Default::default()
    };

    GetWindowInfo(*hwnd, &mut window_info)?;

    let parent = GetParent(*hwnd);
    let root = get_root(hwnd)?;
    let is_cloaked = is_cloaked(hwnd)?;
    let is_minimized = IsIconic(*hwnd) == TRUE;
    let rect = Rect::from_win_rect(&window_info.rcWindow);
    let center_x = rect.x + rect.width / 2;
    let center_y = rect.y + rect.height / 2;

    let monitors = DWMR_APP.monitors.read().unwrap();
    assert!(!monitors.is_empty());

    let mut monitor_index:usize = 0;
    for (index, monitor_iter) in monitors.iter().enumerate() {
        let monitor_rect = &monitor_iter.as_ref().rect;

        let left_check = monitor_rect.x <= center_x;
        let right_check = center_x <= monitor_rect.x + monitor_rect.width;
        let top_check = monitor_rect.y <= center_y;
        let bottom_check = center_y <= monitor_rect.y + monitor_rect.height;

        if left_check && right_check && top_check && bottom_check {
            monitor_index = index;
        }
    }

    let monitor = &monitors[monitor_index];
    let client = Client {
        hwnd: *hwnd,
        parent,
        root,
        rect: rect.into(),
        bw: 0,
        is_minimized,
        is_cloaked,
        monitor: Arc::downgrade(monitor),
        ..Default::default()
    };

    monitor.clients.write().unwrap().push(client.clone());
    Ok(client)
}

unsafe fn scan() -> Result<()> {
    EnumWindows(Some(scan_enum), None)?;

    let focus_hwnd = GetForegroundWindow();
    for monitor in DWMR_APP.monitors.write().unwrap().iter() {
        let clients = monitor.clients.read().unwrap();
        for client in clients.iter() {
            if client.hwnd != focus_hwnd {
                continue;
            }

            let selected_monitor = client.monitor.clone();
            *DWMR_APP.selected_monitor.write().unwrap() = selected_monitor.clone();
            if let Some(monitor_arc) = selected_monitor.upgrade() {
                *monitor_arc.selected_client.write().unwrap() = Some(focus_hwnd);
            }
        }
    }
    Ok(())
}

unsafe extern "system" fn scan_enum(hwnd: HWND, _: LPARAM) -> BOOL {
    if !is_manageable(&hwnd).unwrap() {
        return TRUE;
    }
    let _ = manage(&hwnd);
    TRUE
}

unsafe fn setup(hinstance: &HINSTANCE) -> Result<()> {
    let wnd_class = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: *hinstance,
        lpszClassName: W_APP_NAME,
        ..Default::default()
    };

    request_update_geom()?;

    //EnumWindows(Some(scan_enum), None)?;

    if RegisterClassW(&wnd_class) == 0{
        GetLastError()?;
    }

    let hwnd_result = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        W_APP_NAME,
        W_APP_NAME,
        WINDOW_STYLE::default(),
        0,
        0,
        0,
        0,
        None,
        None,
        *hinstance,
        None,
    );

    if hwnd_result.0 == 0 {
        GetLastError()?;
    }

    let mut hwnd = DWMR_APP.hwnd.write().unwrap();
    *hwnd = Some(hwnd_result);
    Ok(())
}

unsafe fn is_tiled(client: &Client) -> bool {
    !client.is_floating
}

unsafe fn tile(monitor: &Monitor) -> Result<()> {
    let mut clients = monitor.clients.write().unwrap();

    let mut tiled_count: u32 = 0;
    for client in clients.iter() {
        tiled_count += is_tiled(client) as u32;
    }

    if tiled_count <= 0 {
        return Ok(());
    }

    //let mut master_width = 0;
    let mut master_y: u32 = 0;
    let mut stack_y: u32 = 0;

    let master_width = if tiled_count > monitor.master_count {
        if monitor.master_count > 0 {
            ((monitor.rect.width as f32) * monitor.master_factor) as i32
        } else {
            0
        }
    } else {
        monitor.rect.width
    };

    for (index, client) in clients.iter_mut().rev().enumerate() {
        if !is_tiled(client) {
            continue;
        }

        let is_master = index < monitor.master_count as usize;
        let rect = if is_master {
            let height: u32 = (monitor.client_area.height as u32 - master_y) / (min(tiled_count, monitor.master_count) - (index as u32));
            Rect {
                x: monitor.client_area.x,
                y: monitor.client_area.y + master_y as i32,
                width: master_width,
                height: height as i32
            }
        } else {
            let height: u32 = (monitor.client_area.height as u32 - stack_y) / (tiled_count - (index as u32));
            Rect {
                x: monitor.client_area.x + master_width as i32,
                y: monitor.client_area.y + stack_y as i32,
                width: monitor.client_area.width - master_width,
                height: height as i32
            }
        };

        ShowWindow(client.hwnd, SW_NORMAL);
        SetWindowPos(
            client.hwnd,
            None,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            SET_WINDOW_POS_FLAGS(0)
        )?;

        client.rect = rect.clone();

        let next_y = (is_master as u32) * master_y + (!is_master as u32) * stack_y + rect.height as u32;
        if next_y >= monitor.client_area.height as u32 {
            continue;
        }

        if is_master  {
            master_y += rect.height as u32;
        } else{
            stack_y += rect.height as u32;
        }
    }

    Ok(())
}

unsafe fn focus_stack(increase_index: u32) -> Result<()> {
    Ok(())
}

unsafe fn cleanup(hinstance: &HINSTANCE) -> Result<()> {
    //let mut hwnd = DWMR_APP.hwnd.write().unwrap();
    //DestroyWindow((*hwnd).unwrap())?;
    //*hwnd = None;

    //UnregisterClassW(W_APP_NAME, *hinstance)?;

    Ok(())
}

fn main() -> Result<()> {
    unsafe{
        let hmodule = GetModuleHandleW(None)?;
        let hinstance: HINSTANCE = hmodule.into();
        setup(&hinstance)?;
        scan()?;
        arrange()?;
        cleanup(&hinstance)?; 
    }
    Ok(())
}
