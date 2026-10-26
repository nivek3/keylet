mod catalog;
mod macos;
mod model;
mod theme;
mod ui;

use model::{Curve, Failure, KeyRequest, Result};
use std::{
    io::{Read, Write},
    os::fd::FromRawFd,
};

fn emit(value: &serde_json::Value) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, value).map_err(|_| Failure::Transport)?;
    stdout.write_all(b"\n").map_err(|_| Failure::Transport)?;
    stdout.flush().map_err(|_| Failure::Transport)
}
fn pipe_descriptor(fd: i32) -> bool {
    unsafe {
        let mut stat = std::mem::zeroed::<libc::stat>();
        if libc::fstat(fd, &mut stat) != 0 {
            return false;
        }
        matches!(stat.st_mode & libc::S_IFMT, libc::S_IFIFO | libc::S_IFSOCK)
    }
}
fn read_request(input: &mut impl Read, session: &mut macos::AuthenticationSession, fd: Option<i32>) -> Result<Option<KeyRequest>> {
    let mut data = Vec::new();
    loop {
        if let Some(fd) = fd {
            loop {
                let now = std::time::Instant::now();
                session.expire(now);
                let timeout = session.remaining(now)
                    .map(|time| time.as_millis().min(i32::MAX as u128) as i32).unwrap_or(-1);
                let mut poll = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
                let ready = unsafe { libc::poll(&mut poll, 1, timeout) };
                if ready > 0 { break; }
                if ready < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                    return Err(Failure::Transport);
                }
            }
        }
        let mut byte = [0];
        let count = input.read(&mut byte).map_err(|_| Failure::Transport)?;
        if count == 0 {
            return if data.is_empty() { Ok(None) } else { Err(Failure::Input) };
        }
        if byte[0] == b'\n' { break; }
        data.push(byte[0]);
        if data.len() > 2048 { return Err(Failure::Input); }
    }
    let request: KeyRequest = serde_json::from_slice(&data).map_err(|_| Failure::Input)?;
    request.validate()?;
    Ok(Some(request))
}
fn bridge(vault: &macos::Vault, requested_ttl: u64) -> Result<()> {
    if !pipe_descriptor(0) || !pipe_descriptor(3) {
        return Err(Failure::Transport);
    }
    macos::initialize_accessory()?;
    let catalog = catalog::Catalog::for_group(&vault.group)?;
    let mut session = macos::AuthenticationSession::default();
    let mut input = unsafe { std::fs::File::from_raw_fd(0) };
    let mut secret_pipe = unsafe { std::fs::File::from_raw_fd(3) };
    while let Some(request) = read_request(&mut input, &mut session, Some(0))? {
        objc2::rc::autoreleasepool(|_| -> Result<()> {
            // Read the cap on every request, so saving zero takes effect immediately.
            let ttl = requested_ttl.min(catalog.authentication_ttl()?);
            let reason = format!("provide a private key from wallet \"{}\" to your script\nCurve: {}\nPath: {}", request.name, request.curve.name(), request.path);
            let context = session.context(&reason, std::time::Duration::from_secs(ttl), std::time::Instant::now());
            let wallet = vault.read(&request.name, context);
            session.complete(wallet.is_ok(), std::time::Instant::now());
            let pair = model::derive(&wallet?, &request, None)?;
            // Only the selected key crosses the inherited private pipe, never stdout.
            secret_pipe.write_all(&*pair.key).map_err(|_| Failure::Transport)?;
            emit(&serde_json::json!({"name":request.name,"path":request.path,"curve":request.curve.name(),"publicKey":pair.public_key}))
        })?;
    }
    Ok(())
}
fn main_impl() -> Result<()> {
    unsafe {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::setrlimit(libc::RLIMIT_CORE, &limit) != 0 {
            return Err(Failure::Configuration);
        }
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vault = macos::Vault::new()?;
    if args == ["check-signing"] {
        return emit(
            &serde_json::json!({"status":"SIGNING_CONFIGURATION_OK","accessGroup":vault.group}),
        );
    }
    if args.is_empty() {
        return ui::run(vault, None, None);
    }
    if args.len() == 2 && args[0] == "--bridge" {
        return bridge(&vault, args[1].parse().map_err(|_| Failure::Input)?);
    }
    if args == ["list"] {
        macos::initialize_accessory()?;
        for entry in catalog::Catalog::for_group(&vault.group)?.load()? {
            emit(&serde_json::json!({"name":entry.name}))?;
        }
        return Ok(());
    }
    if args.len() == 2 && args[0] == "delete" {
        model::validate_name(&args[1])?;
        return ui::run(vault, None, Some(args[1].clone()));
    }
    if args.len() != 4 || !matches!(args[0].as_str(), "import" | "address") {
        return Err(Failure::Input);
    }
    let request = KeyRequest {
        name: args[1].clone(),
        path: args[2].clone(),
        curve: Curve::Secp256k1,
    };
    request.validate_identifier(&args[3])?;
    if args[0] == "import" {
        return ui::run(vault, Some((request, args[3].clone())), None);
    }
    macos::initialize_accessory()?;
    let context = macos::AuthContext::new(
        &format!(
            "verify the Ethereum address for wallet \"{}\"\nPath: {}\nExpected address: {}",
            request.name, request.path, args[3]
        ),
        false,
    );
    let wallet = vault.read(&request.name, &context)?;
    let pair = model::derive(&wallet, &request, Some(&args[3]))?;
    emit(&serde_json::json!({"name":request.name,"path":request.path,"address":pair.identifier}))
}
fn main() {
    // Suppress panic payloads and backtraces, which may contain secret-bearing inputs.
    std::panic::set_hook(Box::new(|_| eprintln!("INVALID_INPUT")));
    let outcome = objc2::rc::autoreleasepool(|_| main_impl());
    if let Err(error) = outcome {
        eprintln!("{}", error.code());
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_request_is_bounded_and_does_not_accept_secret_arguments() {
        let mut session = macos::AuthenticationSession::default();
        let request = b"{\"name\":\"public-test\",\"curve\":\"ed25519\",\"path\":\"m/0'\"}\n";
        let mut input = request.as_slice();
        assert!(read_request(&mut input, &mut session, None).unwrap().is_some());
        assert!(read_request(&mut input, &mut session, None).unwrap().is_none());
        assert!(read_request(&mut request[..request.len()-1].as_ref(), &mut session, None).is_err());
        assert!(read_request(&mut vec![b' '; 2049].as_slice(), &mut session, None).is_err());
        for invalid in [b"{}\n".as_slice(), b"{\"name\":\"test\",\"curve\":\"ed25519\",\"path\":\"m/0'/1\"}\n"] {
            assert!(read_request(&mut invalid.as_ref(), &mut session, None).is_err());
        }
    }
    #[test]
    fn idle_bridge_expires_authentication_and_reads_multiple_frames() {
        use std::{os::fd::AsRawFd, time::{Duration, Instant}};
        let (mut input, mut writer) = std::os::unix::net::UnixStream::pair().unwrap();
        let fd = input.as_raw_fd();
        let mut session = macos::AuthenticationSession::default();
        session.context("Public pipe fixture", Duration::from_millis(20), Instant::now());
        session.complete(true, Instant::now());
        let sender = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            for _ in 0..2 {
                writeln!(writer, "{{\"name\":\"test\",\"curve\":\"ed25519\",\"path\":\"m/0'\"}}").unwrap();
            }
        });
        assert!(read_request(&mut input, &mut session, Some(fd)).unwrap().is_some());
        assert!(session.remaining(Instant::now()).is_none());
        assert!(read_request(&mut input, &mut session, Some(fd)).unwrap().is_some());
        assert!(read_request(&mut input, &mut session, Some(fd)).unwrap().is_none());
        sender.join().unwrap();
    }
    #[test]
    fn secret_transport_rejects_regular_files() {
        let exe = std::env::current_exe().unwrap();
        let file = std::fs::File::open(exe).unwrap();
        use std::os::fd::AsRawFd;
        assert!(!pipe_descriptor(file.as_raw_fd()));
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(pipe_descriptor(a.as_raw_fd()));
    }
}
