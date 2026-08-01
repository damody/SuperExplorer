use std::{
    env, fs,
    net::{SocketAddr, TcpStream},
    process::Command,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn main() {
    let nonce = env::var("SUPEREXPLORER_SENTINEL_NONCE").expect("sentinel nonce missing");
    if nonce.len() < 16 {
        panic!("sentinel nonce is invalid");
    }
    let marker = env::var("SUPEREXPLORER_SENTINEL_MARKER").expect("sentinel marker missing");
    println!("cargo:rerun-if-env-changed=SUPEREXPLORER_SENTINEL_NONCE");
    let timeout = Duration::from_millis(500);
    let addr: SocketAddr = "1.1.1.1:443".parse().unwrap();
    let (direct, direct_error) = match TcpStream::connect_timeout(&addr, timeout) {
        Ok(_) => (true, String::from("connected")),
        Err(error) => (false, error.to_string()),
    };
    let mut child = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", "$d=$false;$c=$false;try{[Net.Dns]::GetHostAddresses('example.com')|Out-Null;$d=$true}catch{};try{$x=[Net.Sockets.TcpClient]::new();$x.ConnectAsync('1.1.1.1',443).Wait(500)|Out-Null;$c=$x.Connected;$x.Dispose()}catch{};if($d -or $c){exit 7}elseif(-not $d -and -not $c){exit 0}else{exit 9}"]).spawn().expect("child egress probe failed to start");
    let start = std::time::Instant::now();
    let child_code = loop {
        if let Some(status) = child.try_wait().expect("child probe wait failed") {
            break status.code().unwrap_or(9);
        }
        if start.elapsed() > Duration::from_secs(3) {
            child.kill().ok();
            break 9;
        }
        thread::sleep(Duration::from_millis(25));
    };
    if direct || child_code == 7 {
        panic!("egress succeeded");
    }
    if child_code != 0 {
        panic!("egress probe failed closed");
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let pid = std::process::id();
    let json = format!(
        r#"{{"nonce":"{nonce}","pid":{pid},"unix_timestamp":{ts},"direct":"blocked","child":"blocked","direct_error":"{direct_error}","child_detail":"probe exit 0"}}"#
    );
    fs::write(marker, json).expect("sentinel marker write failed");
    println!("cargo:warning=offline egress sentinel passed");
}
