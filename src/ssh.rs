use rpassword::read_password;
use ssh2::{Session, Sftp};
use std::fs::metadata;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub fn connect(host: &str, port: u16) -> Result<Session, String> {
    let address = format!("{}:{}", host, port);
    //the connection expects host:port format
    let tcp = TcpStream::connect(&address)
        .map_err(|e| format!("Error connecting to {}: {}", address, e))?;

    let mut session = Session::new().map_err(|e| format!("Error creating ssh session: {}", e))?;
    session.set_tcp_stream(tcp);

    session
        .handshake()
        .map_err(|e| format!("Error in handshake: {}", e))?;
    Ok(session)
}

pub fn authenticate(session: &Session, user: &str, key_path: Option<&str>) -> Result<(), String> {
    //if an ssh key path is passed
    if let Some(path) = key_path {
        return try_key_auth(session, user, path);
    }

    //if not passed, we look for it
    if let Ok(home) = std::env::var("HOME") {
        //ed25519 first
        let ed25519_path = format!("{}/.ssh/id_ed25519", home);
        if Path::new(&ed25519_path).exists()
            && let Ok(()) = try_key_auth(session, user, &ed25519_path)
        {
            return Ok(());

            //if it fails we try the next one
        }

        //rsa
        let rsa_path = format!("{}/.ssh/id_rsa", home);
        if Path::new(&rsa_path).exists()
            && let Ok(()) = try_key_auth(session, user, &rsa_path)
        {
            return Ok(());
        }
    }
    password_auth(session, user)
}

fn try_key_auth(session: &Session, user: &str, key_path: &str) -> Result<(), String> {
    let path = Path::new(key_path);

    if !path.exists() {
        return Err(format!("Error, key not found: {}", key_path));
    }

    session
        .userauth_pubkey_file(user, None, path, None)
        .map_err(|e| format!("Error trying auth with passkey {} : {}", key_path, e))?;

    Ok(())
}

fn password_auth(session: &Session, user: &str) -> Result<(), String> {
    eprint!("Password for {} :", user);

    let password = read_password().map_err(|e| format!("Error reading password: {}", e))?;
    session
        .userauth_password(user, &password)
        .map_err(|e| format!("Error authenticating: {}", e))?;

    Ok(())
}

pub fn transfer_file(
    sftp: &Sftp,
    local: &str,
    remote: &str,
    preserve: bool,
    mut on_progress: impl FnMut(u64),
) -> Result<(), String> {
    let local_path = Path::new(local);
    if !local_path.exists() {
        return Err(format!("Local file not found: {}", local));
    }
    if !local_path.is_file() {
        return Err(format!("Path not a file: {}", local));
    }

    let mut file =
        std::fs::File::open(local).map_err(|e| format!("Error opening local file: {}", e))?;

    let mut remote_file = sftp
        .create(Path::new(remote))
        .map_err(|e| format!("Error creating remote {}: {}", remote, e))?;

    //let file_length = std::fs::metadata(local).map_err(|e| format!("Could not read file metadata: {}", e))?.len();
    //
    let mut buf = [0u8; 32 * 1024]; //32kb
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Error reading file chunk: {}", e))?;
        if n == 0 {
            break;
        }

        remote_file
            .write_all(&buf[..n])
            .map_err(|e| format!("could not write from buffer: {}", e))?;
        on_progress(n as u64);
    }

    //check metadata
    if preserve {
        apply_metadata(sftp, local, remote)?;
    }

    Ok(())
}
pub fn ensure_remote_dir(sftp: &Sftp, remote: &str) -> Result<(), String> {
    match sftp.stat(Path::new(remote)) {
        Ok(s) if s.is_dir() => {} // ja existeix
        Ok(_) => return Err(format!("{} exists and is not a dir", remote)),
        Err(_) => sftp
            .mkdir(Path::new(remote), 0o755)
            .map_err(|e| format!("Error creating directory: {}", e))?,
    }
    Ok(())
}
//given the tree of files, transfer all
pub fn transfer_tree(sftp: &Sftp, nodes: &[Node], preserve: bool) -> Result<(), String> {
    for node in nodes {
        if node.is_dir {
            println!("{}/{}", "  ".repeat(node.depth), node.name);
            ensure_remote_dir(sftp, &node.remote_path)?;
        } else {
            //for the ansi art clossure:
            let indent = "  ".repeat(node.depth);
            let total = node.size;
            let mut transferred: u64 = 0;
            transfer_file(sftp, &node.local_path, &node.remote_path, preserve, |n| {
                transferred += n;
                let pct = if total == 0 {
                    100
                } else {
                    transferred * 100 / total
                };
                let filled = (pct / 5) as usize;
                let bar = "█".repeat(filled) + &"░".repeat(20 - filled);
                print!("\r\x1b[K{}{} [{}] {}%", indent, node.name, bar, pct);
                //how to solve all your problems, ignore theme
                let _ = std::io::stdout().flush();
            })?;
        }
    }

    //if necessary write metadata (this happens AFTER everything is created bc every time a file is
    //created inside a directory, the accesed time changes)
    if preserve {
        for node in nodes.iter().rev() {
            if node.is_dir {
                apply_metadata(sftp, &node.local_path, &node.remote_path)?;
            }
        }
    }

    Ok(())
}

fn apply_metadata(sftp: &Sftp, local: &str, remote: &str) -> Result<(), String> {
    // llegir meta de local, muntar FileStat, setstat a remote

    let meta =
        std::fs::metadata(local).map_err(|e| format!("Error, could not read metadata: {}", e))?;
    let mode = meta.permissions().mode();
    let mtime = meta
        .modified()
        .map_err(|e| format!("could not read systime: {}", e))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("could not convert to unix time: {}", e))?
        .as_secs();
    let accesed = meta
        .accessed()
        .map_err(|e| format!("Could not read acces time: {}", e))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("could not convert to unix time: {}", e))?
        .as_secs();

    let stat = ssh2::FileStat {
        size: None,
        uid: None,
        gid: None,
        perm: Some(mode),
        atime: Some(accesed),
        mtime: Some(mtime),
    };

    sftp.setstat(Path::new(remote), stat)
        .map_err(|e| format!("Error apllying metadata to {}: {}", remote, e))?;

    Ok(())
}

//for the ansi art that prints the progress as a file tree, i need to create a struct to encapsulate
//all the
pub struct Node {
    pub depth: usize,
    pub name: String,
    pub local_path: String,
    pub remote_path: String,
    pub is_dir: bool,
    pub size: u64,
}
//before sending anything, first we build the tree and then send it with the tree
pub fn build_tree(
    local: &str,
    remote: &str,
    depth: usize,
    nodes: &mut Vec<Node>,
) -> Result<(), String> {
    nodes.push(Node {
        depth,
        name: Path::new(local)
            .file_name()
            .ok_or(format!("invalid path: {}", local))?
            .to_string_lossy()
            .to_string(),
        local_path: local.to_string(),
        remote_path: remote.to_string(),
        is_dir: true,
        size: 0,
    });

    for entry in std::fs::read_dir(local).map_err(|e| format!("Could not open dir: {}", e))? {
        //shadowing
        let entry = entry.map_err(|e| format!("Error: {}", e))?;
        let path = entry.path();
        let name = entry.file_name();
        let local_path = path.to_str().ok_or(format!(
            "Error: /mnt/shared/mcps/mcp_accountinginvalid path"
        ))?;
        let remote_path = format!("{}/{}", remote, name.to_string_lossy());

        if path.is_dir() {
            build_tree(&local_path, &remote_path, depth + 1, nodes)?;
        } else {
            nodes.push(Node {
                depth: depth + 1,
                name: name.to_string_lossy().to_string(),
                local_path: local_path.to_string(),
                remote_path,
                is_dir: false,
                size: metadata(local_path)
                    .map_err(|e| format!("Metadata error: {}", e))?
                    .len(),
            });
        }
    }

    Ok(())
}
