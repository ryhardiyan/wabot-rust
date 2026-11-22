use crate::plugins::Context;
use anyhow::{bail, Context as _, Result};
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command as ProcessCommand};
crate::plugins::plugin!(
    &["exec"],
    "owner",
    "Shell owner dengan timeout dan output terbatas",
    execute,
    owner_only = true,
    custom_prefix = Some("$ ")
);
pub(super) async fn run(mut command: ProcessCommand, seconds: u64) -> Result<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child.stdout.take().context("stdout kosong")?;
    let stderr = child.stderr.take().context("stderr kosong")?;
    async fn collect<R: tokio::io::AsyncRead + Unpin>(mut reader: R) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        let mut buf = [0; 4096];
        loop {
            let n = reader.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            if data.len() + n > 1024 * 1024 {
                bail!("Output proses melebihi 1 MiB");
            }
            data.extend_from_slice(&buf[..n]);
        }
        Ok(data)
    }
    let result = tokio::time::timeout(Duration::from_secs(seconds.clamp(1, 60)), async {
        let (out, err, status) = tokio::try_join!(collect(stdout), collect(stderr), async {
            Ok::<_, anyhow::Error>(child.wait().await?)
        })?;
        Ok::<_, anyhow::Error>(format!(
            "exit={status}\n{}{}",
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&err)
        ))
    })
    .await;
    match result {
        Ok(v) => v,
        Err(_) => {
            let _ = child.kill().await;
            bail!("Proses timeout");
        }
    }
}
async fn execute(ctx: &Context) -> Result<()> {
    let mut text = ctx.text.as_str();
    let mut seconds = 60;
    let mut cwd = None;
    let mut env = Vec::new();
    while text.starts_with("--") {
        let (flag, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
        if let Some(v) = flag.strip_prefix("--timeout=") {
            seconds = (v.parse::<u64>()? / 1000).clamp(1, 60);
        } else if let Some(v) = flag.strip_prefix("--cwd=") {
            cwd = Some(v);
        } else if let Some(v) = flag.strip_prefix("--env=") {
            let (k, v) = v.split_once('=').context("--env=KEY=VALUE")?;
            env.push((k, v));
        }
        text = rest.trim_start();
    }
    anyhow::ensure!(!text.is_empty(), "$ <command>");
    let blocked = [
        "rm -rf /",
        "rm -rf *",
        "rm --no-preserve-root -rf /",
        "mkfs.ext4",
        "dd if=",
        "chmod 777 /",
        "chown root:root /",
        "mv /",
        "cp /",
        "shutdown",
        "reboot",
        "poweroff",
        "halt",
        "kill -9 1",
    ];
    anyhow::ensure!(
        !blocked.iter().any(|prefix| text.starts_with(prefix)),
        "Command diblokir oleh pemeriksaan bawaan"
    );
    let mut command = ProcessCommand::new("sh");
    command.args(["-c", text]);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (k, v) in env {
        command.env(k, v);
    }
    let output = run(command, seconds).await?;
    if output.len() > 8000 {
        ctx.m
            .send_media(
                output.into_bytes(),
                "text/plain",
                "exec.txt",
                "Output shell",
            )
            .await
    } else {
        ctx.reply(output).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn subprocess_captures_exit_and_output() -> Result<()> {
        let mut c = ProcessCommand::new("sh");
        c.args(["-c", "printf fixture; exit 3"]);
        let out = run(c, 5).await?;
        assert!(out.contains("fixture"));
        assert!(out.contains('3'));
        Ok(())
    }
    #[tokio::test]
    async fn subprocess_output_has_hard_limit() -> Result<()> {
        let mut c = ProcessCommand::new("head");
        c.args(["-c", "1100000", "/dev/zero"]);
        assert!(run(c, 5).await.is_err());
        Ok(())
    }
    #[tokio::test]
    async fn subprocess_timeout_is_enforced() -> Result<()> {
        let mut c = ProcessCommand::new("sleep");
        c.arg("10");
        let start = std::time::Instant::now();
        assert!(run(c, 1).await.is_err());
        assert!(start.elapsed() < Duration::from_secs(5));
        Ok(())
    }
}
