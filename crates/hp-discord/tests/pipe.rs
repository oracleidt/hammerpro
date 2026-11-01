use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use hp_discord::frame::{self, Frame, Opcode};
use hp_discord::{Link, Options, Privacy, Session, Status, SystemConnector, Texts, Wanted, build_activity};

fn read_frame(stream: &mut impl Read) -> Frame {
    let mut header = [0u8; frame::HEADER];
    stream.read_exact(&mut header).unwrap();
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    let mut bytes = header.to_vec();
    bytes.resize(frame::HEADER + len, 0);
    stream.read_exact(&mut bytes[frame::HEADER..]).unwrap();
    frame::decode(&bytes).unwrap().unwrap().0
}

fn serve(mut stream: impl Read + Write) -> Vec<Frame> {
    let hs = read_frame(&mut stream);
    stream.write_all(&frame::encode(Opcode::Frame, "{\"cmd\":\"DISPATCH\",\"evt\":\"READY\"}")).unwrap();
    let mut got = vec![hs];
    loop {
        let f = read_frame(&mut stream);
        let done = f.op == Opcode::Close;
        got.push(f);
        if done {
            return got;
        }
    }
}

#[cfg(windows)]
fn server(path: &str) -> std::thread::JoinHandle<Vec<Frame>> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };
    let name: Vec<u16> = std::ffi::OsStr::new(path).encode_wide().chain(Some(0)).collect();
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            4096,
            4096,
            0,
            std::ptr::null(),
        )
    };
    assert!(handle != INVALID_HANDLE_VALUE, "{}", std::io::Error::last_os_error());
    let raw = handle as usize;
    std::thread::spawn(move || {
        let handle = raw as windows_sys::Win32::Foundation::HANDLE;
        unsafe {
            ConnectNamedPipe(handle, std::ptr::null_mut());
            serve(std::fs::File::from_raw_handle(handle as _))
        }
    })
}

#[cfg(unix)]
fn server(path: &str) -> std::thread::JoinHandle<Vec<Frame>> {
    let _ = std::fs::remove_file(path);
    let listener = std::os::unix::net::UnixListener::bind(path).unwrap();
    std::thread::spawn(move || serve(listener.accept().unwrap().0))
}

#[cfg(windows)]
fn test_path() -> String {
    format!(r"\\.\pipe\hp-discord-test-{}", std::process::id())
}

#[cfg(unix)]
fn test_path() -> String {
    std::env::temp_dir().join(format!("hp-discord-test-{}", std::process::id())).to_string_lossy().into_owned()
}

#[test]
fn real_transport_round_trip() {
    let path = test_path();
    let srv = server(&path);
    let opts = Options { handshake_timeout: Duration::from_secs(5), ..Options::default() };
    let missing = PathBuf::from(format!("{path}-missing"));
    let mut s = Session::new(Box::new(SystemConnector::with_paths(vec![missing, PathBuf::from(&path)])), opts);
    let status = Status {
        map: Some("gm_city.vmf"),
        opened_at: Some(1_700_000_000),
        tool: Some("Block"),
        compiling: false,
        game_id: "gmod",
        game_name: "Garry's Mod",
        version: "1.0.0",
        project_url: "https://example.org/hammerpro",
    };
    let a = build_activity(&status, Privacy::default(), &Texts::EN);
    s.set(Wanted { client_id: "1234567890123456789".into(), activity: Some(a) });
    s.step(Instant::now());
    assert!(s.is_connected());
    assert_eq!(s.link(), &Link::Connected);
    s.close();
    let got = srv.join().unwrap();
    #[cfg(unix)]
    let _ = std::fs::remove_file(&path);
    let ops: Vec<Opcode> = got.iter().map(|f| f.op).collect();
    assert_eq!(ops, [Opcode::Handshake, Opcode::Frame, Opcode::Frame, Opcode::Close]);
    let hs: serde_json::Value = serde_json::from_str(&got[0].payload).unwrap();
    assert_eq!(hs, serde_json::json!({ "v": 1, "client_id": "1234567890123456789" }));
    let set: serde_json::Value = serde_json::from_str(&got[1].payload).unwrap();
    assert_eq!(set["cmd"], "SET_ACTIVITY");
    assert_eq!(set["nonce"], "1");
    assert_eq!(set["args"]["pid"], std::process::id());
    let act = &set["args"]["activity"];
    assert_eq!(act["details"], "Editing gm_city.vmf");
    assert_eq!(act["state"], "Block");
    assert_eq!(act["timestamps"]["start"], 1_700_000_000u64);
    assert_eq!(act["assets"]["large_image"], "hammerpro");
    assert_eq!(act["assets"]["large_text"], "Hammer Pro 1.0.0");
    assert_eq!(act["assets"]["small_image"], "gmod");
    assert_eq!(
        act["buttons"],
        serde_json::json!([{ "label": "Get Hammer Pro", "url": "https://example.org/hammerpro" }])
    );
    let clear: serde_json::Value = serde_json::from_str(&got[2].payload).unwrap();
    assert_eq!(clear["nonce"], "2");
    assert!(clear["args"].get("activity").is_none(), "{}", got[2].payload);
}

#[test]
fn discord_pipe_names() {
    let paths = SystemConnector::discord().paths().to_vec();
    assert_eq!(paths.len(), 10);
    for (i, p) in paths.iter().enumerate() {
        let name = p.to_string_lossy();
        assert!(name.ends_with(&format!("discord-ipc-{i}")), "{name}");
        #[cfg(windows)]
        assert!(name.starts_with(r"\\.\pipe\"), "{name}");
    }
}

#[test]
#[ignore = "needs the Discord desktop app running: cargo test -p hp-discord --test pipe -- --ignored"]
fn real_discord_shows_hammer_pro() {
    let id = hp_discord::application_id("").expect("the built in application id");
    let opts = Options { handshake_timeout: Duration::from_secs(5), ..Options::default() };
    let mut s = Session::new(Box::new(SystemConnector::discord()), opts);
    let status = Status {
        map: Some("hp_discord_check.vmf"),
        opened_at: Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()),
        tool: Some("Selection"),
        compiling: false,
        game_id: "gmod",
        game_name: "Garry's Mod",
        version: env!("CARGO_PKG_VERSION"),
        project_url: "",
    };
    s.set(Wanted { client_id: id.to_owned(), activity: Some(build_activity(&status, Privacy::default(), &Texts::EN)) });
    let t0 = Instant::now();
    s.step(t0);
    assert_eq!(s.link(), &Link::Connected, "handshake");
    let hold = std::env::var("HP_DISCORD_HOLD_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(3u64);
    let end = Instant::now() + Duration::from_secs(hold);
    while Instant::now() < end {
        std::thread::sleep(Duration::from_millis(250));
        s.step(Instant::now() + Duration::from_secs(5));
    }
    assert_eq!(s.link(), &Link::Connected, "Discord accepted the activity");
    println!("discord: {:?}, activity sent: {:?}", s.link(), s.sent().map(|a| &a.details));
    s.close();
}
