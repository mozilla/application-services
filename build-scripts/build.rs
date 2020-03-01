/*
* This is a script to clobber the {desktop, ios, android} directories
* and rebuild libs via the build-all.sh script..
*/

use std::fs;
use std::path::Path;
use std::env;
use std::process::Command;
use std::io::prelude::*;

fn main() -> std::io::Result<()> {
    let root = Path::new("../libs");
    println!("moving to {} directory\n", root.display());
    assert!(env::set_current_dir(&root).is_ok());
    println!("changed working directory to {}.\n", root.display());

    let mut file = fs::File::open("libs-version")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    //println!("{}\n", contents);

    // Deletes the following directories if they exists, then rebuilds libs
    if contents.to_uppercase() != "UPDATED: FALSE" {

        //the .exists function is currently not working?
        println!("deleting old directories and rebuilding /libs...\n");
        if Path::new("../libs/desktop").exists() {
            fs::remove_dir_all("desktop")?;
        }
        println!("here/n");

        if Path::new("../libs/android").exists() {
            fs::remove_dir_all("/android")?;
        }
        if Path::new("../libs/ios").exists() {
            fs::remove_dir_all("/ios")?;
        }
        //the rest of this code is for calling the .sh script
        let mut cmd = Command::new("bash");
        println!("here/n");
        cmd.arg("./build-all.sh");

        match cmd.output() {
            Ok(_o) => {}
            Err(_e) => {
                println!("Counld not rebuild libs. You will need to run the script manually.\n");
            }
        }
    }

    fs::write("libs-version", "UPDATED: FALSE")?;
    Ok(())
}
