use rpassword::read_password;
use ssh2::{Session, Sftp};
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

pub fn transfer_file(sftp: &Sftp, local: &str, remote: &str, preserve: bool) -> Result<(), String> {
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

    let file_length = std::fs::metadata(local)
        .map_err(|e| format!("Could not read file metadata: {}", e))?
        .len();
    let mut progress = 0;
    let mut percentage = 0;
    let mut buf = [0u8; 32 * 1024]; //32kb
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Error reading file chunk: {}", e))?;
        progress += n as u64;
        percentage = progress * 100 / file_length;
        if n == 0 {
            break;
        }

        remote_file
            .write_all(&buf[..n])
            .map_err(|e| format!("could not write from buffer: {}", e))?;

        print!("\r{}%", percentage);
        std::io::stdout()
            .flush()
            .map_err(|e| format!("Error: {}", e))?;
    }

    //check metadata
    if preserve {
        apply_metadata(sftp, local, remote)?;
    }

    Ok(())
}

pub fn transfer_dir(sftp: &Sftp, local: &str, remote: &str, preserve: bool) -> Result<(), String> {
    match sftp.stat(Path::new(remote)) {
        Ok(stat) if stat.is_dir() => {
            //exists and it's a directory
        }
        Ok(_) => {
            //exists but it's not a directory
            return Err(format!(
                "Remote path {} exists and it's not a directory",
                remote
            ));
        }
        Err(_) => {
            sftp.mkdir(Path::new(remote), 0o755)
                .map_err(|e| format!("Error creating directory: {}", e))?;
        }
    }

    for entry in std::fs::read_dir(local).map_err(|e| format!("Error opening iterator: {}", e))? {
        // read_dir returns iterator, entry now is a element of the iterator, so at first is a
        // Result<DirEntry, Error>
        // need to unpack first
        let entry = entry.map_err(|e| format!("Error: {}", e))?; //this is called
        //shadowing, it means to create a new variable shadowing the previous one
        let path = entry.path();
        let name = entry.file_name();

        //now for invoking the recursion, i need to create the path the way transfer_dir uses it
        let remote_path = format!("{}/{}", remote, name.to_string_lossy());
        let local_path = path.to_str().ok_or("Path is not a valid UTF-8")?;
        //Recursion decission: file or directory:
        if path.is_dir() {
            //recursion
            transfer_dir(sftp, local_path, &remote_path, preserve)?;
        } else {
            //is a file
            transfer_file(sftp, local_path, &remote_path, preserve)?;
        }
    }
    if preserve {
        apply_metadata(sftp, local, remote)?;
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
