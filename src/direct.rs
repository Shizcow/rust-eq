use std::process::Command;
use std::path::PathBuf;
use std::process::Stdio;
use std::collections::HashSet;

use rustc_demangle::try_demangle;

use tempfile::tempdir;
use tempfile::TempDir;

use haybale::Project;
use haybale::Config;

use haybale::symex_function;
use haybale::backend::DefaultBackend;
use haybale::backend::Backend;
use haybale::ExecutionManager;
use haybale::ReturnValue;
use haybale::Error;
use haybale::State;
use haybale::solver_utils::PossibleSolutions::*;

use boolector::BVSolution;

use itertools::Itertools;

use llvm_ir::Type;

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
	analyze_fn(&old_project, &new_project, &oldfn_name, &newfn_name, verbose, complexity)?;
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

    if oldfn.return_type.as_ref() == &Type::VoidType {
	return Err(anyhow::Error::msg(format!("Cannot compare: Function {} returns void (vacuous evaluation)", oldfn_name)));
    }
    if newfn.return_type.as_ref() == &Type::VoidType {
	return Err(anyhow::Error::msg(format!("Cannot compare: Function {} returns void (vacuous evaluation)", newfn_name)));
    }

    Ok(())
}

fn check_rval(project: &Project, em: &ExecutionManager<DefaultBackend>) -> anyhow::Result<u32> {
    match em.func().return_type.as_ref() {
        Type::VoidType => {
            Err(anyhow::Error::msg("Cannot compare: Function has void type")) // checked earlier
        },
        ty => {
            let width = project
                .size_in_bits(&ty)
                .expect("Function return type shouldn't be an opaque struct type");
	    if width == 0 {
		return Err(anyhow::Error::msg("Cannot compare: Function return type has width 0 bits but isn't void type")); // void type was handled above
	    }
	    Ok(width)
        },
    }
}

fn get_rvals(em: &mut ExecutionManager<DefaultBackend>) -> anyhow::Result<Vec<ReturnValue<<DefaultBackend as Backend>::BV>>> {
    em.map(|bvretval|
           match bvretval {
	       Ok(ReturnValue::ReturnVoid) => {
		   Err(anyhow::Error::msg("Function shouldn't return void"))
	       },
	       Ok(v) => {
		   Ok(v)
	       },
	       Err(Error::LoopBoundExceeded(_)) => {
		   Err(anyhow::Error::msg("Loop bound exceeeded during analysis"))
	       },
	       Err(_) => {
		   Err(anyhow::Error::msg("Symex failure"))
	       }
           }).try_collect()
}

fn get_bvals<'b>(fn_name: &str, complexity: usize, state: &State<'b, DefaultBackend>, bv: &<DefaultBackend as Backend>::BV) -> anyhow::Result<HashSet<BVSolution>> {
    match state.get_possible_solutions_for_bv(bv, complexity).map_err(|e| anyhow::Error::msg(state.full_error_message_with_context(e)))? {
	AtLeast(_) => {
	    Err(anyhow::Error::msg(format!("Function `{}` is too complex to analyze with current --complexity setting", fn_name)))
	},
	Exactly(hs) => Ok(hs)
    }
}

fn analyze_fn(old_project: &Project, new_project: &Project, oldfn_name: &str, newfn_name: &str, verbose: u8, complexity: usize) -> anyhow::Result<()> {
    if verbose >= 1 {
	println!("Performing symbolic execution on  `{}` and '{}'", oldfn_name, newfn_name);
    }
    
    let mut old_em: ExecutionManager<DefaultBackend> =
        symex_function(oldfn_name, old_project, Config::default(), None).unwrap();
    let mut new_em: ExecutionManager<DefaultBackend> =
        symex_function(newfn_name, new_project, Config::default(), None).unwrap();

    if check_rval(old_project, &old_em)? != check_rval(new_project, &new_em)? {
	return Err(anyhow::Error::msg("Cannot compare: Return widths differ"));
    }

    let newvals = get_rvals(&mut new_em)?;
    let oldvals = get_rvals(&mut old_em)?;

    for old_bvalret in &oldvals {
	if verbose >= 1 {
	    println!("Solving equivalence for {:?}", old_bvalret);
	}
	let found_equivalence = newvals.iter().map(|new_bvalret| {
	    if verbose >= 2 {
		println!("  Against {:?}", new_bvalret);
	    }
	    match (&old_bvalret, new_bvalret) {
		(ReturnValue::Return(old_bv), ReturnValue::Return(new_bv)) => {
		    let old_solutions = get_bvals(oldfn_name, complexity, old_em.state(), old_bv)?;
		    let new_solutions = get_bvals(newfn_name, complexity, new_em.state(), new_bv)?;
		    
		    for old_solution in &old_solutions {
			for new_solution in &new_solutions {
			    if old_solution != new_solution {
				continue;
			    }println!("{:?}", old_solution);
			    // return values are the same
			    // but we've yet to check if they are the same for identical inputs
			    let old_param_bvs: Vec<_> = old_em.param_bvs().clone();
			    let new_param_bvs: Vec<_> = new_em.param_bvs().clone();
			    for old_param_bv in &old_param_bvs {
				let old_params = get_bvals(oldfn_name, complexity, old_em.state(), &old_param_bv)?;
				for new_param_bv in &new_param_bvs {
				    let new_params = get_bvals(newfn_name, complexity, new_em.state(), &new_param_bv)?;
				    for old_param in &old_params {
					for new_param in &new_params {
					    if old_param == new_param {
						return Ok(true);
					    }
					}
				    }
				}
			    }
			}
		    }
		},
		(ReturnValue::Throw(_old_bv, _), ReturnValue::Throw(_new_bv, _)) => {
		    todo!("Analyze thrown values");
		},
		(ReturnValue::Abort(_), ReturnValue::Abort(_)) => {
		    todo!("Analyze aborted values");
		},
		(_, _) => {
		    return Ok(false); // this isn't a match
		}
	    }
	    return Ok(false);
	}).collect::<Result<Vec<_>,anyhow::Error>>()?.into_iter().any(|e| e);

	if !found_equivalence {
	    return Err(anyhow::Error::msg(format!("Function `{}` isn't equivalent over updates!", newfn_name))); // TODO: more output
	}
    }

    Ok(())
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
