mod ssh;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "mysftp")]
#[command(about = "a cli sftp client")]
#[command(version = "0.1.0")]
struct Args{
    //archivo o directorio a transferir
    origin: String,
    //destinatario en formato user@host:ruta_destino
    destination: String,
    //puerto
    //[arg(short long)] permite referirse como una flag, short -> -p ; long -> --port
    //default value es 22, se puede omitir esta flag
    #[arg(short,long, default_value_t=22)]
    port: u16,

    // ruta a las claves ssh, opcional
    #[arg(short = 'i', long = "identity")]
    key_path: Option<String>,
    
    //recursivo, opcional tambien
    #[arg(short, long)]
    recursive: bool,
}
//El objeto donde guardo la información del destino
struct Destination {
    user: String,
    host: String,
    path: String
}
fn parse_destination(input: &str) -> Result<Destination, String> {

    let (user,rest) = input
        .split_once('@')
        .ok_or("Invalid format, @ missing")?;

    let (host, path) = rest
        .split_once(':')
        .ok_or("Invalid format, : missing")?;

    if user.is_empty(){
        return Err("User cannot be empty".to_string());
    }

    if host.is_empty(){
        return Err("Host cannot be empty".to_string());
    }

    Ok(Destination { 
        user: (user.to_string()),
        host: (host.to_string()),
        path: (path.to_string()) 
    })
    }

fn main() {
    let args = Args::parse();

    match parse_destination(&args.destination) {
        Ok(dest) => {
            println!("Conectando a {}@{}:{}...", dest.user, dest.host, args.port);


            let session = match ssh::connect(&dest.host, args.port) {
                Ok(s) => {
                    println!("Conexión establecida y handshake completado.");
                    s
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            match ssh::authenticate(&session, &dest.user, args.key_path.as_deref()) {
                Ok(()) => {
                    println!("Autenticación exitosa.");
                    //aqui va lo chungo xd
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

