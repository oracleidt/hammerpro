use hp_sniffer::SnifferSettings;
use hp_sniffer::session::SnifferHandle;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut s = SnifferSettings {
        authorized: true,
        address: args.get(1).cloned().unwrap_or_default(),
        output_dir: args.get(2).cloned().unwrap_or_default(),
        ..SnifferSettings::default()
    };
    for a in args.iter().skip(3) {
        if let Some((k, v)) = a.split_once('=') {
            let value = match v {
                "true" => hp_sniffer::SettingValue::Bool(true),
                "false" => hp_sniffer::SettingValue::Bool(false),
                _ => hp_sniffer::SettingValue::Text(v.to_owned()),
            };
            if let Err(e) = s.set_internal(k, &value) {
                eprintln!("{e}");
            }
        }
    }
    let h = SnifferHandle::start(s, None).expect("start");
    let mut from = 0;
    loop {
        let (lines, next) = h.log_since(from);
        for l in lines {
            println!("[{:7.1}] {:?} {}", l.seconds, l.level, l.text);
        }
        from = next;
        if h.is_finished() {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    h.join();
    match h.outcome() {
        Some(Ok(o)) => println!("{}", o.report.render_text()),
        Some(Err(e)) => {
            eprintln!("failed: {e}");
            std::process::exit(1);
        }
        None => {}
    }
}
