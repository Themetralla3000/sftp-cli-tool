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
    #[arg(short = 'P', long = "port", default_value_t = 22)]
    port: u16,

    #[arg(short, long)]
    presserve: bool,
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
                    let sftp = match session.sftp() {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("Error opening sftp: {}", e);
                            std::process::exit(1)
                        }
                    };

                    //to match how scp handles creating files from the path adding intermidiate
                    //directories, I have to first construct the path

                    let basename = match Path::new(&args.origin).file_name() {
                        Some(name) => name.to_string_lossy(),
                        None => {
                            eprintln!("Error: invalid source path: {}", args.origin);
                            std::process::exit(1);
                        }
                    };
                    let remote_target = match sftp.stat(Path::new(&dest.path)) {
                        Ok(s) if s.is_dir() => format!("{}/{}", dest.path, basename),
                        _ => dest.path.clone(),
                    };

                    if Path::new(&args.origin).is_dir() {
                        if !args.recursive {
                            // r + not directory
                            eprintln!("Error: {} is a directory", args.origin);
                            std::process::exit(1);
                        }
                        //1. walk the local file tree into a node vector
                        let mut tree = Vec::new();
                        if let Err(e) = ssh::build_tree(&args.origin, &remote_target, 0, &mut tree)
                        {
                            eprintln!("Error: {}", e);
                            std::process::exit(1);
                        }
                        //2. transfer the whole vector
                        if let Err(e) = ssh::transfer_tree(&sftp, &tree, args.presserve) {
                            eprintln!("Error: {}", e);
                            std::process::exit(1);
                        }
                    } else {
                        //is file
                        if let Err(e) =
                            ssh::transfer_file(&sftp, &args.origin, &remote_target, args.presserve)
                        {
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
