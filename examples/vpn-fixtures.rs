use base64::{engine::general_purpose::STANDARD, Engine};
use literust_browser::vpn;
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("output directory"));
    std::fs::create_dir_all(&dir).unwrap();
    let id = "11111111-2222-4333-8444-555555555555";
    let mut links=vec![format!("vless://{id}@example.com:443?security=tls&type=tcp"),format!("vless://{id}@example.com:443?security=tls&type=ws&path=%2Fsocket"),format!("vless://{id}@example.com:443?security=tls&type=grpc&serviceName=test"),format!("vless://{id}@example.com:443?security=tls&type=xhttp&path=%2Ftest"),format!("vless://{id}@example.com:443?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sni=example.com&flow=xtls-rprx-vision"),"trojan://password@example.com:443".into(),"ss://YWVzLTEyOC1nY206cGFzc3dvcmQ@example.com:8388".into(),"socks5://example.com:1080".into(),"http://example.com:8080".into(),"https://user:pass@example.com:8443".into()];
    links.push(format!("vmess://{}",STANDARD.encode(format!(r#"{{"add":"example.com","port":"443","id":"{id}","net":"ws","path":"/socket","tls":"tls"}}"#))));
    for (i, link) in links.iter().enumerate() {
        let n = vpn::parse_link(link).unwrap();
        std::fs::write(
            dir.join(format!("{i}.json")),
            serde_json::to_vec_pretty(&vpn::config(&n, 19080)).unwrap(),
        )
        .unwrap();
    }
    // Local-only integration target; the test creates an upstream HTTP proxy.
    let n = vpn::parse_link("http://127.0.0.1:19181").unwrap();
    std::fs::write(
        dir.join("integration.json"),
        serde_json::to_vec(&vpn::config(&n, 19180)).unwrap(),
    )
    .unwrap();
    println!(
        "Wrote {} fake-credential protocol fixtures and local integration config",
        links.len()
    );
}
