//! Native dynamic-library support from `src/nvim/os/dl.c`.

type LibraryHandle = *mut std::ffi::c_void;

#[allow(dead_code)]
struct DynamicLibrary {
    handle: LibraryHandle,
}

#[allow(dead_code)]
impl DynamicLibrary {
    fn open(name: &[u8]) -> Option<Self> {
        let name = std::ffi::CString::new(name).ok()?;

        #[cfg(unix)]
        let handle =
            unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_LAZY) };

        #[cfg(windows)]
        let handle = {
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn LoadLibraryA(name: *const u8) -> LibraryHandle;
            }
            unsafe { LoadLibraryA(name.as_ptr().cast()) }
        };

        #[cfg(not(any(unix, windows)))]
        let handle = std::ptr::null_mut();

        (!handle.is_null()).then_some(Self { handle })
    }

    unsafe fn symbol(
        &self,
        name: &[u8],
    ) -> Option<*mut std::ffi::c_void> {
        let name = std::ffi::CString::new(name).ok()?;

        #[cfg(unix)]
        let symbol = unsafe { libc::dlsym(self.handle, name.as_ptr()) };

        #[cfg(windows)]
        let symbol = {
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn GetProcAddress(
                    module: LibraryHandle,
                    name: *const u8,
                ) -> *mut std::ffi::c_void;
            }
            unsafe { GetProcAddress(self.handle, name.as_ptr().cast()) }
        };

        #[cfg(not(any(unix, windows)))]
        let symbol = std::ptr::null_mut();

        (!symbol.is_null()).then_some(symbol)
    }
}

impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::dlclose(self.handle);
        }

        #[cfg(windows)]
        {
            #[link(name = "kernel32")]
            unsafe extern "system" {
                #[allow(dead_code)]
                fn FreeLibrary(module: LibraryHandle) -> i32;
            }
            unsafe {
                FreeLibrary(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c_runtime_name() -> &'static [u8] {
        #[cfg(windows)]
        {
            b"ucrtbase.dll"
        }
        #[cfg(target_os = "macos")]
        {
            b"/usr/lib/libSystem.B.dylib"
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            b"libc.so.6"
        }
    }

    #[test]
    fn dynamic_library_rejects_interior_nul_names() {
        assert!(DynamicLibrary::open(b"bad\0name").is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot load native libraries")]
    fn dynamic_library_loads_the_c_runtime_and_abs_symbol() {
        let library =
            DynamicLibrary::open(c_runtime_name()).expect("C runtime");
        assert!(unsafe { library.symbol(b"abs") }.is_some());
        assert!(unsafe { library.symbol(b"nero_missing_symbol") }.is_none());
    }
}
