use std::process::Command;
use std::path::PathBuf;
use std::process::Stdio;

use rustc_demangle::try_demangle;

use tempfile::tempdir;
use tempfile::TempDir;

use haybale::Project;
use haybale::Config;

pub fn run(old_file: &PathBuf, new_file: &PathBuf, verbose: u8, complexity: usize) -> anyhow::Result<()> {
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
    
    let (old_project, new_project, functions_to_analyze) = get_fns_from_bc_files(old_bc, new_bc, verbose)?;

    for (oldfn_name, newfn_name) in functions_to_analyze {
	check_preconditions(&old_project, &new_project, &oldfn_name, &newfn_name, verbose)?;
	analyze_fn(&old_project, &new_project, &oldfn_name, &newfn_name, verbose)?;
    }
    
    output_dir.close()?;
    Ok(())
}

fn check_preconditions(old_project: &Project, new_project: &Project, oldfn_name: &str, newfn_name: &str, verbose: u8) -> anyhow::Result<()> {
    if verbose >= 2 {
	println!("Checking preconditions of '{}'", oldfn_name);
    }

    let oldfn = old_project.get_func_by_name(oldfn_name).unwrap().0;
    let newfn = new_project.get_func_by_name(newfn_name).unwrap().0;

    if oldfn.parameters.len() != newfn.parameters.len() {
	return Err(anyhow::Error::msg(format!("Cannot compare: Parameter lengths differ for function {}", oldfn_name)));
    }

    for (old, new) in std::iter::zip(oldfn.parameters.iter(), newfn.parameters.iter()) {
	if old != new {
	    return Err(anyhow::Error::msg(format!("Cannot compare: Parameter types significantly differ for function {}", oldfn_name)));
	}
    }

    if oldfn.return_type.as_ref() == &llvm_ir::Type::VoidType {
	return Err(anyhow::Error::msg(format!("Cannot compare: Function {} returns void (vacuous evaluation)", oldfn_name)));
    }
    if newfn.return_type.as_ref() == &llvm_ir::Type::VoidType {
	return Err(anyhow::Error::msg(format!("Cannot compare: Function {} returns void (vacuous evaluation)", newfn_name)));
    }

    Ok(())
}

fn analyze_fn(old_project: &Project, new_project: &Project, oldfn_name: &str, newfn_name: &str, verbose: u8) -> anyhow::Result<()> {
    if verbose >= 1 {
	println!("Performing symbolic execution on  `{}` and '{}'", oldfn_name, newfn_name);
    }
    todo!("");
    // let mut em: ExecutionManager<DefaultBackend> =
    //     symex_function(funcname, project, Config::Default(), params).unwrap();

    // let returnwidth = match em.func().return_type.as_ref() {
    //     Type::VoidType => {
    //         return Err("find_zero_of_func: function has void type".into());
    //     },
    //     ty => {
    //         let width = project
    //             .size_in_bits(&ty)
    //             .expect("Function return type shouldn't be an opaque struct type");
    //         assert_ne!(width, 0, "Function return type has width 0 bits but isn't void type"); // void type was handled above
    //         width
    //     },
    // };
    // let zero = em.state().zero(returnwidth);
    // let mut found = false;
    // while let Some(bvretval) = em.next() {
    //     match bvretval {
    //         Ok(ReturnValue::ReturnVoid) => panic!("Function shouldn't return void"),
    //         Ok(ReturnValue::Throw(_)) => continue, // we're looking for values that result in _returning_ zero, not _throwing_ zero
    //         Ok(ReturnValue::Abort) => continue,
    //         Ok(ReturnValue::Return(bvretval)) => {
    //             let state = em.mut_state();
    //             bvretval._eq(&zero).assert();
    //             if state.sat()? {
    //                 found = true;
    //                 break;
    //             }
    //         },
    //         Err(Error::LoopBoundExceeded(_)) => continue, // ignore paths that exceed the loop bound, keep looking
    //         Err(e) => return Err(em.state().full_error_message_with_context(e)),
    //     }
    // }

    // let param_bvs: Vec<_> = em.param_bvs().clone();
    // let func = em.func();
    // let state = em.mut_state();
    // if found {
    //     // in this case state.sat() must have passed
    //     Ok(Some(
    //         func.parameters
    //             .iter()
    //             .zip_eq(param_bvs.iter())
    //             .map(|(p, bv)| {
    //                 let param_as_u64 = state
    //                     .get_a_solution_for_bv(bv)?
    //                     .expect("since state.sat() passed, expected a solution for each var")
    //                     .as_u64()
    //                     .expect("parameter more than 64 bits wide");
    //                 Ok(match p.ty.as_ref() {
    //                     Type::IntegerType { bits: 8 } => SolutionValue::I8(param_as_u64 as i8),
    //                     Type::IntegerType { bits: 16 } => SolutionValue::I16(param_as_u64 as i16),
    //                     Type::IntegerType { bits: 32 } => SolutionValue::I32(param_as_u64 as i32),
    //                     Type::IntegerType { bits: 64 } => SolutionValue::I64(param_as_u64 as i64),
    //                     Type::PointerType { .. } => SolutionValue::Ptr(param_as_u64),
    //                     ty => unimplemented!("Function parameter with type {:?}", ty),
    //                 })
    //             })
    //             .collect::<Result<_>>()?,
    //     ))
    // } else {
    //     Ok(None)
    // }
}

fn get_fns_from_bc_files(old_bc: PathBuf, new_bc: PathBuf, verbose: u8) -> anyhow::Result<(Project, Project, Vec<(String, String)>)> {
    let old_project = Project::from_bc_path(old_bc)
	.map_err(|e| anyhow::Error::msg(format!("Failed to parse old module: {}", e)))?;
    let new_project = Project::from_bc_path(new_bc)
	.map_err(|e| anyhow::Error::msg(format!("Failed to new old module: {}", e)))?;

    let fns = new_project.all_functions().filter_map(|(func, _)| {
	let demangled = match try_demangle(&func.name) {
	    Ok(d) => d,
	    Err(_) => return None, // This is rust, so every readable function is already demangled
	};
	// TODO C++ support with try_demangle
	let modstr = format!("{:#}", demangled); // string value, without trailing hash value
	let modpath: syn::Path = match syn::parse_str(&modstr) {
	    Ok(m) => m,
	    Err(_) => return None, // if this can't be parsed, we don't care about it
	};

	if modpath.leading_colon.is_some() {
	    return None; // not a pub fn
	}

	if modpath.segments.first().map(|s| format!("{}", s.ident)) != Some("new".to_owned()) {
	    return None;
	}

	// Now that we know this is a function worth analyzing, cross-reference against the old implementation.
	// Need to make sure that it exists there too

	// We know exactly what to expect here, so a simple replace is just fine
	let oldfn_name = "old".to_owned() + &modstr[3..]; // 3 = strlen("new")

	if old_project.get_func_by_name(&oldfn_name).is_none() {
	    if verbose >= 1 {
		println!("Function `{}` exists in new implementation, but not old implementation. Ignoring.", oldfn_name);
	    }
	    return None;
	}
	
	Some((oldfn_name, modstr))
    }).collect();

    return Ok((old_project, new_project, fns));
}

fn create_bitcodes(output_dir: &TempDir, old_file: &PathBuf, new_file: &PathBuf, _verbose: u8) -> anyhow::Result<(PathBuf, PathBuf)> {

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
