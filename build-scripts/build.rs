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

    let most_recent_version = String::new();
    //this line may not be working for some reason:
    //this should be the equivalent of the cd command
    let root = Path::new("../libs");
    println!("moving to {} directory\n", root.display());
    assert!(env::set_current_dir(&root).is_ok());
    println!("changed working directory to {}.\n", root.display());

    let mut file = fs::File::open("libs-version")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    println!("{}\n", contents);

    // Deletes the following directories if they exist
    // then rebuilds libs
    if contents.to_uppercase() != "UPDATED: TRUE" {

        println!("deleting old directories and rebuilding /libs...\n");
        //I think we can ignore errors generated here.
        //for example deleting directories that don't exist.
        fs::remove_dir_all("/desktop");
        println!("here/n");
        //println!("0\n");
        //fs::remove_dir_all("/libs/android");
        //fs::remove_dir_all("/libs/ios");

        //the rest of this code is for calling the .sh script
        let mut cmd = Command::new("bash");
        println!("here/n");
        cmd.arg("./build-all.sh");

        match cmd.output(){
            Ok(o)=> {

            }
            Err(e)=> {
                println!("Counld not rebuild libs. You will need to run the script manually.\n");
            }
        }

    }
    println!("Build script done.\n");
    //this will create a file if it doesn't yet exist
    //and write the new version code to it.
    fs::write("libs-version", "UPDATED: FALSE")?;
    Ok(())
}
