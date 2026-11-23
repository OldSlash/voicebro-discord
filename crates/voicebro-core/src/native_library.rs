//! Load a trusted native library without searching for it by name.

use libloading::Library;
use std::io;
use std::path::Path;

/// Loads the library at an explicit path, with bundled DLL lookup on Windows.
///
/// # Safety
/// The caller must trust the library and its dependencies, including their initializers
/// and termination routines, and verify the ABI of any symbols it uses.
pub(crate) unsafe fn load(path: &Path) -> io::Result<Library> {
    let path = std::path::absolute(path)?;
    #[cfg(windows)]
    {
        use libloading::os::windows::{Library as WindowsLibrary, LOAD_WITH_ALTERED_SEARCH_PATH};

        // SAFETY: the caller trusts the library; the flag requires an absolute path.
        unsafe { WindowsLibrary::load_with_flags(&path, LOAD_WITH_ALTERED_SEARCH_PATH) }
            .map(Library::from)
            .map_err(io::Error::other)
    }
    #[cfg(not(windows))]
    {
        // SAFETY: the caller trusts the library; the absolute path bypasses name lookup.
        unsafe { Library::new(&path) }.map_err(io::Error::other)
    }
}
