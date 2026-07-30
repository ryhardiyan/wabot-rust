/**
 * @file WebP sticker conversion utility using Node.js child_process
 * @module lib/sticker
 * @description Converts images and videos to WhatsApp sticker format (WebP) with EXIF metadata.
 * @license Apache-2.0
 * @author Naruya Izumi
 */

import webp from "node-webpmux";
import { spawn } from "node:child_process";

export async function imageToWebp(buffer, options = {}) {
    const { quality = 90 } = options;
    return new Promise((resolve, reject) => {
        const proc = spawn("ffmpeg", [
            "-i",
            "pipe:0",
            "-vcodec",
            "libwebp",
            "-vf",
            "scale='min(320,iw)':min'(320,ih)':force_original_aspect_ratio=decrease,fps=fps=15,pad=320:320:-1:-1:color=0x00000000,split[a][b];[a]palettegen=reserve_transparent=on:transparency_color=ffffff[p];[b][p]paletteuse",
            "-lossless",
            "0",
            "-q:v",
            quality.toString(),
            "-preset",
            "default",
            "-loop",
            "0",
            "-an",
            "-vsync",
            "0",
            "-f",
            "webp",
            "pipe:1",
        ]);

        const chunks = [];
        proc.stdout.on("data", (chunk) => chunks.push(chunk));
        proc.on("error", (err) => reject(err));
        proc.on("close", (code) => {
            if (code !== 0) return reject(new Error(`ffmpeg exited with code ${code}`));
            resolve(Buffer.concat(chunks));
        });

        proc.stdin.write(buffer);
        proc.stdin.end();
    });
}

export async function videoToWebp(buffer, options = {}) {
    return new Promise((resolve, reject) => {
        const proc = spawn("ffmpeg", [
            "-i",
            "pipe:0",
            "-vcodec",
            "libwebp",
            "-vf",
            "scale='min(320,iw)':min'(320,ih)':force_original_aspect_ratio=decrease,fps=fps=15,pad=320:320:-1:-1:color=0x00000000,split[a][b];[a]palettegen=reserve_transparent=on:transparency_color=ffffff[p];[b][p]paletteuse",
            "-lossless",
            "0",
            "-compression_level",
            "6",
            "-q:v",
            "60",
            "-preset",
            "default",
            "-loop",
            "0",
            "-an",
            "-vsync",
            "0",
            "-t",
            "6",
            "-f",
            "webp",
            "pipe:1",
        ]);

        const chunks = [];
        proc.stdout.on("data", (chunk) => chunks.push(chunk));
        proc.on("error", (err) => reject(err));
        proc.on("close", (code) => {
            if (code !== 0) return reject(new Error(`ffmpeg exited with code ${code}`));
            resolve(Buffer.concat(chunks));
        });

        proc.stdin.write(buffer);
        proc.stdin.end();
    }).catch(async () => {
        const json = {
            processName: "convertMp4BufferToWebpDataUrl",
            mimetype: "video/mp4",
            dataProcess: {
                data: buffer.toString("base64"),
                mimetype: "video/mp4",
            },
            stickerMetadata: {},
            sessionInfo: {
                WA_VERSION: "2.2106.5",
                os: "Mac OS",
                browser: "Chrome",
            },
            config: {
                stickerServerEndpoint: true,
            },
        };

        const res = await fetch("https://sticker-api.openwa.dev/convertMp4BufferToWebpDataUrl", {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
            },
            body: JSON.stringify(json),
        });

        const data = await res.json();
        return Buffer.from(data.dataUrl.split(";base64,")[1], "base64");
    });
}

export async function addExif(webpBuffer, metadata = {}) {
    const img = new webp.Image();
    const json = {
        "sticker-pack-id": metadata.packId || `Luoyisse-${Date.now()}`,
        "sticker-pack-name": metadata.packName || "My Sticker",
        "sticker-pack-publisher": metadata.packPublish || "Luoyisse Bot",
        "emojis": metadata.categories || [""],
        "is-avatar-sticker": metadata.isAvatar || 0,
    };

    const exifAttr = Buffer.from([
        0x49, 0x49, 0x2a, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x41, 0x57,
        0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x16, 0x00, 0x00, 0x00,
    ]);

    const jsonBuff = Buffer.from(JSON.stringify(json), "utf-8");
    const exif = Buffer.concat([exifAttr, jsonBuff]);
    exif.writeUInt32LE(jsonBuff.length, 14);

    await img.load(webpBuffer);
    img.exif = exif;
    return await img.save(null);
}

export async function sticker(buffer, options = {}) {
    const { isVideo = false, isWebp = false } = options;

    let webpBuffer;
    if (isWebp) {
        return addExif(buffer, {
            packName: options.packname || global.config?.stickpack || "Liora",
            packPublish: options.author || global.config?.stickauth || "© Naruya Izumi",
        });
    }

    if (isVideo) {
        webpBuffer = await videoToWebp(buffer, {
            quality: options.quality || 60,
        });
    } else {
        webpBuffer = await imageToWebp(buffer, {
            quality: options.quality || 90,
        });
    }

    return await addExif(webpBuffer, {
        packName: options.packname || global.config?.stickpack || "Liora",
        packPublish: options.author || global.config?.stickauth || "© Naruya Izumi",
    });
}
