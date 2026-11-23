use crate::plugins::Context;
use serde::Deserialize;
use std::{
    fs,
    time::{Duration, Instant},
};

crate::plugins::plugin!(&["ping"], "info", execute);

#[derive(Debug, Deserialize)]
struct IpInfo {
    ip: Option<String>,
    city: Option<String>,
    region: Option<String>,
    country: Option<String>,
    connection: Option<IpConnection>,
}

#[derive(Debug, Deserialize)]
struct IpConnection {
    isp: Option<String>,
    org: Option<String>,
}

async fn provider(http: &reqwest::Client) -> (String, String, String) {
    let result = tokio::time::timeout(Duration::from_secs(4), async {
        http.get("https://ipwho.is/")
            .header("User-Agent", "wabot-rust/11")
            .send()
            .await?
            .error_for_status()?
            .json::<IpInfo>()
            .await
    })
    .await;

    let Ok(Ok(info)) = result else {
        return ("N/A".into(), "N/A".into(), "N/A".into());
    };

    let ip = info
        .ip
        .as_deref()
        .map(mask_ip)
        .unwrap_or_else(|| "N/A".into());
    let region = [
        info.city.as_deref(),
        info.region.as_deref(),
        info.country.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|s| !s.is_empty() && *s != "N/A")
    .collect::<Vec<_>>()
    .join(", ");
    let region = if region.is_empty() {
        "N/A".into()
    } else {
        region
    };
    let isp = info
        .connection
        .as_ref()
        .and_then(|c| {
            c.isp
                .as_deref()
                .filter(|s| !s.is_empty())
                .or_else(|| c.org.as_deref())
        })
        .unwrap_or("N/A")
        .to_string();

    (ip, region, isp)
}

fn mask_ip(ip: &str) -> String {
    let octets: Vec<&str> = ip.split('.').collect();
    if octets.len() != 4 {
        return "***.***.***.***".into();
    }
    let mut out = octets[0].to_string();
    for octet in &octets[1..] {
        out.push('.');
        out.push_str(&"*".repeat(octet.len().clamp(1, 3)));
    }
    out
}

fn first_line(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()?
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn proc_kib(path: &str, key: &str) -> Option<u64> {
    let source = fs::read_to_string(path).ok()?;
    source
        .lines()
        .find_map(|line| line.strip_prefix(key))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn megabytes(kib: u64) -> String {
    format!("{:.1} MB", kib as f64 / 1024.0)
}

fn gigabytes(kib: u64) -> String {
    format!("{:.1} GB", kib as f64 / 1024.0 / 1024.0)
}

fn human_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let secs = seconds % 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days} hari"));
    }
    if hours > 0 {
        parts.push(format!("{hours} jam"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes} menit"));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(format!("{secs} detik"));
    }
    parts.join(" ")
}

fn os_uptime() -> u64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
        .map(|s| s as u64)
        .unwrap_or(0)
}

fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    }
}

async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let started = Instant::now();

    let hostname = first_line("/etc/hostname")
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "unknown".into());
    let os = first_line("/proc/sys/kernel/ostype").unwrap_or_else(|| std::env::consts::OS.into());
    let ram = proc_kib("/proc/meminfo", "MemTotal:")
        .map(gigabytes)
        .unwrap_or_else(|| "N/A".into());
    let rss = proc_kib("/proc/self/status", "VmRSS:")
        .map(megabytes)
        .unwrap_or_else(|| "N/A".into());
    let (ip, region, isp) = provider(&ctx.app.http).await;
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let platform = std::env::consts::OS;
    let arch_text = arch();
    let os_uptime_text = human_duration(os_uptime());
    let bot_uptime_text = human_duration(ctx.app.started.elapsed().as_secs());
    let response_ms = started.elapsed().as_secs_f64() * 1000.0;

    let text = format!(
        "`INFO SERVER`\n\
         - Hostname  : {hostname}\n\
         - Platform  : {platform}\n\
         - OS        : {os}\n\
         - Arch      : {arch_text}\n\
         - Ram       : {ram}\n\
         \n\
         `PROVIDER INFO`\n\
         - IP     : {ip}\n\
         - Region : {region}\n\
         - ISP    : {isp}\n\
         \n\
         `RUNTIME OS`\n\
         - _{os_uptime_text}_\n\
         \n\
         `RUNTIME BOT`\n\
         - _{bot_uptime_text}_\n\
         \n\
         `RESPONSE`\n\
         - _{response_ms:.1} ms_\n\
         \n\
         `RUST MEMORY USAGE`\n\
         - _RSS : {rss}_\n\
         \n\
         *_CPU Core(s) Usage ({cores} Core CPU)_*",
    );

    ctx.reply(text).await
}
