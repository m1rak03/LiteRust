//! Strict share-link importer. Secrets never appear in error strings or UI state.
use base64::{
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
    Engine,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;
pub const MAX_IMPORT: usize = 1024 * 1024;
pub const MAX_NODES: usize = 64;
type Result<T> = std::result::Result<T, &'static str>;
#[derive(Clone, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub outbound: Value,
}
impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Node(<redacted>)")
    }
}
#[derive(Default, Serialize, Deserialize)]
pub struct Vault {
    #[serde(default)]
    pub system_scope: bool,
    pub enabled: bool,
    pub selected: usize,
    pub nodes: Vec<Node>,
}
impl Vault {
    pub fn active(&self) -> Result<&Node> {
        self.nodes.get(self.selected).ok_or("Выберите сервер VPN")
    }
}
fn decode(s: &str) -> Result<String> {
    let compact: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let s = compact.as_str();
    [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD]
        .iter()
        .find_map(|e| e.decode(s).ok())
        .and_then(|b| String::from_utf8(b).ok())
        .ok_or("Неверный Base64 в ключе")
}
fn unescape(s: &str) -> Result<String> {
    percent_encoding::percent_decode_str(s)
        .decode_utf8()
        .map(|v| v.into_owned())
        .map_err(|_| "Неверная кодировка ключа")
}
fn label(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(80).collect()
}
fn uuid(s: &str) -> Result<String> {
    if s.len() != 36
        || s.chars().enumerate().any(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c != '-'
            } else {
                !c.is_ascii_hexdigit()
            }
        })
    {
        return Err("Некорректный UUID сервера");
    }
    Ok(s.to_owned())
}
fn stream(u: &Url) -> Result<Value> {
    let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
    let get = |k: &str, default: &str| q.get(k).cloned().unwrap_or_else(|| default.to_owned());
    if get("allowInsecure", "0") == "1" || get("insecure", "0") == "1" {
        return Err("Ключ отключает проверку TLS. Используйте ключ с корректным сертификатом");
    }
    let network = get("type", "tcp");
    let security = get(
        "security",
        if u.scheme() == "trojan" {
            "tls"
        } else {
            "none"
        },
    );
    let mut s = json!({"network":network,"security":security});
    match network.as_str() {
        "tcp" | "raw" => {
            if get("headerType", "none") != "none" {
                return Err("TCP HTTP-header в этом формате не поддерживается");
            }
        }
        "ws" => {
            s["wsSettings"] =
                json!({"path":get("path","/"),"host":get("host",u.host_str().unwrap_or(""))});
        }
        "grpc" => {
            s["grpcSettings"] =
                json!({"serviceName":get("serviceName",""),"multiMode":get("mode","gun")=="multi"});
        }
        "xhttp" => {
            s["xhttpSettings"] =
                json!({"path":get("path","/"),"host":get("host",""),"mode":get("mode","auto")});
        }
        _ => return Err("Неподдерживаемый транспорт: доступны TCP, WS, gRPC и XHTTP"),
    }
    match security.as_str() {
        "none" => (),
        "tls" => {
            s["tlsSettings"] = json!({"serverName":get("sni",u.host_str().unwrap_or("")),"allowInsecure":false,"fingerprint":get("fp","chrome")});
            if let Some(alpn) = q.get("alpn") {
                s["tlsSettings"]["alpn"] = json!(alpn.split(',').collect::<Vec<_>>());
            }
        }
        "reality" => {
            let key = get("pbk", "");
            if key.is_empty() {
                return Err("В Reality-ключе отсутствует pbk");
            }
            s["realitySettings"] = json!({"serverName":get("sni",""),"fingerprint":get("fp","chrome"),"publicKey":key,"shortId":get("sid",""),"spiderX":get("spx","/")});
        }
        _ => return Err("Неизвестный режим защиты транспорта"),
    }
    Ok(s)
}
pub fn parse_link(input: &str) -> Result<Node> {
    if input.len() > 16384 {
        return Err("Ключ слишком длинный");
    }
    let input = input.trim();
    if let Some(encoded) = input.strip_prefix("vmess://") {
        let v: Value =
            serde_json::from_str(&decode(encoded)?).map_err(|_| "Некорректный VMess JSON")?;
        let text = |k: &str| v[k].as_str().unwrap_or("");
        let host = text("add");
        if host.is_empty() {
            return Err("Не задан сервер VMess");
        }
        let port = v["port"]
            .as_u64()
            .or_else(|| text("port").parse().ok())
            .filter(|n| *n > 0 && *n <= 65535)
            .ok_or("Некорректный порт")?;
        let mut transport = Url::parse("vmess://placeholder:443").unwrap();
        transport
            .set_host(Some(host))
            .map_err(|_| "Некорректный адрес VMess")?;
        {
            let mut q = transport.query_pairs_mut();
            q.append_pair(
                "type",
                if text("net").is_empty() {
                    "tcp"
                } else {
                    text("net")
                },
            );
            q.append_pair(
                "security",
                if text("tls") == "tls" { "tls" } else { "none" },
            );
            for (src, dst) in [
                ("sni", "sni"),
                ("host", "host"),
                ("path", "path"),
                ("fp", "fp"),
                ("alpn", "alpn"),
            ] {
                if !text(src).is_empty() {
                    q.append_pair(dst, text(src));
                }
            }
            if text("net") == "grpc" {
                q.append_pair("serviceName", text("path"));
            }
        }
        if !matches!(text("tls"), "" | "none" | "tls") {
            return Err("Неизвестный режим TLS в VMess");
        }
        let alter = v["aid"]
            .as_u64()
            .or_else(|| text("aid").parse().ok())
            .unwrap_or(0);
        if alter != 0 {
            return Err("Устаревший VMess alterId не поддерживается; нужен AEAD-ключ");
        }
        let cipher = if text("scy").is_empty() {
            "auto"
        } else {
            text("scy")
        };
        return Ok(Node {
            name: label(if text("ps").is_empty() {
                "VMess"
            } else {
                text("ps")
            }),
            outbound: json!({"tag":"vpn","protocol":"vmess","settings":{"vnext":[{"address":host,"port":port,"users":[{"id":uuid(text("id"))?,"security":cipher}]}]},"streamSettings":stream(&transport)?}),
        });
    }
    // SIP002 plus legacy ss://base64(method:password@server:port).
    let expanded;
    let input = if let Some(rest) = input.strip_prefix("ss://") {
        let payload = rest.split('#').next().unwrap_or("");
        if !payload.contains('@') {
            expanded = format!("ss://{}", decode(payload)?);
            expanded.as_str()
        } else {
            input
        }
    } else {
        input
    };
    let u = Url::parse(input).map_err(|_| "Некорректная ссылка сервера")?;
    let host = u.host_str().ok_or("Не задан адрес сервера")?;
    let port = u
        .port_or_known_default()
        .filter(|n| *n > 0)
        .ok_or("Не задан порт сервера")?;
    let name = label(
        &u.fragment()
            .map(unescape)
            .transpose()?
            .unwrap_or_else(|| format!("{} · {}", u.scheme(), host)),
    );
    let mut outbound = json!({"tag":"vpn"});
    match u.scheme(){
        "vless"=>{outbound["protocol"]=json!("vless");let q:std::collections::HashMap<_,_>=u.query_pairs().into_owned().collect();
            if q.get("encryption").is_some_and(|s|s!="none"){return Err("Этот режим VLESS encryption не поддерживается");}
            let flow=q.get("flow").cloned().unwrap_or_default();if !matches!(flow.as_str(),""|"xtls-rprx-vision"){return Err("Неизвестный VLESS flow");}
            outbound["settings"]=json!({"vnext":[{"address":host,"port":port,"users":[{"id":uuid(u.username())?,"encryption":"none","flow":flow}]}]});outbound["streamSettings"]=stream(&u)?;
        },
        "trojan"=>{outbound["protocol"]=json!("trojan");let password=unescape(u.username())?;if password.is_empty(){return Err("Пустой пароль Trojan");}outbound["settings"]=json!({"servers":[{"address":host,"port":port,"password":password}]});outbound["streamSettings"]=stream(&u)?;},
        "ss"=>{if u.query_pairs().any(|(k,_)|k=="plugin"){return Err("Shadowsocks plugins не поддерживаются");}
            let credentials=if let Some(pass)=u.password(){format!("{}:{}",unescape(u.username())?,unescape(pass)?)}else{decode(&unescape(u.username())?)?};
            let (method,password)=credentials.split_once(':').ok_or("Неверный ключ Shadowsocks")?;
            if !matches!(method,"aes-128-gcm"|"aes-256-gcm"|"chacha20-ietf-poly1305"|"2022-blake3-aes-128-gcm"|"2022-blake3-aes-256-gcm"|"2022-blake3-chacha20-poly1305"){return Err("Неподдерживаемый шифр Shadowsocks");}
            if password.is_empty(){return Err("Пустой пароль Shadowsocks");}
            outbound["protocol"]=json!("shadowsocks");outbound["settings"]=json!({"servers":[{"address":host,"port":port,"method":method,"password":password}]});
        },
        "socks"|"socks5"|"http"|"https"=>{outbound["protocol"]=json!(if u.scheme().starts_with("http"){"http"}else{"socks"});let mut server=json!({"address":host,"port":port});
            if !u.username().is_empty(){server["users"]=json!([{"user":unescape(u.username())?,"pass":unescape(u.password().unwrap_or(""))?}]);}
            outbound["settings"]=json!({"servers":[server]});
            if u.scheme()=="https"{outbound["streamSettings"]=json!({"security":"tls","tlsSettings":{"serverName":host,"allowInsecure":false}});}
        },
        _=>return Err("Формат не поддерживается. Доступны VLESS, VMess, Trojan, Shadowsocks, SOCKS5 и HTTP(S). Happ-зашифрованные ссылки, Hysteria2, TUIC и WireGuard пока не поддерживаются"),
    }
    Ok(Node { name, outbound })
}
pub fn parse_list(text: &str) -> Result<Vec<Node>> {
    if text.len() > MAX_IMPORT {
        return Err("Подписка превышает 1 MiB");
    }
    let decoded;
    let text = if !text.contains("://") {
        decoded = decode(text)?;
        decoded.as_str()
    } else {
        text
    };
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect();
    if lines.is_empty() || lines.len() > MAX_NODES {
        return Err("Нужно от 1 до 64 серверов");
    }
    lines.into_iter().map(parse_link).collect()
}
#[derive(Debug, Default, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    #[default]
    Auto,
    Keys,
    Subscription,
}
pub fn import(text: &str, proxy: Option<u16>) -> Result<Vec<Node>> {
    import_with_kind(text, proxy, ImportKind::Auto)
}
pub fn import_with_kind(text: &str, proxy: Option<u16>, kind: ImportKind) -> Result<Vec<Node>> {
    let text = text.trim();
    let unwrapped;
    let text = if let Some(inner) = text.strip_prefix("happ://add/") {
        unwrapped = if inner.contains("://") {
            inner.to_owned()
        } else {
            unescape(inner)?
        };
        unwrapped.as_str()
    } else {
        text
    };
    if kind == ImportKind::Keys {
        return parse_list(text);
    }
    let subscription = Url::parse(text).ok().filter(|u| {
        u.scheme() == "https"
            && u.username().is_empty()
            && (kind == ImportKind::Subscription || u.port().is_none() || u.path() != "/")
    });
    if kind == ImportKind::Subscription && subscription.is_none() {
        return Err("Подписка должна быть HTTPS-ссылкой без встроенного логина");
    }
    if let Some(mut url) = subscription {
        let mut builder = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(20))
            .redirects(0);
        if let Some(port) = proxy {
            builder = builder.proxy(
                ureq::Proxy::new(format!("http://127.0.0.1:{port}"))
                    .map_err(|_| "Ошибка локального прокси")?,
            );
        }
        let agent = builder.build();
        for _ in 0..5 {
            let response = agent
                .get(url.as_str())
                .call()
                .map_err(|_| "Не удалось загрузить подписку по HTTPS")?;
            if matches!(response.status(), 301 | 302 | 303 | 307 | 308) {
                url = url
                    .join(
                        response
                            .header("Location")
                            .ok_or("Некорректное перенаправление подписки")?,
                    )
                    .map_err(|_| "Некорректное перенаправление")?;
                if url.scheme() != "https" || !url.username().is_empty() {
                    return Err("Подписка перенаправляет на небезопасный адрес");
                }
                continue;
            }
            use std::io::Read;
            let mut body = String::new();
            response
                .into_reader()
                .take(MAX_IMPORT as u64 + 1)
                .read_to_string(&mut body)
                .map_err(|_| "Не удалось прочитать подписку")?;
            return parse_list(&body);
        }
        return Err("Слишком много перенаправлений подписки");
    }
    parse_list(text)
}
/// Pin only the dial address; preserve TLS SNI and transport host names.
pub fn pin_address(node: &Node, ip: std::net::IpAddr) -> Node {
    let mut node = node.clone();
    let key = if node.outbound["settings"]["vnext"].is_array() {
        "vnext"
    } else {
        "servers"
    };
    let host = node.outbound["settings"][key][0]["address"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    node.outbound["settings"][key][0]["address"] = json!(ip.to_string());
    let stream = &mut node.outbound["streamSettings"];
    if stream["security"] == "tls"
        && stream["tlsSettings"]["serverName"]
            .as_str()
            .is_none_or(str::is_empty)
    {
        stream["tlsSettings"]["serverName"] = json!(host);
    }
    let path = match stream["network"].as_str().unwrap_or("tcp") {
        "ws" if stream["wsSettings"]["headers"]["Host"]
            .as_str()
            .is_none_or(str::is_empty) =>
        {
            Some(("wsSettings", "host"))
        }
        "grpc" => Some(("grpcSettings", "authority")),
        "xhttp" => Some(("xhttpSettings", "host")),
        "httpupgrade" => Some(("httpupgradeSettings", "host")),
        _ => None,
    };
    if let Some((settings, field)) = path {
        if stream[settings][field].as_str().is_none_or(str::is_empty) {
            stream[settings][field] = json!(host);
        }
    }
    node
}

pub fn config(node: &Node, port: u16) -> Value {
    json!({"log":{"loglevel":"none"},"inbounds":[{"tag":"browser","listen":"127.0.0.1","port":port,"protocol":"http","settings":{}}],"outbounds":[node.outbound.clone()]})
}
pub fn browser_args(port: u16) -> String {
    format!("--proxy-server=http://127.0.0.1:{port} --proxy-bypass-list=<-loopback> --disable-quic --force-webrtc-ip-handling-policy=disable_non_proxied_udp")
}

/// Public metadata only, never address, username, password or UUID.
pub fn description(node: &Node) -> String {
    let protocol = node.outbound["protocol"].as_str().unwrap_or("proxy");
    let transport = node.outbound["streamSettings"]["network"]
        .as_str()
        .unwrap_or("tcp");
    let security = node.outbound["streamSettings"]["security"]
        .as_str()
        .unwrap_or("proxy");
    format!("{protocol} / {transport} / {security}").to_uppercase()
}
/// Explicit user-triggered connectivity/HTTP RTT probe, not ICMP or server load.
pub fn probe(port: u16) -> Result<u128> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(12))
        .redirects(0)
        .proxy(
            ureq::Proxy::new(format!("http://127.0.0.1:{port}"))
                .map_err(|_| "Не удалось проверить прокси")?,
        )
        .build();
    let started = std::time::Instant::now();
    let response = agent
        .get("https://www.gstatic.com/generate_204")
        .call()
        .map_err(|_| "Тест HTTPS не прошёл. Проверьте ключ и доступность сервера.")?;
    if response.status() != 204 {
        return Err("Сервер проверки вернул неожиданный ответ");
    }
    Ok(started.elapsed().as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "11111111-2222-4333-8444-555555555555";
    #[test]
    fn pinning_preserves_transport_hosts() {
        for transport in ["ws", "xhttp", "grpc"] {
            let original = parse_list(&format!(
                "vless://{ID}@example.com:443?security=tls&type={transport}"
            ))
            .unwrap()
            .remove(0);
            let pinned = pin_address(&original, "192.0.2.1".parse().unwrap());
            assert_eq!(
                pinned.outbound["settings"]["vnext"][0]["address"],
                "192.0.2.1"
            );
            assert_eq!(
                pinned.outbound["streamSettings"]["tlsSettings"]["serverName"],
                "example.com"
            );
            let (field, key) = if transport == "grpc" {
                ("grpcSettings", "authority")
            } else if transport == "ws" {
                ("wsSettings", "host")
            } else {
                ("xhttpSettings", "host")
            };
            assert_eq!(pinned.outbound["streamSettings"][field][key], "example.com");
            assert_eq!(
                original.outbound["settings"]["vnext"][0]["address"],
                "example.com"
            );
        }
        let original=parse_list(&format!("vless://{ID}@example.com:443?security=tls&type=ws&host=cdn.example.org&sni=tls.example.org")).unwrap().remove(0);
        let pinned = pin_address(&original, "192.0.2.2".parse().unwrap());
        assert_eq!(
            pinned.outbound["streamSettings"]["wsSettings"]["host"],
            "cdn.example.org"
        );
        assert_eq!(
            pinned.outbound["streamSettings"]["tlsSettings"]["serverName"],
            "tls.example.org"
        );
    }
    #[test]
    fn vless_reality_and_no_direct_fallback() {
        let n=parse_link(&format!("vless://{ID}@example.com:443?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&sni=example.com&flow=xtls-rprx-vision#Test")).unwrap();
        let c = config(&n, 19080);
        assert_eq!(c["outbounds"].as_array().unwrap().len(), 1);
        assert_eq!(c["inbounds"][0]["listen"], "127.0.0.1");
        assert_eq!(
            c["outbounds"][0]["streamSettings"]["realitySettings"]["publicKey"],
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        );
        assert!(!c.to_string().contains("freedom"));
    }
    #[test]
    fn protocols_and_transports() {
        for link in [
            format!("vless://{ID}@example.com:443?security=tls&type=ws&path=%2Fsocket"),
            format!("vless://{ID}@example.com:443?security=tls&type=grpc&serviceName=test"),
            format!("vless://{ID}@example.com:443?security=tls&type=xhttp&path=%2Ftest"),
            "trojan://password@example.com:443?sni=example.com".into(),
            "socks5://user:pass@example.com:1080".into(),
            "http://user:pass@example.com:8080".into(),
            "https://user:pass@example.com:8443".into(),
            "ss://YWVzLTEyOC1nY206cGFzc3dvcmQ@example.com:8388".into(),
        ] {
            assert!(parse_link(&link).is_ok(), "fixture failed");
        }
        let vmess=format!("vmess://{}",STANDARD.encode(format!(r#"{{"v":"2","ps":"VMess","add":"example.com","port":"443","id":"{ID}","aid":"0","net":"ws","path":"/socket","tls":"tls"}}"#)));
        assert_eq!(parse_link(&vmess).unwrap().outbound["protocol"], "vmess");
    }
    #[test]
    fn rejects_insecure_unsupported_and_malformed() {
        for link in [
            "vless://bad@example.com:443",
            "trojan://password@example.com:443?allowInsecure=1",
            "ss://YWVzLTEyOC1nY206cGFzc3dvcmQ@example.com:8388?plugin=test",
            "hysteria2://secret@example.com:443",
            "file:///tmp/config",
            "vless://11111111-2222-4333-8444-555555555555@example.com:443?type=unknown",
        ] {
            assert!(parse_link(link).is_err());
        }
    }
    #[test]
    fn subscriptions_and_limits() {
        let list = "socks5://example.com:1080\nhttp://example.com:8080";
        assert_eq!(parse_list(&STANDARD.encode(list)).unwrap().len(), 2);
        assert_eq!(parse_list(list).unwrap().len(), 2);
        assert!(parse_list(&"http://example.com:8080\n".repeat(65)).is_err());
        assert!(parse_list(&"x".repeat(MAX_IMPORT + 1)).is_err());
    }
    #[test]
    fn secrets_not_in_errors_or_debug() {
        let n = parse_link("trojan://SENSITIVE_PASSWORD@example.com:443").unwrap();
        assert!(!format!("{n:?}").contains("SENSITIVE_PASSWORD"));
        let e = parse_link("unsupported://SENSITIVE_PASSWORD@example.com:443").unwrap_err();
        assert!(!e.contains("SENSITIVE_PASSWORD"));
    }
    #[test]
    fn explicit_import_kind_resolves_https_proxy_ambiguity() {
        let nodes = import_with_kind("https://example.com:443", None, ImportKind::Keys).unwrap();
        assert_eq!(nodes[0].outbound["protocol"], "http");
        assert!(
            import_with_kind("http://example.com/list", None, ImportKind::Subscription).is_err()
        );
    }
    #[test]
    fn plain_happ_links_preserve_escaped_secrets() {
        let nodes = import("happ://add/trojan://pa%40ss@example.com:443", None).unwrap();
        assert_eq!(
            nodes[0].outbound["settings"]["servers"][0]["password"],
            "pa@ss"
        );
        assert_eq!(
            import("happ://add/socks5%3A%2F%2Fexample.com%3A1080", None)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn browser_proxy_disables_bypasses() {
        let args = browser_args(12345);
        assert!(args.contains("127.0.0.1:12345"));
        assert!(args.contains("--disable-quic"));
        assert!(args.contains("disable_non_proxied_udp"));
        assert!(args.contains("<-loopback>"));
    }
}
