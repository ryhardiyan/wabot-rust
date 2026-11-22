use anyhow::{bail, Context, Result};
use std::{io::Cursor, process::Stdio, time::Duration};
use tokio::process::Command;

pub async fn convert(bytes: Vec<u8>, video: bool, pack: String, author: String) -> Result<Vec<u8>> {
    let webp = if video {
        transcode(bytes, &["-t","6","-vf","scale=320:320:force_original_aspect_ratio=decrease,pad=320:320:(ow-iw)/2:(oh-ih)/2:color=0x00000000,fps=15","-c:v","libwebp","-loop","0","-an","-f","webp"]).await?
    } else {
        tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(64 * 1024 * 1024);
            limits.max_image_width = Some(8192);
            limits.max_image_height = Some(8192);
            reader.limits(limits);
            let image = reader.decode()?.thumbnail(320, 320).to_rgba8();
            let mut canvas = image::RgbaImage::new(320, 320);
            image::imageops::overlay(
                &mut canvas,
                &image,
                ((320 - image.width()) / 2) as i64,
                ((320 - image.height()) / 2) as i64,
            );
            let encoded = webp::Encoder::from_rgba(canvas.as_raw(), 320, 320).encode(85.0);
            Ok(encoded.to_vec())
        })
        .await??
    };
    add_exif(webp, &pack, &author)
}

pub async fn transcode(input: Vec<u8>, args: &[&str]) -> Result<Vec<u8>> {
    // File temporer seekable menangani MP4 dengan moov atom di akhir; tidak masuk shell.
    tokio::fs::create_dir_all("tmp").await?;
    let file = tempfile::Builder::new()
        .prefix("media-")
        .tempfile_in("tmp")?;
    tokio::fs::write(file.path(), input).await?;
    let mut child = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-threads", "1", "-i"])
        .arg(file.path())
        .args(args)
        .arg("pipe:1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Pasang ffmpeg untuk konversi video/audio")?;
    let stdout = child.stdout.take().context("stdout ffmpeg kosong")?;
    let stderr = child.stderr.take().context("stderr ffmpeg kosong")?;
    async fn limited<R: tokio::io::AsyncRead + Unpin>(mut r: R, max: usize) -> Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let mut out = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = r.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            if out.len() + n > max {
                bail!("Output proses melampaui batas");
            }
            out.extend_from_slice(&chunk[..n]);
        }
        Ok(out)
    }
    let output = tokio::time::timeout(Duration::from_secs(90), async {
        let (out, err, status) = tokio::try_join!(
            limited(stdout, 25 * 1024 * 1024),
            limited(stderr, 64 * 1024),
            async { Ok::<_, anyhow::Error>(child.wait().await?) }
        )?;
        if !status.success() {
            bail!("ffmpeg gagal: {}", String::from_utf8_lossy(&err));
        }
        Ok(out)
    })
    .await;
    match output {
        Ok(result) => result,
        Err(_) => {
            let _ = child.kill().await;
            bail!("ffmpeg timeout");
        }
    }
}

pub fn add_exif(mut data: Vec<u8>, pack: &str, author: &str) -> Result<Vec<u8>> {
    anyhow::ensure!(
        data.len() >= 20 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP",
        "WebP tidak valid"
    );
    let payload = serde_json::to_vec(
        &serde_json::json!({"sticker-pack-id":"MyWa-Sockets","sticker-pack-name":pack,"sticker-pack-publisher":author,"emojis":[""]}),
    )?;
    let mut exif = vec![
        0x49, 0x49, 0x2a, 0, 8, 0, 0, 0, 1, 0, 0x41, 0x57, 7, 0, 0, 0, 0, 0, 0x16, 0, 0, 0,
    ];
    exif[14..18].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    exif.extend(payload);
    // Extended header memberi flag EXIF; ukuran kanvas WebP menggunakan 24-bit LE.
    if &data[12..16] != b"VP8X" {
        let image = image::load_from_memory(&data)?;
        let mut header = Vec::from(&b"VP8X"[..]);
        header.extend(10u32.to_le_bytes());
        header.extend([0x18, 0, 0, 0]);
        header.extend_from_slice(&(image.width() - 1).to_le_bytes()[..3]);
        header.extend_from_slice(&(image.height() - 1).to_le_bytes()[..3]);
        data.splice(12..12, header);
    } else {
        anyhow::ensure!(data.len() >= 30, "VP8X header terpotong");
        data[20] |= 0x08;
    }
    data.extend(b"EXIF");
    data.extend((exif.len() as u32).to_le_bytes());
    data.extend(&exif);
    if exif.len() % 2 == 1 {
        data.push(0);
    }
    let size = u32::try_from(data.len() - 8)?;
    data[4..8].copy_from_slice(&size.to_le_bytes());
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn sticker_has_metadata_and_valid_canvas() -> Result<()> {
        let mut b = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(16, 8).write_to(&mut b, image::ImageFormat::Png)?;
        let out = convert(b.into_inner(), false, "Pack".into(), "Author".into()).await?;
        assert_eq!(image::load_from_memory(&out)?.width(), 320);
        assert!(out.windows(4).any(|s| s == b"EXIF"));
        Ok(())
    }
    #[test]
    fn truncated_webp_returns_error() {
        assert!(add_exif(b"RIFF1234WEBP".to_vec(), "p", "a").is_err());
    }
    #[tokio::test]
    #[ignore = "memerlukan ffmpeg; jalankan cargo test -- --ignored"]
    async fn ffmpeg_image_pipeline() -> Result<()> {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(16, 16).write_to(&mut png, image::ImageFormat::Png)?;
        let bytes = transcode(
            png.into_inner(),
            &["-frames:v", "1", "-c:v", "libwebp", "-f", "webp"],
        )
        .await?;
        assert_eq!(image::load_from_memory(&bytes)?.width(), 16);
        Ok(())
    }
}
