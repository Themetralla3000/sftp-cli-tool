use rpassword::read_password;
use ssh2::Session;
use std::io::Write;
use std::net::TcpStream;
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

pub fn transfer_file(session: &Session, local: &str, remote: &str) -> Result<(), String> {
    let local_path = Path::new(local);
    if !local_path.exists() {
        return Err(format!("Local file not found: {}", local));
    }
    if !local_path.is_file() {
        return Err(format!("Path not a file: {}", local));
    }
    let raw_data = std::fs::read(local).map_err(|e| format!("Error, could not read: {}", e))?;
    let sftp_session = session
        .sftp()
        .map_err(|e| format!("Error, could not create sftp session: {}", e))?;
    let mut remote_file = sftp_session
        .create(Path::new(remote))
        .map_err(|e| format!("Error creating remote {}: {}", remote, e))?;
    remote_file
        .write_all(&raw_data)
        .map_err(|e| format!("Error writing file: {}", e))?;
    //no need to close connection or anything, when the function leaves its scope, it drops
    //everything instanciated wujuu
    Ok(())
}

pub fn transfer_dir(session: &Session, local: &str, remote: &str) -> Result<(), String> {
    let sftp = session
        .sftp()
        .map_err(|e| format!("Error creating sftp session: {}", e))?;
    sftp.mkdir(Path::new(remote), 0o755)
        .map_err(|e| format!("Error creating directory: {}", e))?;

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
            transfer_dir(session, local_path, &remote_path)?;
        } else {
            //is a file
            transfer_file(session, local_path, &remote_path)?;
        }
    }

    Ok(())
}
