use std::process::Command;
use std::path::PathBuf;
use std::process::Stdio;

use tempfile::tempdir;
use tempfile::TempDir;


pub fn run(old_file: &PathBuf, new_file: &PathBuf, verbose: u8) -> anyhow::Result<()> {
    let output_dir = tempdir()?;

    if verbose >= 2 {
	println!("Created new temporary directory `{}` to store compilation output", output_dir.path().to_string_lossy());
    }

    let (old_bc, new_bc) = {
	match create_bitcodes(&output_dir, old_file, new_file, verbose) {
	    Ok((bc, rs)) => {
		(bc, rs)
	    },
	    Err(e) => {
		output_dir.close()?;
		return Err(e);
	    }
	}
    };

    if verbose >= 2 {
	println!("Compiled LLVM bitcode files `{}` and '{}'", old_bc.to_string_lossy(), new_bc.to_string_lossy());
    }
    
    drop(old_bc);
    drop(new_bc);
    output_dir.close()?;
    Ok(())
}

pub fn create_bitcodes(output_dir: &TempDir, old_file: &PathBuf, new_file: &PathBuf, _verbose: u8) -> anyhow::Result<(PathBuf, PathBuf)> {

    let mut old_bc = output_dir.path().to_path_buf();
    old_bc.push("old.bc");
    
    let mut new_bc = output_dir.path().to_path_buf();
    new_bc.push("new.bc");

    for (bc, rs) in [(&old_bc, old_file), (&new_bc, new_file)] {

	let mut rustc_cmd = Command::new("rustc");
	rustc_cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit()); // print warnings and debugs during compilation
	rustc_cmd.args([
	    "--emit=llvm-bc",
	    "--crate-type=lib",
	    "-C",
	    "opt-level=0",
	    "-C",
	    "debuginfo=1",
	    "-o",
	])
	    .arg(bc.as_os_str())
	    .arg(rs.as_os_str());

	let status = rustc_cmd.status()?;

	if !status.success() {
	    return Err(anyhow::Error::msg("Compilation failed"));
	}
    }
    
    Ok((old_bc, new_bc))
}
