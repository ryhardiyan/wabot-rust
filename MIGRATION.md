# MyWa Sockets — Rust migration

Registry saat ini: **48 plugin aktif, 110 alias command**, serta satu modul
Getcontact yang tetap nonaktif seperti implementasi lama. Ini adaptasi
fungsional, bukan klaim kesetaraan 1:1 seluruh scraper/UI lama.

## Build dan jalankan

Toolchain minimum: Rust 1.95. Paket bot memakai Edition 2021; crate WhatsApp
upstream memakai Edition 2024. `Cargo.lock` dan revisi Git upstream dikunci.

```bash
CARGO_BUILD_JOBS=2 cargo check --locked --all-targets
CARGO_BUILD_JOBS=2 cargo test --locked
CARGO_BUILD_JOBS=2 cargo test --locked -- --ignored
CARGO_BUILD_JOBS=2 cargo run --locked -- --check
CARGO_BUILD_JOBS=2 cargo run --locked
```

`--check` memvalidasi konfigurasi/registry tanpa koneksi WhatsApp, pembukaan sesi,
atau perubahan database. Jalankan dari root proyek agar `.env` dan path relatif
tetap sama. Startup normal menampilkan QR atau kode pairing dari `PAIRING_NUMBER`.
Ctrl+C/SIGTERM menghentikan runtime dan flush sesi melalui upstream.

Jika ruang disk terbatas, tambahkan `CARGO_TARGET_DIR=/path/disk-lapang` dan
`CARGO_INCREMENTAL=0`. Test berlabel ignored menjalankan pipeline ffmpeg nyata.

Untuk Termux: instal Rust terbaru, clang, pkg-config, dan ffmpeg. Validasi saat
migrasi menggunakan Linux x86_64; build Android/ARM perlu diuji pada perangkat.
`ffmpeg` dibutuhkan untuk video sticker, audio recognition, serta renderer QC.
`libwebp` dan SQLite dibangun dari dependency bawaan, tanpa OpenSSL sistem.

## Konfigurasi

Variabel lama dipertahankan: `OWNERS` (CSV atau JSON array), `PAIRING_NUMBER`,
`PAIRING_CODE`, `WATERMARK`, `AUTHOR`, `STICKPACK`, `STICKAUTH`, `THUMBNAIL_URL`,
`SELF`, `LOG_LEVEL`, `LOG_PRETTY`, `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`,
`GOOGLE_REFRESH_TOKEN`, `OWNER_NUMBER`, `GITHUB_TOKEN`, `SERVER_PORT`.
Loader memakai dotenvy terlebih dahulu dan mendukung value lama ber-spasi
tanpa quote seperti loader lama; `.env` tidak ditulis ulang. Environment proses
mengalahkan nilai file.
Jika parser kompatibilitas diperlukan, nilai dibaca literal seperti implementasi
lama (tanpa ekspansi `${VAR}` atau escape multiline).

Tambahan opsional:

| Variabel | Default / fungsi |
| --- | --- |
| `SESSION_DB` | `database/session.db` — sesi baru khusus whatsapp-rust |
| `USER_DB` | `src/database/users.json` — kompatibel bentuk JSON user lama |
| `MAX_MEDIA_MB` | `25` — batas input/output HTTP dan media; pengiriman WA memiliki plafon 25 MiB |
| `MAX_CONCURRENT_COMMANDS` | `4` — batas handler yang berjalan bersamaan |
| `WEBHOOK_ADDR` | `127.0.0.1:<SERVER_PORT atau 6104>` |
| `WEBHOOK_TOKEN` | kosong = server webhook tidak dijalankan |
| `NOTIFICATION_GROUP` | kosong = simpan callback tanpa kirim notifikasi WA |
| `GITHUB_MEDIA_REPO` | `database` |
| `PAYMENTKU_API_KEY` | wajib untuk `/depo`; token tidak di-hardcode |
| `ANABOT_API_KEY` | `freeApikey` |
| `ACRCLOUD_ACCESS_KEY`, `ACRCLOUD_ACCESS_SECRET`, `ACRCLOUD_HOST` | fallback pengenalan musik; host default identify-us-west-2.acrcloud.com |
| `MINECRAFT_SERVER` | alamat default dari implementasi lama |

Endpoint webhook tetap `GET /orkut?data=...` dan
`GET /callback?refid=...&message=...`. Semua request menyertakan
`Authorization: Bearer <WEBHOOK_TOKEN>`. Riwayat dibatasi 50 entri per file.
Notifikasi callback bukan bukti rekonsiliasi saldo; tidak membuat invoice palsu.
Untuk akses eksternal, gunakan reverse proxy HTTPS dan konfigurasi bind eksplisit.

## Adaptasi perilaku yang disengaja

- Seluruh kode kini hanya Rust; runtime tidak memanggil Node/Bun atau menggunakan
  dynamic import. Perubahan plugin membutuhkan build/restart.
- Reconnect/backoff memakai lifecycle upstream `Client::run`, bukan loop kedua
  di luar client; logout berhenti tanpa menghapus atau mereset sesi otomatis.
- Sesi Baileys tidak disalin sebagai kredensial Signal Rust. Gunakan pairing
  pertama kali dan path sesi yang berbeda; file sesi lama tidak dihapus.
- Prefix `/` dan `.` diterima. `$ ` menjalankan shell owner; `> ` atau `=> `
  mengevaluasi ekspresi **Rust** melalui rustc, bukan JavaScript di proses bot.
- Izin owner, grup, admin, bot-admin, self mode, dan registrasi diperiksa sebelum
  eksekusi. LID tidak diperlakukan sebagai nomor telepon; hanya mapping protokol
  `sender_alt` yang menautkan PN/LID.
- Handler berjalan dalam task Tokio yang dibatasi semaphore. Pesan baru saat
  kapasitas penuh dilewati tanpa membuat antrean tak terbatas. Cache deduplikasi
  memakai insert atomik; database JSON memakai write-lock sampai commit disk
  dan pembaruan memori selesai, termasuk ketika future pemanggil dibatalkan.
- Typo `onwer` di hdvideo/wink diperbaiki menjadi owner-only. Upload GitHub dan
  read-view-once juga owner-only. Cloudflare/convert menerapkan izin owner di metadata.
- Getcontact tetap tidak aktif: pada implementasi lama `handler.command` dikomentari.
  Tidak ada token hardcoded yang disalin dan tidak ada debit layanan itu.
- Plugin musik menggunakan Doreso dengan polling terbatas dan fallback ACRCloud
  HMAC-SHA1 bila dikonfigurasi; secret hardcoded lama tidak disalin.
- `/file` memakai path relatif proyek dan tidak mengirim file `.env`/sesi.
  Command shell/eval owner tetap menjalankan kode dengan hak proses bot;
  jalankan deployment sebagai user OS non-root dan gunakan daftar owner terbatas.
- Spotify/Twitter memakai API helper downloader yang sudah ada di implementasi lama, bukan
  scraper Spotmate/GraphQL X per-plugin. Kontrak provider eksternal dapat berubah.
- Pencarian/menu/detail API dikirim sebagai teks atau JSON, bukan carousel/tombol
  interaktif lama. Statistik grup berbentuk teks. QC memakai renderer teks lokal
  ffmpeg tanpa avatar/bubble identik Canvas implementasi lama.
- Konverter Clash mengirim JSON dalam file `.yaml`: JSON merupakan subset YAML 1.2.
  Konfigurasi dasarnya diperkecil dan controller diikat ke localhost.
- Kredensial Cloudflare baru disimpan di `database/cloudflare.json`; kredensial
  lama `userConfig.json`/`sharedAccess.json` tidak otomatis diimpor. Tiket share
  diselesaikan kembali saat setiap request agar masa berlaku tidak hilang.
- Alur pembayaran memantau status gateway tetapi tidak mengubah saldo lokal.
  Jangan menyamakan pesan callback dengan verifikasi pembayaran gateway.

## Batas verifikasi

Build/check/test offline menguji kompilasi API, registry, parser, izin, database
atomik, cache, batas media, stiker, dan HTTP fixture lokal. Tidak membuktikan
pairing akun riil, pengiriman WA, API provider produksi, pembayaran riil,
profil RAM produksi, atau kesetaraan UI/fitur penuh terhadap implementasi lama.

Untuk uji penerimaan: pairing akun uji; kirim `/register`, `/ping`, `/menu`;
verifikasi penolakan command owner dari akun lain; coba media kecil; uji group
admin/non-admin; restart untuk memastikan sesi persisten; lalu uji tiap provider
dengan kredensial milik deployment. Hindari menguji pembayaran dengan dana riil
sebelum alur rekonsiliasi gateway dan transaksi unik diverifikasi.
