// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::anyhow,
    std::path::{Path, PathBuf},
    trycmd::{schema::TryCmd, Error},
};

/// List of coreutils binaries to materialize in trycmd test environments.
const COREUTILS_BINARIES: [&str; 9] = [
    "cat", "cp", "hashsum", "ln", "ls", "mkdir", "sort", "test", "touch",
];

/// Ensures Rust coreutils multicall binary is available.
///
/// Essentially runs `cargo install coreutils` into the `target/coreutils` directory
/// for the current Cargo workspace project.
fn ensure_coreutils_multicall() -> anyhow::Result<PathBuf> {
    let current_exe = std::env::current_exe()?;

    let target_dir = current_exe
        .parent()
        .ok_or_else(|| anyhow!("unable to determine current exe parent"))?
        .parent()
        .ok_or_else(|| anyhow!("unable to determine parent directory of current exe directory"))?
        .parent()
        .ok_or_else(|| anyhow!("unable to parent grandparent of current exe directory"))?;

    let coreutils_dir = target_dir.join("coreutils");

    let coreutils_bin_dir = coreutils_dir.join("bin");

    let mut multicall_bin = coreutils_bin_dir.join("coreutils");

    if cfg!(windows) {
        multicall_bin.set_extension("exe");
    }

    if multicall_bin.exists() {
        return Ok(multicall_bin);
    }

    let cargo_bin = std::env::var_os("CARGO")
        .ok_or_else(|| anyhow!("unable to resolve CARGO environment variable"))?;

    eprintln!("installing Rust coreutils tp {}", coreutils_dir.display());
    let output = std::process::Command::new(cargo_bin)
        .args(vec![
            "install".to_string(),
            "--root".to_string(),
            coreutils_dir.display().to_string(),
            "--version".to_string(),
            "0.0.18".to_string(),
            "coreutils".to_string(),
        ])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "error installing coreutils: stdout: {}; stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(multicall_bin)
}

#[cfg(unix)]
fn install_coreutils_bin(multicall_bin: &Path, bin: &Path) -> Result<(), std::io::Error> {
    std::os::unix::fs::symlink(multicall_bin, bin)
}

#[cfg(windows)]
fn install_coreutils_bin(multicall_bin: &Path, bin: &Path) -> Result<(), std::io::Error> {
    std::fs::copy(multicall_bin, bin).map(|_| ())
}

/// Custom loader for .trycmd files.
fn load_trycmd(path: &Path) -> Result<TryCmd, Error> {
    let mut cmd = TryCmd::load_trycmd(path)?;

    // CWD should be the crate root.
    let cwd = std::env::current_dir().map_err(Error::new)?;

    // We set the test to execute from a sandboxed copy of the crate root.
    // This allows tests to create their own files without disturbing the
    // source checkout.
    cmd.fs.base = Some(cwd.clone());
    cmd.fs.cwd = Some(cwd.clone());
    cmd.fs.sandbox = Some(true);

    Ok(cmd)
}

#[test]
fn cli_tests() {
    let coreutils_multicall = ensure_coreutils_multicall().unwrap();
    let coreutils_bin = coreutils_multicall.parent().unwrap();

    let cases = trycmd::TestCases::new();

    for bin in COREUTILS_BINARIES {
        let mut bin_path = coreutils_bin.join(bin);
        if cfg!(windows) {
            bin_path.set_extension("exe");
        }

        if !bin_path.exists() {
            install_coreutils_bin(&coreutils_multicall, &bin_path).unwrap();
        }

        cases.register_bin(bin, bin_path);
    }

    cases.file_extension_loader("trycmd", load_trycmd);

    cases.case("tests/cmd/*.trycmd").case("tests/cmd/*.toml");

    // Help output breaks without notarize feature.
    if cfg!(not(feature = "notarize")) {
        cases.skip("tests/cmd/encode-app-store-connect-api-key.trycmd");
        cases.skip("tests/cmd/help.trycmd");
        cases.skip("tests/cmd/notary*.trycmd");
    }

    // Tests with `ln -s` may not work on Windows. So just skip them.
    if cfg!(windows) {
        cases.skip("tests/cmd/sign-bundle-framework.trycmd");
        cases.skip("tests/cmd/sign-bundle-with-nested-framework.trycmd");
    }
}
