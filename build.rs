use std::{
    error::Error,
    env::var,
    path::{Path, PathBuf},
};

/// Returns the path to the `Windows Kits` directory. It's by default at
/// `C:\Program Files (x86)\Windows Kits\10`.
#[cfg(windows)]
fn get_windows_kits_dir() -> Result<PathBuf, Box<dyn Error>> {
    use winreg::{enums::*, RegKey};

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = r"SOFTWARE\Microsoft\Windows Kits\Installed Roots";
    let dir: String = hklm.open_subkey(key)?.get_value("KitsRoot10")?;

    Ok(dir.into())
}

/// Returns the path to the `Windows Kits` directory, when cross-compiling. The
/// user is expected to pass its location through an environment variable.
#[cfg(not(windows))]
fn get_windows_kits_dir() -> Result<PathBuf, Box<dyn Error>> {
    let path = std::env::var_os("WINDOWS_KIT_DIR")
        .ok_or("When cross-compiling, please provide windows kit local path \
                through the WINDOWS_KIT_DIR environment variable.")?;
    Ok(PathBuf::from(path))
}

/// Returns the path to the kernel mode libraries. The path may look like this:
/// `C:\Program Files (x86)\Windows Kits\10\Lib\10.0.18362.0\km`.
fn get_km_dir(windows_kits_dir: &PathBuf) -> Result<PathBuf, Box<dyn Error>> {
    let readdir = Path::new(windows_kits_dir).join("Lib").read_dir()?;

    let max_libdir = readdir
        .filter_map(|dir| dir.ok())
        .map(|dir| dir.path())
        .filter(|dir| {
            dir.components()
                .last()
                .and_then(|c| c.as_os_str().to_str())
                .map(|c| c.starts_with("10.") && dir.join("km").is_dir())
                .unwrap_or(false)
        })
        .max()
        .ok_or_else(|| format!("Can not find a valid km dir in `{:?}`", windows_kits_dir))?;

    Ok(max_libdir.join("km"))
}

fn internal_link_search() {
    let windows_kits_dir = get_windows_kits_dir().unwrap();
    let km_dir = get_km_dir(&windows_kits_dir).unwrap();
    let target = var("TARGET").unwrap();

    let arch = if target.contains("x86_64") {
        "x64"
    } else if target.contains("i686") {
        "x86"
    } else {
        panic!("Only support x86_64 and i686!");
    };

    let lib_dir = km_dir.join(arch);
    println!("cargo:rustc-link-search=native={}", lib_dir.to_str().unwrap());
}

fn main() {
    internal_link_search()
}
