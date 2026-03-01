use std::net::TcpStream;
use ssh2::Session;
use std::path::Path;
use rpassword::read_password;

pub fn connect(host: &str, port: u16) -> Result<Session,String>{
    
    let address = format!("{}:{}",host,port);
    //la conexion pide en formato host:port
    let tcp = TcpStream::connect(&address)
            .map_err(|e| format!("Error connecting to {}: {}", address, e))?;
    
    let mut session = Session::new()
            .map_err(|e| format!("Error creating ssh session: {}",e))?;
    session.set_tcp_stream(tcp);

    session.handshake()
    .map_err(|e| format!("Error in hanshake: {}",e))?;

    Ok(session)

}

pub fn authenticate(session: &Session, user: &str, key_path: Option<&str>) -> Result<(),String>{
    //si pasan rut a clave ssh
    if let Some(path) = key_path{
        return try_key_auth(session,user,path);
    }

    //sino la pasan la buscamos
    if let Some(home) = std::env::var("HOME").ok() {
        //ed25519 primero
        let ed25519_path = format!("{}/.ssh/id_ed25519", home);
        if Path::new(&ed25519_path).exists() {
            if let Ok(()) = try_key_auth(session, user, &ed25519_path) {
                return Ok(());
            }
            //si falla probamos la siguiente
        }

        //rsa
        let rsa_path = format!("{}/.ssh/id_rsa", home);
        if Path::new(&rsa_path).exists() {
            if let Ok(()) = try_key_auth(session, user, &rsa_path) {
                return Ok(());
            }
        }
    }
    password_auth(session,user)
}


fn try_key_auth(session: &Session, user: &str, key_path: &str) -> Result<(),String>{
    let path = Path::new(key_path);

    if !path.exists(){
        return Err(format!("Error, key not found: {}",key_path));
    }

    session
        .userauth_pubkey_file(user, None, path, None)
        .map_err(|e| format!("Error trying auth with passkey {} : {}",key_path,e))?;
    
    Ok(())
}

fn password_auth(session: &Session, user: &str) -> Result<(),String>{
    eprint!("Password for {} :",user);

    let password = read_password()
                                            .map_err(|e| format!("Error reading password: {}",e))?;
    session.userauth_password(user, &password)
            .map_err(|e| format!("Error authenticating: {}",e))?;
    
    Ok(())
}