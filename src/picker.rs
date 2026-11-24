//! The Windows "open file" dialog, for picking a background picture.

use std::path::PathBuf;

#[cfg(windows)]
pub fn pick_image() -> Option<PathBuf> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, SIGDN_FILESYSPATH,
    };
    // SAFETY: COM on this thread (initialized here); the filter strings live until the
    // dialog closes and the returned path is freed with CoTaskMemFree.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog
            .SetOptions(options | FOS_FORCEFILESYSTEM | FOS_FILEMUSTEXIST)
            .ok()?;
        let name = HSTRING::from(tr!("Imagens", "Pictures"));
        let spec = HSTRING::from("*.png;*.jpg;*.jpeg;*.webp");
        let filters = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(name.as_ptr()),
            pszSpec: PCWSTR(spec.as_ptr()),
        }];
        let _ = dialog.SetFileTypes(&filters);
        let _ = dialog.SetTitle(&HSTRING::from(tr!(
            "Escolha uma imagem de fundo",
            "Choose a background picture"
        )));
        dialog.Show(None).ok()?;
        let path = dialog
            .GetResult()
            .ok()?
            .GetDisplayName(SIGDN_FILESYSPATH)
            .ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const std::ffi::c_void));
        text.map(PathBuf::from)
    }
}

#[cfg(not(windows))]
pub fn pick_image() -> Option<PathBuf> {
    None
}
