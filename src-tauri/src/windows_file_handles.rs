//! A handle-relative child keeps a checked NTFS directory nonempty while writing.
//! Paths are never followed to install the guard. Dropping its handle deletes it.
use crate::error::{AppError, AppResult};
use std::{
    ffi::c_void,
    fs::File,
    os::windows::io::{AsRawHandle, FromRawHandle},
    ptr,
};
#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}
#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: *mut c_void,
    object_name: *mut UnicodeString,
    attributes: u32,
    security_descriptor: *mut c_void,
    security_quality_of_service: *mut c_void,
}
#[repr(C)]
struct IoStatusBlock {
    status: usize,
    information: usize,
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtCreateFile(
        handle: *mut *mut c_void,
        access: u32,
        attributes: *mut ObjectAttributes,
        status: *mut IoStatusBlock,
        allocation: *mut i64,
        file_attributes: u32,
        share: u32,
        disposition: u32,
        options: u32,
        ea: *mut c_void,
        ea_length: u32,
    ) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
}

pub fn create_guard(directory: &File) -> AppResult<File> {
    child(
        directory,
        &format!(".azd-gui-write-{}.lock", uuid::Uuid::new_v4()),
        false,
        true,
    )
}
pub fn open_existing(directory: &File, name: &str, is_directory: bool) -> AppResult<File> {
    child(directory, name, is_directory, false)
}

fn child(directory: &File, name: &str, is_directory: bool, guard: bool) -> AppResult<File> {
    if name.is_empty()
        || name.len() > 1024
        || matches!(name, "." | "..")
        || name.contains(['/', '\\', ':'])
        || name.chars().any(char::is_control)
    {
        return Err(AppError::Invalid("Unsafe native child filename.".into()));
    }
    let mut name: Vec<u16> = name.encode_utf16().collect();
    let mut unicode = UnicodeString {
        length: (name.len() * 2) as u16,
        maximum_length: (name.len() * 2) as u16,
        buffer: name.as_mut_ptr(),
    };
    let mut attributes = ObjectAttributes {
        length: std::mem::size_of::<ObjectAttributes>() as u32,
        root_directory: directory.as_raw_handle(),
        object_name: &mut unicode,
        attributes: 0x40 | 0x1000,
        security_descriptor: ptr::null_mut(),
        security_quality_of_service: ptr::null_mut(),
    };
    let mut status = IoStatusBlock {
        status: 0,
        information: 0,
    };
    let mut handle = ptr::null_mut();
    let access = 0x80000000 | 0x100000 | if guard { 0x40000000 | 0x10000 } else { 0 };
    let options =
        0x20 | 0x200000 | if is_directory { 1 } else { 0x40 } | if guard { 0x1000 } else { 0 };
    // SAFETY: live buffers and synchronous I/O. Every lookup is one immediate
    // handle-relative child; OBJ_DONT_REPARSE forbids following reparse metadata.
    let result = unsafe {
        NtCreateFile(
            &mut handle,
            access,
            &mut attributes,
            &mut status,
            ptr::null_mut(),
            if guard { 0x100 } else { 0 },
            if guard { 0 } else { 1 },
            if guard { 2 } else { 1 },
            options,
            ptr::null_mut(),
            0,
        )
    };
    if result < 0 {
        return Err(AppError::Io(std::io::Error::from_raw_os_error(unsafe {
            RtlNtStatusToDosError(result)
        }
            as i32)));
    }
    // SAFETY: NtCreateFile succeeded and returned one exclusively owned handle.
    let file = unsafe { File::from_raw_handle(handle) };
    use std::os::windows::fs::MetadataExt;
    let metadata = file.metadata()?;
    if metadata.file_attributes() & 0x400 != 0
        || if is_directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return Err(AppError::Invalid("Unsafe native child handle.".into()));
    }
    Ok(file)
}
