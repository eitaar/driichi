use std::{
    env,
    io::{self, IsTerminal},
};

use double_riichi_server::hash_password_for_cli;

fn main() {
    let mut arguments = env::args().skip(1);
    match (arguments.next().as_deref(), arguments.next()) {
        (Some("hash-password"), None) => {
            if let Err(error) = hash_password_command() {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("usage: driichi hash-password");
            std::process::exit(2);
        }
    }
}

fn hash_password_command() -> Result<(), String> {
    let password = read_secret("Password: ")?;
    let confirmation = read_secret("Confirm password: ")?;
    match hash_password_for_cli(&password, &confirmation)
        .map_err(|_| "password is invalid".to_owned())?
    {
        Some(hash) => {
            println!("{hash}");
            Ok(())
        }
        None => Err("password confirmation does not match".to_owned()),
    }
}

fn read_secret(prompt: &str) -> Result<String, String> {
    eprint!("{prompt}");
    if io::stdin().is_terminal() {
        rpassword::read_password().map_err(|_| "could not read password".to_owned())
    } else {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        rpassword::read_password_from_bufread(&mut input)
            .map_err(|_| "could not read password".to_owned())
    }
}
