use super::*;

#[tokio::test]
async fn a_progressing_frame_can_exceed_one_payload_idle_timeout() {
    let server = Endpoint::builder(presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .clear_address_lookup()
        .alpns(vec![protocol::ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let client = Endpoint::builder(presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .clear_address_lookup()
        .bind()
        .await
        .unwrap();
    let port = server.addr().ip_addrs().find(|address| address.is_ipv4()).unwrap().port();
    let address = EndpointAddr::new(server.id()).with_ip_addr(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
    let sender = async {
        let connection = client.connect(address, protocol::ALPN).await.unwrap();
        let (mut send, _) = connection.open_bi().await.unwrap();
        let bytes = serde_json::to_vec(&Wire::Ping(97)).unwrap();
        send.write_all(&(bytes.len() as u32).to_be_bytes()).await.unwrap();
        for byte in bytes {
            send.write_all(&[byte]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        send.finish().unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), send.stopped()).await;
    };
    let receiver = async {
        let connection = server.accept().await.unwrap().await.unwrap();
        let (_, mut recv) = connection.accept_bi().await.unwrap();
        let start = Instant::now();
        let message =
            read_with_timeouts(&mut recv, 1024, Duration::from_secs(2), Duration::from_millis(200)).await.unwrap();
        assert!(matches!(message.0, Wire::Ping(97)));
        assert!(start.elapsed() > Duration::from_millis(200));
        assert_eq!(recv.read(&mut [0; 1]).await.unwrap(), None);
    };
    tokio::join!(sender, receiver);
    client.close().await;
    server.close().await;
}

#[test]
fn editor_updates_coalesce_without_displacing_ordered_map_changes() {
    let (sender, receiver) = mpsc::sync_channel(2);
    let latest = Arc::new(Mutex::new(BTreeMap::new()));
    let editor = Editor { reliable: sender, latest: latest.clone() };
    let events = SessionEvents { reliable: receiver, latest };
    for index in 0..256 {
        event(&editor, Event::Traffic { sent: index, received: index + 1 }).unwrap();
        event(&editor, Event::Participants(Vec::new())).unwrap();
        event(&editor, Event::Bans(Vec::new())).unwrap();
        event(&editor, Event::NetworkPrivacy { relay_only: true, published_ip_count: 0, direct_path_count: 0 })
            .unwrap();
    }
    for sequence in 1..=2 {
        event(&editor, Event::Applied { author: 1, sequence, revision: sequence, changes: Vec::new() }).unwrap();
    }
    assert_eq!(editor.latest.lock().unwrap().len(), 4);
    for (event, expected) in events.try_iter().take(2).zip(1..=2) {
        assert!(matches!(event, Event::Applied { sequence, .. } if sequence == expected));
    }
    assert!(matches!(events.try_recv().unwrap(), Event::Participants(_)));
    assert!(matches!(events.try_recv().unwrap(), Event::Traffic { sent: 255, received: 256 }));
    event(&editor, Event::Closed).unwrap();
    assert!(matches!(events.try_recv().unwrap(), Event::Closed));
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn slow_initial_download_keeps_the_session_and_latest_presence() {
    let mut text = String::from("world {\"id\" \"1\"} entity {\"id\" \"2\" \"classname\" \"info_target\"");
    for index in 0..2100 {
        text.push_str(&format!("\"field{index}\" \"{}\"", "x".repeat(16_384)));
    }
    text.push('}');
    let map = Snapshot::from_vmf(&text).unwrap();
    let password = "slow-download-password";
    let host = Session::host(
        map.clone(),
        "Host".into(),
        Zeroizing::new(password.into()),
        HostOptions { room: Room { name: "Slow download".into() }, mode: ConnectionMode::Direct, port: 0 },
    )
    .unwrap();
    let started = Instant::now();
    let (invitation, address) = loop {
        assert!(started.elapsed() < DEADLINE);
        if let Ok(Event::Ready { invitation: Some(code), direct_addresses, .. }) = host.events.try_recv() {
            let port = direct_addresses.iter().find(|address| address.is_ipv4()).unwrap().port();
            break (Invitation::decode(&code).unwrap(), std::net::SocketAddr::from(([127, 0, 0, 1], port)));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .clear_address_lookup()
        .bind()
        .await
        .unwrap();
    let address = EndpointAddr::new(invitation.endpoint.parse().unwrap()).with_ip_addr(address);
    let connection = endpoint.connect(address, protocol::ALPN).await.unwrap();
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    write(&mut send, &Wire::Hello { version: protocol::VERSION }).await.unwrap();
    let (Wire::Challenge { nonce }, _) = read(&mut recv, 2048).await.unwrap() else { panic!("challenge") };
    let proof = protocol::proof(
        &invitation.credential(password),
        &nonce,
        &invitation.endpoint,
        &endpoint.id().to_string(),
        "Guest",
    );
    let identity = iroh::SecretKey::from_bytes(&[72; 32]);
    write(
        &mut send,
        &Wire::Authenticate {
            version: protocol::VERSION,
            name: "Guest".into(),
            session_name: "Slow download".into(),
            device: identity.public().to_string(),
            signature: identity.sign(&proof).to_bytes().to_vec(),
            proof,
        },
    )
    .await
    .unwrap();
    for index in 0..80 {
        host.send(Command::Presence(Presence { position: [index as f64, 0.0, 0.0], ..Default::default() })).unwrap();
        while let Ok(event) = host.events.try_recv() {
            assert!(!matches!(event, Event::Error(_) | Event::Closed));
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let (Wire::Welcome { map: received, .. }, _) = read(&mut recv, MAX_FRAME).await.unwrap() else { panic!("welcome") };
    assert_eq!(received, map);
    write(&mut send, &Wire::Ping(57)).await.unwrap();
    let (mut pong, mut presence) = (false, false);
    while !pong || !presence {
        match read(&mut recv, MAX_FRAME).await.unwrap().0 {
            Wire::Pong(57) => pong = true,
            Wire::Participants(rows) => {
                presence |= rows.iter().any(|row| row.id == 1 && row.presence.position[0] == 79.0)
            }
            _ => panic!("unexpected message"),
        }
    }
    connection.close(0u32.into(), b"Test complete");
    endpoint.close().await;
}
