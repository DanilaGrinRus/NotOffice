//! AppContainer + Job Object launcher for the native PDF worker.
#![cfg(windows)]

use std::ffi::c_void;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
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

const PROFILE_NAME: &str = "NotOffice.PdfWorker";
const PROCESS_MEMORY_LIMIT: usize = 256 * 1024 * 1024;
const PAGE_LIMIT: usize = 32 * 1024 * 1024;

pub struct Sandbox {
    directory: PathBuf,
    worker: PathBuf,
    input: PathBuf,
}

impl Sandbox {
    pub fn new(input_bytes: &[u8]) -> Result<Self, String> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "notoffice-pdf-sandbox-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let worker = directory.join("worker.exe");
        let current = std::env::current_exe().map_err(|error| error.to_string())?;
        std::fs::copy(&current, &worker).map_err(|error| error.to_string())?;
        let input = directory.join("input.pdf");
        std::fs::write(&input, input_bytes).map_err(|error| error.to_string())?;
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
        Ok(Self {
            directory,
            worker,
            input,
        })
    }

    pub fn probe(&self) -> Result<Vec<[f32; 2]>, String> {
        let output = self.directory.join("pages.txt");
        let _ = std::fs::remove_file(&output);
        self.run(
            &[
                "--pdf-probe-worker".into(),
                self.input.to_string_lossy().into_owned(),
                output.to_string_lossy().into_owned(),
            ],
            std::time::Duration::from_secs(15),
        )?;
        let metadata = std::fs::symlink_metadata(&output).map_err(|error| error.to_string())?;
        if !metadata.file_type().is_file() || metadata.len() > 1024 * 1024 {
            return Err("PDF worker page manifest is invalid or oversized".into());
        }
        let data = std::fs::read_to_string(&output).map_err(|error| error.to_string())?;
        let mut pages = Vec::new();
        for line in data.lines() {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() != 2 {
                return Err("Invalid PDF worker page data".into());
            }
            let width = fields[0]
                .parse::<f32>()
                .map_err(|error| error.to_string())?;
            let height = fields[1]
                .parse::<f32>()
                .map_err(|error| error.to_string())?;
            pages.push([width, height]);
        }
        if pages.is_empty() || pages.len() > crate::pdf::MAX_PAGES {
            return Err("PDF worker returned an invalid page count".into());
        }
        Ok(pages)
    }

    pub fn render(&self, index: usize) -> Result<Vec<u8>, String> {
        let output = self.directory.join(format!("page-{index}.png"));
        let _ = std::fs::remove_file(&output);
        self.run(
            &[
                "--pdf-render-worker".into(),
                self.input.to_string_lossy().into_owned(),
                index.to_string(),
                output.to_string_lossy().into_owned(),
            ],
            std::time::Duration::from_secs(30),
        )?;
        let metadata = std::fs::symlink_metadata(&output).map_err(|error| error.to_string())?;
        if !metadata.file_type().is_file() || metadata.len() > PAGE_LIMIT as u64 {
            return Err("PDF page raster is invalid or oversized".into());
        }
        let bytes = std::fs::read(&output).map_err(|error| error.to_string())?;
        if bytes.len() > PAGE_LIMIT {
            return Err("PDF page raster exceeds 32 MiB".into());
        }
        Ok(bytes)
    }

    fn run(&self, args: &[String], timeout: std::time::Duration) -> Result<(), String> {
        let mut command = args
            .iter()
            .map(|arg| quote_arg(arg))
            .collect::<Vec<_>>()
            .join(" ");
        command = format!("{} {command}", quote_arg(&self.worker.to_string_lossy()));
        let mut command_w = wide(&command);
        let worker_w = wide(&self.worker.to_string_lossy());
        let current_w = wide(&self.directory.to_string_lossy());
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
        if unsafe { InitializeProcThreadAttributeList(attribute_ptr, 1, 0, &mut attribute_size) }
            == 0
        {
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
            return Err("PDF worker timed out".into());
        }
        if wait != WAIT_OBJECT_0 || exit != 0 {
            return Err(format!("PDF worker process failed (exit {exit})"));
        }
        Ok(())
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn quote_arg(value: &str) -> String {
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
    let converted = unsafe { ConvertSidToStringSidW(sid, &mut text) };
    if converted == 0 {
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
    let derived = unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) };
    if derived < 0 {
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
