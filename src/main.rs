mod ssh;
use clap::Parser;
use std::path::Path;
#[derive(Parser, Debug)]
#[command(name = "mysftp")]
#[command(about = "a cli sftp client")]
#[command(version = "0.1.0")]
struct Args {
    //file or directory to transfer
    origin: String,
    //recipient in user@host:dest_path format
    destination: String,
    //port
    //[arg(short, long)] allows referring to it as a flag, short -> -p ; long -> --port
    //default value is 22, this flag can be omitted
    #[arg(short, long, default_value_t = 22)]
    port: u16,

    // path to the ssh keys, optional
    #[arg(short = 'i', long = "identity")]
    key_path: Option<String>,

    //recursive, also optional
    #[arg(short, long)]
    recursive: bool,
}
//The struct where I store the destination info
struct Destination {
    user: String,
    host: String,
    path: String,
}
fn parse_destination(input: &str) -> Result<Destination, String> {
    let (user, rest) = input.split_once('@').ok_or("Invalid format, @ missing")?;

    let (host, path) = rest.split_once(':').ok_or("Invalid format, : missing")?;

    if user.is_empty() {
        return Err("User cannot be empty".to_string());
    }

    if host.is_empty() {
        return Err("Host cannot be empty".to_string());
    }

    Ok(Destination {
        user: (user.to_string()),
        host: (host.to_string()),
        path: (path.to_string()),
    })
}

fn main() {
    let args = Args::parse();

    match parse_destination(&args.destination) {
        Ok(dest) => {
            println!("Connecting to {}@{}:{}...", dest.user, dest.host, args.port);

            let session = match ssh::connect(&dest.host, args.port) {
                Ok(s) => {
                    println!("Connection established and handshake completed.");
                    s
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            match ssh::authenticate(&session, &dest.user, args.key_path.as_deref()) {
                Ok(()) => {
                    println!("Authentication successful.");
                    //the hard part goes here
                    if Path::new(&args.origin).is_dir() {
                        if !args.recursive {
                            // r + not directory
                            eprintln!("Error: {} is a directory", args.origin);
                            std::process::exit(1);
                        }

                        if let Err(e) = ssh::transfer_dir(&session, &args.origin, &dest.path) {
                            eprintln!("Error: {}", e);
                            std::process::exit(1);
                        }
                    } else {
                        //is file
                        if let Err(e) = ssh::transfer_file(&session, &args.origin, &dest.path) {
                            eprintln!("Error: {}", e);
                            std::process::exit(1);
                        }
                    }

                    //recursive

                    println!("File transferred correctly");
                }

                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
