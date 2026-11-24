//! The game process: finding it, reading and writing its memory, allocating code caves.

use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::{
    FlushInstructionCache, ReadProcessMemory, WriteProcessMemory,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, Process32FirstW, Process32NextW,
    MODULEENTRY32W, PROCESSENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, VirtualProtectEx, VirtualQueryEx, MEMORY_BASIC_INFORMATION,
    MEM_COMMIT, MEM_FREE, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION,
    PROCESS_VM_READ, PROCESS_VM_WRITE,
};

pub const GAME_EXE: &str = "NieRAutomata.exe";

/// An open handle to the game, with its main module.
pub struct Process {
    handle: HANDLE,
    pub base: u64,
    pub size: usize,
}

// SAFETY: a process handle can be used from any thread; every call takes `&self`.
unsafe impl Send for Process {}
unsafe impl Sync for Process {}

fn wide_name(raw: &[u16]) -> String {
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..end])
}

/// The process id of the first process with this executable name.
pub fn find_pid(exe: &str) -> Option<u32> {
    // SAFETY: plain Toolhelp snapshot walk; the entry struct is sized before the first call
    // and the snapshot handle is closed at the end.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = None;
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            if wide_name(&entry.szExeFile).eq_ignore_ascii_case(exe) {
                found = Some(entry.th32ProcessID);
                break;
            }
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
        found
    }
}

impl Process {
    /// Opens the process and finds its main module. `None` if the module isn't mapped yet
    /// (the game is still starting) or access is denied.
    pub fn open(pid: u32, exe: &str) -> Option<Self> {
        // SAFETY: OpenProcess/Toolhelp with a sized entry struct; handles are closed on
        // failure (and on Drop for the returned value).
        unsafe {
            let handle = OpenProcess(
                PROCESS_VM_READ
                    | PROCESS_VM_WRITE
                    | PROCESS_VM_OPERATION
                    | PROCESS_QUERY_INFORMATION,
                false,
                pid,
            )
            .ok()?;
            let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid)
            {
                Ok(s) => s,
                Err(_) => {
                    let _ = CloseHandle(handle);
                    return None;
                }
            };
            let mut entry = MODULEENTRY32W {
                dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
                ..Default::default()
            };
            let mut module = None;
            let mut ok = Module32FirstW(snap, &mut entry).is_ok();
            while ok {
                if wide_name(&entry.szModule).eq_ignore_ascii_case(exe) {
                    module = Some((entry.modBaseAddr as u64, entry.modBaseSize as usize));
                    break;
                }
                ok = Module32NextW(snap, &mut entry).is_ok();
            }
            let _ = CloseHandle(snap);
            match module {
                Some((base, size)) => Some(Self { handle, base, size }),
                None => {
                    let _ = CloseHandle(handle);
                    None
                }
            }
        }
    }

    pub fn is_alive(&self) -> bool {
        let mut code = 0u32;
        // SAFETY: valid handle, output is a local.
        unsafe { GetExitCodeProcess(self.handle, &mut code).is_ok() && code == 259 }
    }

    pub fn read_into(&self, addr: u64, buf: &mut [u8]) -> bool {
        if buf.is_empty() {
            return true;
        }
        let mut got = 0usize;
        // SAFETY: the buffer is ours and its length is passed; the remote address may be
        // invalid, which only makes the call fail.
        unsafe {
            ReadProcessMemory(
                self.handle,
                addr as *const c_void,
                buf.as_mut_ptr().cast(),
                buf.len(),
                Some(&mut got),
            )
            .is_ok()
                && got == buf.len()
        }
    }

    pub fn read_vec(&self, addr: u64, len: usize) -> Option<Vec<u8>> {
        let mut v = vec![0u8; len];
        self.read_into(addr, &mut v).then_some(v)
    }

    pub fn read<const N: usize>(&self, addr: u64) -> Option<[u8; N]> {
        let mut b = [0u8; N];
        self.read_into(addr, &mut b).then_some(b)
    }

    pub fn u32(&self, addr: u64) -> Option<u32> {
        self.read::<4>(addr).map(u32::from_le_bytes)
    }
    pub fn i32(&self, addr: u64) -> Option<i32> {
        self.read::<4>(addr).map(i32::from_le_bytes)
    }
    pub fn u64(&self, addr: u64) -> Option<u64> {
        self.read::<8>(addr).map(u64::from_le_bytes)
    }

    /// Writes data (also into code pages: the protection is lifted for the write).
    pub fn write(&self, addr: u64, data: &[u8]) -> bool {
        // SAFETY: the protection change, write and restore all target the same remote range;
        // the source buffer is ours.
        unsafe {
            let mut old = PAGE_PROTECTION_FLAGS(0);
            let protected = VirtualProtectEx(
                self.handle,
                addr as *const c_void,
                data.len(),
                PAGE_EXECUTE_READWRITE,
                &mut old,
            )
            .is_ok();
            let mut done = 0usize;
            let ok = WriteProcessMemory(
                self.handle,
                addr as *const c_void,
                data.as_ptr().cast(),
                data.len(),
                Some(&mut done),
            )
            .is_ok()
                && done == data.len();
            if protected {
                let mut tmp = PAGE_PROTECTION_FLAGS(0);
                let _ = VirtualProtectEx(
                    self.handle,
                    addr as *const c_void,
                    data.len(),
                    old,
                    &mut tmp,
                );
            }
            let _ = FlushInstructionCache(self.handle, Some(addr as *const c_void), data.len());
            ok
        }
    }

    pub fn write_u32(&self, addr: u64, v: u32) -> bool {
        self.write(addr, &v.to_le_bytes())
    }
    pub fn write_u64(&self, addr: u64, v: u64) -> bool {
        self.write(addr, &v.to_le_bytes())
    }
    pub fn write_f32(&self, addr: u64, v: f32) -> bool {
        self.write(addr, &v.to_le_bytes())
    }

    /// Allocates RWX memory anywhere.
    pub fn alloc(&self, size: usize) -> Option<u64> {
        // SAFETY: plain remote allocation.
        let p = unsafe {
            VirtualAllocEx(
                self.handle,
                None,
                size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_EXECUTE_READWRITE,
            )
        };
        (!p.is_null()).then_some(p as u64)
    }

    /// Allocates RWX memory within ±2 GB of `near`, so code there can reach it with
    /// 32-bit relative jumps and RIP-relative operands.
    pub fn alloc_near(&self, near: u64, size: usize) -> Option<u64> {
        const GRANULARITY: u64 = 0x1_0000;
        const REACH: u64 = 0x7FF0_0000;
        let lo = near.saturating_sub(REACH).max(GRANULARITY);
        let hi = near.saturating_add(REACH);
        // Walk the free regions below `near` (closest first), then above it.
        let try_at = |addr: u64| -> Option<u64> {
            // SAFETY: allocation at a specific address; failure just returns null.
            let p = unsafe {
                VirtualAllocEx(
                    self.handle,
                    Some(addr as *const c_void),
                    size,
                    MEM_COMMIT | MEM_RESERVE,
                    PAGE_EXECUTE_READWRITE,
                )
            };
            (!p.is_null()).then_some(p as u64)
        };
        let mut addr = (near & !(GRANULARITY - 1)).saturating_sub(GRANULARITY);
        while addr > lo {
            if let Some(region) = self.query(addr) {
                if region.state == MEM_FREE.0 && region.size as u64 >= size as u64 {
                    let start =
                        (region.base + region.size as u64 - size as u64) & !(GRANULARITY - 1);
                    if start >= region.base && start >= lo {
                        if let Some(p) = try_at(start) {
                            return Some(p);
                        }
                    }
                }
                addr = region
                    .alloc_base
                    .min(region.base)
                    .saturating_sub(GRANULARITY);
            } else {
                addr = addr.saturating_sub(GRANULARITY);
            }
        }
        let mut addr = (near + GRANULARITY) & !(GRANULARITY - 1);
        while addr < hi {
            if let Some(region) = self.query(addr) {
                if region.state == MEM_FREE.0 {
                    let start = (region.base + GRANULARITY - 1) & !(GRANULARITY - 1);
                    if start + size as u64 <= region.base + region.size as u64 {
                        if let Some(p) = try_at(start) {
                            return Some(p);
                        }
                    }
                }
                addr = region.base + region.size as u64;
            } else {
                addr += GRANULARITY;
            }
        }
        None
    }

    pub fn free(&self, addr: u64) {
        // SAFETY: releases a region this program allocated.
        unsafe {
            let _ = VirtualFreeEx(self.handle, addr as *mut c_void, 0, MEM_RELEASE);
        }
    }

    fn query(&self, addr: u64) -> Option<Region> {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        // SAFETY: the output struct is ours and sized.
        let n = unsafe {
            VirtualQueryEx(
                self.handle,
                Some(addr as *const c_void),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        (n != 0).then(|| Region {
            base: mbi.BaseAddress as u64,
            alloc_base: if mbi.AllocationBase.is_null() {
                mbi.BaseAddress as u64
            } else {
                mbi.AllocationBase as u64
            },
            size: mbi.RegionSize,
            state: mbi.State.0,
        })
    }
}

struct Region {
    base: u64,
    alloc_base: u64,
    size: usize,
    state: u32,
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle was opened in `open` and is closed once.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
