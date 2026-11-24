//! Puts the icon, the version info and a manifest inside the Windows executable.
//!
//! The manifest asks for no elevation: the game runs as the normal user, so the trainer can
//! open it without administrator rights.

/// Background pictures embedded in the executable when present (otherwise the app paints its
/// procedural scenery). Generates `$OUT_DIR/backgrounds.rs`.
fn backgrounds() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("backgrounds.rs");
    let mut code = String::new();
    println!("cargo:rerun-if-changed=assets");
    for (name, stem) in [("NIGHT", "assets/night"), ("RUINS", "assets/ruins")] {
        let found = ["webp", "jpg", "jpeg", "png"]
            .iter()
            .map(|ext| std::path::PathBuf::from(format!("{stem}.{ext}")))
            .find(|p| p.exists());
        match found {
            Some(path) => {
                println!("cargo:rerun-if-changed={}", path.display());
                let abs = path.canonicalize().unwrap();
                let abs = abs.to_string_lossy().replace('\\', "/").replace("//?/", "");
                code += &format!("pub const {name}: &[u8] = include_bytes!(\"{abs}\");\n");
            }
            None => code += &format!("pub const {name}: &[u8] = &[];\n"),
        }
    }
    std::fs::write(out, code).unwrap();
}

fn main() {
    backgrounds();
    println!("cargo:rerun-if-changed=assets/yorha-trainer.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        if std::path::Path::new("assets/yorha-trainer.ico").exists() {
            res.set_icon("assets/yorha-trainer.ico");
        }
        res.set("ProductName", "YoRHa Trainer")
            .set("FileDescription", "YoRHa Trainer")
            .set("CompanyName", "Oz Vessalius (@Silhatote)")
            .set(
                "LegalCopyright",
                "© 2026 Oz Vessalius (@Silhatote). MIT license.",
            )
            .set_manifest(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#,
            );
        if let Err(e) = res.compile() {
            println!("cargo:warning=no icon or manifest in the executable: {e}");
        }
    }
}
