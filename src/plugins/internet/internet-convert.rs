use crate::plugins::Context;
use anyhow::{Context as _, Result};
use base64::Engine;
use serde_json::{json, Value};
crate::plugins::plugin!(
    &["convert", "yamlgen", "clashgen"],
    "internet",
    execute,
    owner_only = true
);
async fn execute(ctx: &Context) -> Result<()> {
    let text = ctx
        .m
        .quoted
        .as_ref()
        .map(|q| q.text.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(&ctx.text);
    let a = text.split_whitespace().collect::<Vec<_>>();
    anyhow::ensure!(a.len() == 4, "/convert config|proxies <server> y|n <link>");
    let result = convert(a[0], a[1], a[2], a[3])?;
    // JSON adalah subset YAML 1.2 yang valid; tidak memerlukan serializer YAML dinamis.
    let bytes = serde_json::to_vec_pretty(&result)?;
    ctx.m
        .send_media(
            bytes,
            "application/yaml",
            "clash-config.yaml",
            "Konfigurasi Clash YAML 1.2",
        )
        .await
}
fn convert(mode: &str, server: &str, wild: &str, link: &str) -> Result<Value> {
    anyhow::ensure!(
        matches!(mode, "config" | "proxies") && matches!(wild, "y" | "n"),
        "Mode atau wildcard tidak valid"
    );
    anyhow::ensure!(
        !server.is_empty() && !server.contains(['/', ':', '@', ' ']),
        "Server harus berupa hostname/IP"
    );
    let (mut p, host, path, network) = if let Some(encoded) = link.strip_prefix("vmess://") {
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let v: Value = serde_json::from_slice(&bytes)?;
        let name = v["ps"].as_str().unwrap_or("VMess_Proxy");
        let host = v["sni"]
            .as_str()
            .or_else(|| v["host"].as_str())
            .or_else(|| v["add"].as_str())
            .unwrap_or("")
            .to_string();
        let port = v["port"]
            .as_u64()
            .or_else(|| v["port"].as_str().and_then(|s| s.parse().ok()))
            .unwrap_or(443);
        (
            json!({"name":name,"server":server,"port":port,"type":"vmess","uuid":v["id"],"alterId":v["aid"].as_u64().unwrap_or(0),"cipher":v["cipher"].as_str().unwrap_or("auto"),"tls":v["tls"]=="tls","udp":true,"skip-cert-verify":true}),
            host,
            v["path"].as_str().unwrap_or("/").to_string(),
            v["net"].as_str().unwrap_or("tcp").to_string(),
        )
    } else {
        let u = reqwest::Url::parse(link)?;
        anyhow::ensure!(
            matches!(u.scheme(), "vless" | "trojan"),
            "Protokol akun tidak dikenal"
        );
        let q = u.query_pairs().collect::<std::collections::HashMap<_, _>>();
        let host = q
            .get("sni")
            .or_else(|| q.get("host"))
            .map(|s| s.to_string())
            .unwrap_or_else(|| u.host_str().unwrap_or("").into());
        let mut p = json!({"name":u.fragment().unwrap_or("Proxy"),"server":server,"port":u.port().unwrap_or(443),"type":u.scheme(),"tls":true,"udp":true,"skip-cert-verify":true});
        p[if u.scheme() == "trojan" {
            "password"
        } else {
            "uuid"
        }] = json!(u.username());
        (
            p,
            host,
            q.get("path")
                .map(|s| s.to_string())
                .unwrap_or_else(|| "/".into()),
            q.get("type")
                .or_else(|| q.get("network"))
                .map(|s| s.to_string())
                .unwrap_or_else(|| "tcp".into()),
        )
    };
    let host = if wild == "y" && !host.is_empty() && host.parse::<std::net::IpAddr>().is_err() {
        format!("{server}.{host}")
    } else {
        host
    };
    p["sni"] = json!(host);
    p["network"] = json!(network);
    if network == "ws" {
        p["ws-opts"] = json!({"path":path,"headers":{"Host":host}});
    }
    if network == "grpc" {
        p["grpc-opts"] = json!({"grpc-service-name":""});
    }
    if mode == "proxies" {
        return Ok(json!({"proxies":[p]}));
    }
    let name = p["name"].as_str().context("Nama proxy kosong")?.to_string();
    Ok(
        json!({"port":7890,"socks-port":7891,"redir-port":7892,"mixed-port":7893,"tproxy-port":7895,"ipv6":false,"mode":"rule","log-level":"silent","allow-lan":true,"external-controller":"127.0.0.1:9090","bind-address":"*","unified-delay":true,"profile":{"store-selected":true,"store-fake-ip":true},"dns":{"enable":true,"ipv6":false,"enhanced-mode":"fake-ip","nameserver":["8.8.8.8","1.1.1.1"],"fake-ip-range":"198.18.0.1/16"},"proxies":[p],"proxy-groups":[{"name":"PROXY","type":"select","proxies":[name,"DIRECT"]}],"rules":["MATCH,PROXY"]}),
    )
}
#[cfg(test)]
mod tests {
    #[test]
    fn vless() {
        let v = super::convert(
            "proxies",
            "new.example",
            "n",
            "vless://abc@old.example:443?type=ws&path=%2Ffoo#Test",
        )
        .expect("valid fixture");
        assert_eq!(v["proxies"][0]["server"], "new.example");
    }
}
