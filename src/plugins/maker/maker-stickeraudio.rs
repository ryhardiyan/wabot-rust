use crate::{
    bot_lib::{core::message, sticker},
    plugins::Context,
};

// Deskripsi (per fallback ke panel bantuan): "Ubah video/GIF menjadi stiker GIF dengan audio".
crate::plugins::plugin!(
    &["stickeraudio", "sa", "audiosticker", "stkaud"],
    "maker",
    execute
);

async fn execute(ctx: &Context) -> anyhow::Result<()> {
    let media = ctx.m.media();
    let mime = message::mime(media);
    let source = if mime.starts_with("video/") || mime == "image/gif" {
        Source::Video
    } else if mime == "image/webp" {
        Source::Sticker
    } else {
        ctx.reply("Kirim atau balas video/GIF/stiker terlebih dahulu, yaa.")
            .await?;
        return Ok(());
    };

    let raw = match ctx.m.download(ctx.app.config.max_media_bytes).await {
        Ok(raw) if !raw.is_empty() => raw,
        _ => {
            ctx.reply("Media belum bisa diambil, yaa.").await?;
            return Ok(());
        }
    };

    let audio = match sticker::extract_opus_audio(raw.clone()).await {
        Ok(audio) => audio,
        Err(err) if err.downcast_ref::<sticker::NoAudioStream>().is_some() => {
            ctx.reply("Videonya tidak memiliki audio. Pilih video yang ada suaranya, yaa.")
                .await?;
            return Ok(());
        }
        Err(_) => {
            ctx.reply("Audio belum bisa diekstrak. Pastikan videonya valid dan coba lagi, yaa.")
                .await?;
            return Ok(());
        }
    };

    let c = &ctx.app.config;
    let result = match source {
        Source::Video => {
            sticker::convert(raw, true, c.stickpack.clone(), c.stickauth.clone()).await
        }
        Source::Sticker => sticker::add_exif(raw, &c.stickpack, &c.stickauth),
    };
    let sticker_bytes = match result {
        Ok(bytes) if bytes.len() <= 1024 * 1024 => bytes,
        _ => {
            ctx.reply("Stikernya belum bisa dibuat, yaa.").await?;
            return Ok(());
        }
    };

    if ctx
        .m
        .send_audio_sticker(sticker_bytes, audio, true)
        .await
        .is_err()
    {
        ctx.reply("Stiker audio belum bisa dikirim, yaa.").await?;
    }
    Ok(())
}

enum Source {
    Video,
    Sticker,
}
