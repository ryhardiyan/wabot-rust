//! Quote Chat adaptation. Original developer: DyySilence, copyright 2026.
use crate::{bot_lib::sticker, plugins::Context};
crate::plugins::plugin!(
    &["qc"],
    "maker",
    "Render quote chat menjadi stiker lokal",
    execute
);
async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let text = format!(
        "{} {}",
        ctx.m.quoted.as_ref().map(|q| q.text.as_str()).unwrap_or(""),
        ctx.text
    );
    anyhow::ensure!(
        !text.trim().is_empty() && text.len() <= 1200,
        "/qc <teks> atau balas pesan"
    );
    // drawtext membaca textfile: teks pengguna tidak pernah disisipkan sebagai ekspresi filter.
    tokio::fs::create_dir_all("tmp").await?;
    let file = tempfile::Builder::new().prefix("qc-").tempfile_in("tmp")?;
    let wrapped = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .chunks(5)
        .map(|words| words.join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    tokio::fs::write(file.path(), format!("{}\n\n{}", ctx.m.push_name, wrapped)).await?;
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        512,
        512,
        image::Rgba([42, 44, 46, 255]),
    ))
    .write_to(&mut png, image::ImageFormat::Png)?;
    let filter = format!(
        "drawtext=textfile='{}':fontcolor=white:fontsize=24:x=24:y=40:line_spacing=8",
        file.path().display()
    );
    let bytes = sticker::transcode(
        png.into_inner(),
        &[
            "-vf",
            &filter,
            "-frames:v",
            "1",
            "-c:v",
            "libwebp",
            "-f",
            "webp",
        ],
    )
    .await?;
    ctx.m
        .send_media(
            sticker::add_exif(bytes, &ctx.app.config.stickpack, &ctx.app.config.stickauth)?,
            "image/webp",
            "qc.webp",
            "",
        )
        .await
}
