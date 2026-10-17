//! Narrow Win32 calls; no elevated permissions or shell interpolation.
#![cfg(target_os = "windows")]
use std::io;

#[repr(C)]
#[derive(Default)]
struct FileTime { low: u32, high: u32 }
#[repr(C)]
struct LastInputInfo { cb_size: u32, dw_time: u32 }
#[link(name = "kernel32")]
extern "system" {
    fn GetSystemTimes(idle: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
    fn GetTickCount64() -> u64;
}
#[link(name = "user32")]
extern "system" {
    fn GetLastInputInfo(info: *mut LastInputInfo) -> i32;
    fn LockWorkStation() -> i32;
}

fn as_u64(t: FileTime) -> u64 { ((t.high as u64) << 32) | t.low as u64 }

pub fn cpu_times() -> Option<(u64,u64,u64)> {
    let (mut idle,mut kernel,mut user) = (FileTime::default(),FileTime::default(),FileTime::default());
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 { return None; }
    Some((as_u64(idle),as_u64(kernel),as_u64(user)))
}

fn safe_idle_delta_ms(now:u32,last:u32)->u32 {
    let elapsed=now.wrapping_sub(last);
    // LASTINPUTINFO may briefly report a tick in the future; do not treat
    // that as 49 days of inactivity and unexpectedly trigger an action.
    if elapsed>i32::MAX as u32 {0}else{elapsed}
}
pub fn idle_seconds() -> Option<f64> {
    let mut info = LastInputInfo { cb_size: std::mem::size_of::<LastInputInfo>() as u32, dw_time: 0 };
    if unsafe { GetLastInputInfo(&mut info) } == 0 { return None; }
    // LASTINPUTINFO.dwTime is a 32-bit uptime counter and can wrap.
    let now = unsafe { GetTickCount64() } as u32;
    Some(safe_idle_delta_ms(now,info.dw_time) as f64 / 1000.0)
}

pub fn power_action(action: &str) -> Result<(), String> {
    match action {
        "lock" => {
            if unsafe { LockWorkStation() } != 0 { Ok(()) }
            else { Err(format!("Не удалось заблокировать компьютер: {}", io::Error::last_os_error())) }
        }
        "shutdown" | "restart" | "logoff" => {
            let args: &[&str] = match action {
                "shutdown" => &["/s", "/t", "0"],
                "restart" => &["/r", "/t", "0"],
                _ => &["/l"]
            };
            let system_root = std::env::var_os("SystemRoot")
                .or_else(|| std::env::var_os("WINDIR"))
                .ok_or("Не удалось определить каталог Windows")?;
            let path = std::path::Path::new(&system_root).join("System32").join("shutdown.exe");
            std::process::Command::new(path).args(args).spawn()
                .map(|_| ()).map_err(|e| format!("Не удалось вызвать Windows: {e}"))
        }
        _ => Err("Неизвестное системное действие".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn idle_tick_wrap_is_safe(){
        assert_eq!(safe_idle_delta_ms(5,u32::MAX-4),10);
        assert_eq!(safe_idle_delta_ms(1,2),0);
        assert_eq!(safe_idle_delta_ms(9000,1000),8000);
    }
}
