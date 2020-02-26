/*
*This compiles, the relative file paths need to be messed with i think.
*Apparently to grt the most_recent_version variable we need to read it from
*something that was updated durring a PR.
*This means we need to edit their PR process such that when someone makes a change
*that effects libs, they have to enter a new version number.
*/


use std::fs;
use std::path::Path;
use std::process::Command;
use std::io::prelude::*;

fn main() -> std::io::Result<()> {

    let most_recent_version = String::new();
    //this line may not be working for some reason:
    //this should be the equivalent of the cd command
    let root = Path::new("/libs");

    println!("moving to {} directory\n", root.display());
    let mut file = fs::File::open("libs-version")?;
    println!("here\n");
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Deletes the following directories if they exist
    // then rebuilds libs
    if contents != most_recent_version {

        println!("deleting old directories and rebuilding /libs...\n");
        //I think we can ignore errors generated here.
        //for example deleting directories that don't exist.
        fs::remove_dir_all("/libs/desktop");
        //println!("0\n");
        fs::remove_dir_all("/libs/android");
        fs::remove_dir_all("/libs/ios");

        //the rest of this code is for calling the .sh script
        let mut cmd = Command::new("bash");
        cmd.arg("./build-all.sh");

        match cmd.output(){
            Ok(o)=> {

            }
            Err(e)=> {
                println!("Counld not rebuild libs. You will need to run the script manually.\n");
            }
        }

    }
    //this will create a file if it doesn't yet exist
    //and write the new version code to it.
    fs::write("/ibs/version", most_recent_version)?;
    Ok(())
}
