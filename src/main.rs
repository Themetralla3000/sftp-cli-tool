mod ssh;
use crate::ssh::{
    authenticate, build_remote_tree, build_tree, connect, download_file, download_tree,
    transfer_file, transfer_tree,
};
use clap::Parser;
use ssh2::Sftp;
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
    preserve: bool,
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

enum Endpoint {
    Local(String),
    Remote(Destination),
}

fn parse_endpoint(input: &str) -> Endpoint {
    match parse_destination(input) {
        Ok(dest) => Endpoint::Remote(dest),
        Err(_) => Endpoint::Local(input.to_string()),
    }
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

fn connect_and_auth(remote: &Destination, port: u16, key_path: Option<&str>) -> Sftp {
    let session = match connect(&remote.host, port) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    match authenticate(&session, &remote.user, key_path) {
        Ok(()) => match session.sftp() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error opening sftp: {}", e);
                std::process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn main() {
    let args = Args::parse();

    let origin = parse_endpoint(&args.origin);
    let destination = parse_endpoint(&args.destination);

    match (origin, destination) {
        //Push
        (Endpoint::Local(local), Endpoint::Remote(remote)) => {
            let sftp = connect_and_auth(&remote, args.port, args.key_path.as_deref());

            let basename = match Path::new(&local).file_name() {
                Some(name) => name.to_string_lossy(),
                None => {
                    eprintln!("Error: invalid source path: {}", local);
                    std::process::exit(1);
                }
            };
            let remote_target = match sftp.stat(Path::new(&remote.path)) {
                Ok(s) if s.is_dir() => format!("{}/{}", remote.path, basename),
                _ => remote.path.clone(),
            };

            if Path::new(&local).is_dir() {
                if !args.recursive {
                    eprintln!("Error: {} is a directory", local);
                    std::process::exit(1);
                }
                let mut tree = Vec::new();
                if let Err(e) = build_tree(&local, &remote_target, 0, &mut tree) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                if let Err(e) = transfer_tree(&sftp, &tree, args.preserve) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            } else if let Err(e) =
                transfer_file(&sftp, &local, &remote_target, args.preserve, |_| {})
            {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }

            println!("File transferred correctly");
        }
        //pull
        (Endpoint::Remote(remote), Endpoint::Local(local)) => {
            let sftp = connect_and_auth(&remote, args.port, args.key_path.as_deref());

            //if the local destination is an existing dir, append the remote file name instead of overwriting the dir
            let basename = match Path::new(&remote.path).file_name() {
                Some(name) => name.to_string_lossy(),
                None => {
                    eprintln!("Error: invalid remote path: {}", remote.path);
                    std::process::exit(1);
                }
            };
            let local_target = if Path::new(&local).is_dir() {
                format!("{}/{}", local, basename)
            } else {
                local
            };
            //si es dir

            match sftp.stat(Path::new(&remote.path)) {
                //exists and is a dir
                Ok(s) if s.is_dir() => {
                    if !args.recursive {
                        eprintln!("Error, {} is a directory", remote.path);
                        std::process::exit(1)
                    }
                    let mut tree = Vec::new();
                    if let Err(e) =
                        build_remote_tree(&sftp, &local_target, &remote.path, 0, &mut tree)
                    {
                        eprintln!("Error building remote tree: {}", e);
                        std::process::exit(1);
                    }
                    if let Err(e) = download_tree(&sftp, &tree) {
                        eprintln!("Error downloading tree: {}", e);
                        std::process::exit(1);
                    }
                }
                Ok(_) => {
                    if let Err(e) = download_file(&sftp, &remote.path, &local_target, |_| {}) {
                        eprintln!("Error downloading file: {}", e);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {} on remote path {}", e, remote.path);
                    std::process::exit(1);
                }
            }
        }

        //invalid convinations (two locals or two remotes)
        (Endpoint::Remote(_), Endpoint::Remote(_)) => {
            eprintln!("Error: remote-to-remote transfers are not supported");
            std::process::exit(1);
        }
        (Endpoint::Local(_), Endpoint::Local(_)) => {
            eprintln!("Error: at least one endpoint must be remote");
            std::process::exit(1);
        }
    }
}
