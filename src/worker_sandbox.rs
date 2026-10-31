//! AppContainer + Job Object launcher for read-only document workers.
#![cfg(windows)]

use std::ffi::c_void;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows_sys::Win32::Security::{PSID, SECURITY_CAPABILITIES};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, STARTUPINFOEXW,
};

const PROFILE_NAME: &str = "NotOffice.Worker";
const PROCESS_MEMORY_LIMIT: usize = 512 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 128 * 1024 * 1024;

pub fn run(path: &Path, flag: &str) -> Result<Vec<u8>, String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "notoffice-worker-sandbox-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let worker = directory.join("worker.exe");
    let current = std::env::current_exe().map_err(|error| error.to_string())?;
    std::fs::copy(&current, &worker).map_err(|error| error.to_string())?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("bin");
    let input = directory.join(format!("input.{extension}"));
    std::fs::copy(path, &input).map_err(|error| error.to_string())?;
    let output = directory.join("output.json");
    let sid = sid_string()?;
    let grant = format!("*{sid}:(OI)(CI)M");
    let status = Command::new("icacls")
        .arg(&directory)
        .arg("/grant")
        .arg(&grant)
        .creation_flags(0x08000000)
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&directory);
        return Err("AppContainer ACL grant failed".into());
    }
    let result = run_process(
        &directory,
        &worker,
        &input,
        &output,
        flag,
        std::time::Duration::from_secs(30),
    );
    let bytes = std::fs::read(&output).unwrap_or_default();
    let metadata = std::fs::symlink_metadata(&output).ok();
    let valid = metadata
        .as_ref()
        .is_some_and(|value| value.file_type().is_file() && value.len() <= OUTPUT_LIMIT as u64);
    let _ = std::fs::remove_dir_all(&directory);
    result?;
    if !valid || bytes.len() > OUTPUT_LIMIT {
        return Err("Worker returned an invalid or oversized result".into());
    }
    Ok(bytes)
}

#[allow(clippy::too_many_arguments)]
fn run_process(
    directory: &Path,
    worker: &Path,
    input: &Path,
    output: &Path,
    flag: &str,
    timeout: std::time::Duration,
) -> Result<(), String> {
    let command = format!(
        "{} {} {} {}",
        quote(worker.to_string_lossy().as_ref()),
        quote(flag),
        quote(input.to_string_lossy().as_ref()),
        quote(output.to_string_lossy().as_ref())
    );
    let mut command_w = wide(&command);
    let worker_w = wide(worker.to_string_lossy().as_ref());
    let current_w = wide(directory.to_string_lossy().as_ref());
    let sid = derive_sid()?;
    let mut attribute_size = 0usize;
    unsafe {
        InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut attribute_size);
    }
    if attribute_size == 0 {
        return Err("AppContainer attribute list is unavailable".into());
    }
    let mut attribute = vec![0u8; attribute_size];
    let attribute_ptr = attribute.as_mut_ptr() as *mut c_void;
    if unsafe { InitializeProcThreadAttributeList(attribute_ptr, 1, 0, &mut attribute_size) } == 0 {
        return Err("AppContainer attribute list failed".into());
    }
    let capabilities = SECURITY_CAPABILITIES {
        AppContainerSid: sid,
        Capabilities: std::ptr::null_mut(),
        CapabilityCount: 0,
        Reserved: 0,
    };
    if unsafe {
        UpdateProcThreadAttribute(
            attribute_ptr,
            0,
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
            &capabilities as *const _ as *const c_void,
            std::mem::size_of::<SECURITY_CAPABILITIES>(),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    } == 0
    {
        unsafe { DeleteProcThreadAttributeList(attribute_ptr) };
        return Err("AppContainer security capability failed".into());
    }
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.lpAttributeList = attribute_ptr;
    let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessW(
            worker_w.as_ptr(),
            command_w.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT,
            std::ptr::null(),
            current_w.as_ptr(),
            &startup as *const STARTUPINFOEXW as *const _,
            &mut process,
        )
    };
    unsafe {
        DeleteProcThreadAttributeList(attribute_ptr);
        LocalFree(sid);
    }
    if created == 0 {
        return Err(format!(
            "AppContainer process creation failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if !job.is_null() {
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        info.BasicLimitInformation.ActiveProcessLimit = 1;
        info.ProcessMemoryLimit = PROCESS_MEMORY_LIMIT;
        unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            AssignProcessToJobObject(job, process.hProcess);
        }
    }
    let wait = unsafe {
        WaitForSingleObject(
            process.hProcess,
            timeout.as_millis().min(u32::MAX as u128) as u32,
        )
    };
    if wait == WAIT_TIMEOUT {
        unsafe {
            TerminateProcess(process.hProcess, 1);
            WaitForSingleObject(process.hProcess, 5_000);
        }
    }
    let mut exit = 1u32;
    unsafe {
        GetExitCodeProcess(process.hProcess, &mut exit);
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
        if !job.is_null() {
            CloseHandle(job);
        }
    }
    if wait == WAIT_TIMEOUT {
        return Err("Worker timed out".into());
    }
    if wait != WAIT_OBJECT_0 || exit != 0 {
        return Err(format!("Worker process failed (exit {exit})"));
    }
    let _ = command;
    Ok(())
}

fn quote(value: &str) -> String {
    let mut result = String::from("\"");
    for character in value.chars() {
        if character == '"' {
            result.push('\\');
        }
        result.push(character);
    }
    result.push('"');
    result
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn sid_string() -> Result<String, String> {
    let name = wide(PROFILE_NAME);
    let mut sid: PSID = std::ptr::null_mut();
    let created = unsafe {
        CreateAppContainerProfile(
            name.as_ptr(),
            name.as_ptr(),
            name.as_ptr(),
            std::ptr::null(),
            0,
            &mut sid,
        )
    };
    if created < 0 {
        let derived = unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) };
        if derived < 0 {
            return Err("AppContainer profile is unavailable".into());
        }
    }
    let mut text: *mut u16 = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        unsafe { LocalFree(sid) };
        return Err("AppContainer SID conversion failed".into());
    }
    let value =
        unsafe { std::ffi::OsString::from_wide(std::slice::from_raw_parts(text, length(text))) }
            .to_string_lossy()
            .into_owned();
    unsafe {
        LocalFree(text as *mut c_void);
        LocalFree(sid);
    }
    Ok(value)
}

fn derive_sid() -> Result<PSID, String> {
    let name = wide(PROFILE_NAME);
    let mut sid: PSID = std::ptr::null_mut();
    if unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) } < 0 {
        return Err("AppContainer SID is unavailable".into());
    }
    Ok(sid)
}

fn length(value: *const u16) -> usize {
    let mut length = 0;
    unsafe {
        while *value.add(length) != 0 {
            length += 1;
        }
    }
    length
}
