#[macro_use]
extern crate failure;
extern crate goblin;
extern crate toml;
#[macro_use]
extern crate log;
extern crate env_logger;

use std::io::Read;
use std::fs::File;
use std::process::{self, Command};
use std::env;
use std::path::{Path, PathBuf};
use std::collections::HashSet;

use failure::{Error};

use goblin::elf::Elf;

type Result<T> = ::std::result::Result<T, Error>;

fn get_workspace_root(args: &[String]) -> Result<PathBuf> {
    if let Some(p) = args.get(1) {
        Ok(Path::new(p).canonicalize()?)
    } else {
        let mut cur_dir = env::current_dir()?;
        if cur_dir.ends_with("megazord-symbol-checker") {
            cur_dir.pop();
        }
        Ok(cur_dir)
    }
}

fn build_android_lib(name: &str) -> Result<()> {
    // Do an android x86 release build. The arch shouldn't matter, and we use
    // release to maximize the chance that something will screw up and fail to
    // export the symbols.
    info!("Building {} for android (release, x86). This may take a while...", name);
    info!("  (run with RUST_LOG=trace for more verbose output)");
    let mut cmd = Command::new("./scripts/android-megazord.sh");
    cmd.args(&["x86", "release", name]);
    if !log_enabled!(log::Level::Trace) {
        cmd.stdout(process::Stdio::null()).stderr(process::Stdio::null());
    }
    let status = cmd.status()?;
    if !status.success() {
        eprintln!("Error: \
Failed to run `./scripts/android-megazord x86 release \"{}\"` (status: {:?}).
Things to try:
- Make sure you have the NDK set up, and have set the `ANDROID_NDK_TOOLCHAIN_DIR`
  and `ANDROID_NDK_API_VERSION` environment variables. We expect a standalone
  android x86 toolchain to be available at `$ANDROID_NDK_TOOLCHAIN_DIR/x86-$ANDROID_NDK_API_VERSION`
- Make sure you have your .cargo/config set for [target.i686-linux-android] 
  (should have `linker` and `ar`
- If these are correct you may need to run a `cargo clean` if you previously
  performed builds with slightly different configurations
", name, status);
        bail!("Child process exited with error status {:?}", status);
    }
    Ok(())
}

fn read_file_bytes<P: AsRef<Path>>(path: P, buffer: &mut Vec<u8>) -> Result<()> {
    let mut file = File::open(path)?;
    buffer.reserve(
        file.metadata().map(|m| m.len() as usize + 1).unwrap_or(0));
    file.read_to_end(buffer)?;
    Ok(())
}

fn read_toml<P: AsRef<Path>>(path: P) -> Result<toml::Value> {
    let mut file = File::open(path)?;
    let mut string = String::with_capacity(
        file.metadata().map(|m| m.len() as usize + 1).unwrap_or(0));
    file.read_to_string(&mut string)?;
    let toml = string.parse::<toml::Value>()?;
    Ok(toml)
}

fn get_table_key(toml: &toml::Value, table_name: &str, key: &str) -> Option<String> {
    toml.get(table_name)
        .and_then(|val| val.as_table())
        .and_then(|tab| tab.get(key))
        .and_then(|val| val.as_str())
        .map(|s| s.to_string())
}

#[derive(Clone, Debug)]
struct LibInfo {
    path: String,
    crate_name: String,
    lib_name: String,
}

impl LibInfo {
    pub fn new(path: &str) -> Result<LibInfo> {
        let mut cargo_toml_path: PathBuf = Path::new(path).to_owned();
        cargo_toml_path.push("Cargo.toml");

        let toml = read_toml(&cargo_toml_path)?;

        let crate_name = get_table_key(&toml, "package", "name").ok_or_else(||
            format_err!("Failed to locate [package] name in {}/Cargo.toml!", path))?;

        // [lib] name is "defaulted to the name of the package or project, with
        // any dashes replaced with underscores" according to cargo docs. 
        let lib_name = get_table_key(&toml, "lib", "name").unwrap_or_else(||
            crate_name.replace("-", "_"));

        Ok(LibInfo {
            path: path.into(),
            lib_name,
            crate_name,
        })
    }

    fn get_shared_object_path(&self) -> PathBuf {
        let mut buf: PathBuf = Path::new("./target/i686-linux-android/release").to_owned();
        buf.push(format!("lib{}.so", self.lib_name));
        buf
    }

    fn get_shared_object_bytes(&self, buf: &mut Vec<u8>) -> Result<()> {
        buf.clear();
        let path = self.get_shared_object_path();
        info!("Reading shared object from {:?}", path);
        Ok(read_file_bytes(self.get_shared_object_path(), buf)?)
    }

    pub fn build(&self) -> Result<()> {
        build_android_lib(&self.path)
    }

    pub fn parse_elf_symbols(&self, buf: &mut Vec<u8>) -> Result<HashSet<String>> {
        self.get_shared_object_bytes(buf)?;
        info!("Parsing lib{}.so", self.lib_name);
        let sofile = Elf::parse(&buf)?;
        let mut symbols: HashSet<String> = HashSet::new();

        let sym_names = sofile.dynstrtab;
        for sym in sofile.dynsyms.iter() {
            if sym.st_name == 0 {
                // first entry is always all 0s (probably should check this is the first...)
                continue;
            }
            if sym.is_import() {
                // We don't care about ELF imports, but log them at trace level.
                trace!(" {}: Skipping imported symbol {} ({:?})", self.crate_name,
                    sym_names.get(sym.st_name).unwrap_or(Ok("(unknown)"))
                    .unwrap_or("(unknown)"), sym);
                continue;
            }
            if !sym.is_function() {
                // Not clear if this is the right choice, but I doubt anybody
                // is exporting globals this way. Warn if they are because this
                // seems like a bad idea.
                warn!(" {}: Skipping non-function symbol {} ({:?})", self.crate_name,
                    sym_names.get(sym.st_name).unwrap_or(Ok("(unknown)"))
                    .unwrap_or("(unknown)"), sym);
                continue;
            }
            match sym_names.get(sym.st_name) {
                Some(Ok(s)) => {
                    debug!("{}: FOUND SYM {} {:?}", self.crate_name, s, sym);
                    symbols.insert(s.into());
                },
                other => {
                    warn!("{}: Something went wrong finding symbol!: {:?}, {:?}",
                          self.crate_name, other, sym)
                }
            }
        }
        info!("{}: Found {} symbols", self.crate_name, symbols.len());
        Ok(symbols)
    }
}

fn main() -> Result<()> {
    // Default log level is info, since this is slow and we want to indicate
    // what we're doing somewhat.
    let env = env_logger::Env::default().filter_or("RUST_LOG", "info");
    env_logger::init_from_env(env);
    let args = env::args().collect::<Vec<_>>();
    let root = get_workspace_root(&args)?;
    info!("Using workspace root: {:?}", root);
    env::set_current_dir(&root)?;

    let megazord = LibInfo::new("ffi-megazord")?;
    let components = &[
        LibInfo::new("fxa-client/ffi")?,
        LibInfo::new("sync15/passwords/ffi")?,
    ];
    // Build all required libs
    megazord.build()?;
    for lib in components {
        lib.build()?;
    }
    let mut buf: Vec<u8> = vec![];

    let preserved = megazord.parse_elf_symbols(&mut buf)?;
    let mut missing: Vec<(String, &LibInfo)> = vec![];
    for lib in components {
        let from_this_lib = lib.parse_elf_symbols(&mut buf)?;
        for sym in from_this_lib {
            if !preserved.contains(&sym) {
                eprintln!("Error: {} doesn't contain symbol from {}: {}!", megazord.crate_name, lib.crate_name, sym);
                missing.push((sym.clone(), &lib));
            }
        }
    }

    if missing.len() != 0 { 
        eprintln!("lib{}.so is missing {} symbols:", megazord.lib_name, missing.len());
        for (symbol, lib) in missing {
            eprintln!("  - from lib{}.so: {}", lib.lib_name, symbol);
        }
        process::exit(1);
    }
    println!("All expected symbols accounted for!");
    Ok(())
}
