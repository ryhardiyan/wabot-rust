# MyWa Sockets

Bot WhatsApp "Luoyisse" — ditulis penuh dalam **Rust** (Tokio + `whatsapp-rust`). Tidak ada runtime JavaScript/Node/Bun.

## Ringkasan

- Runtime async dengan `tokio`, koneksi WhatsApp via `whatsapp-rust` (penyimpanan sesi SQLite).
- Sistem plugin terdaftar saat kompilasi (`src/plugins/`): downloader, group, info, internet, maker, owner, stalk, tools, user.
- Penanganan pesan: deduplikasi, batas konkurensi, timeout per handler, izin owner/admin, dan **rate limit per pengirim**.
- Webhook HTTP (axum) untuk notifikasi Order Kuota & callback transaksi, dilengkapi endpoint `/health`.
- Database user berbasis JSON (`USER_DB`) dengan pembaruan atomik; sesi WhatsApp di SQLite (`SESSION_DB`).
- Log via `tracing` (level diatur lewat `RUST_LOG` / `LOG_LEVEL`, format JSON dengan `LOG_PRETTY=false`).

## Persyaratan

- Rust **1.95+** (`edition = "2021"`, lihat `Cargo.toml`)
- Git
- Nomor WhatsApp yang akan dipasangkan (QR / pairing code)

## Menjalankan

```bash
cp .env.example .env
# isi .env (minimal OWNERS / PAIRING_NUMBER atau PAIRING_CODE)

cargo run -- --check   # validasi .env + cek duplikat command, tanpa koneksi WhatsApp
cargo test             # jalankan unit test
cargo run --release    # jalankan bot; ikuti QR/pairing code di terminal
```

Prefix perintah: `/` atau `.` (contoh: `/ping`). Beberapa plugin punya prefix khusus.

## Perintah yang sering dipakai

| Perintah | Keterangan |
| --- | --- |
| `/menu` | Daftar seluruh perintah beserta kategori |
| `/ping` | Status runtime, uptime, jumlah plugin |
| `/register` | Petunjuk/verifikasi registrasi pengguna |
| `/backup` | **(owner)** salin data user, sesi, dan webhook ke `backups/<timestamp>/` |
| `/file` | **(owner)** baca/tulis/hapus file proyek (menolak file kredensial & sesi) |

## Variabel lingkungan

Lihat [`.env.example`](.env.example) untuk daftar lengkap. Ringkasan yang wajib diperhatikan:

| Variabel | Fungsi | Default |
| --- | --- | --- |
| `OWNERS` | Daftar nomor owner (comma-separated atau array JSON) | kosong |
| `PAIRING_NUMBER` / `PAIRING_CODE` | Nomor/kode pairing WhatsApp | kosong |
| `OWNER_NUMBER` | Nomor owner untuk alur registrasi pengguna | kosong |
| `SESSION_DB` | File sesi SQLite | `database/session.db` |
| `USER_DB` | File database pengguna JSON | `src/database/users.json` |
| `MAX_CONCURRENT_COMMANDS` | Batas command bersamaan | `4` |
| `RATE_LIMIT_PER_MINUTE` | Batas command per pengirim per menit (`0` = mati) | `30` |
| `MAX_MEDIA_MB` | Batas unduhan media | `25` |
| `WEBHOOK_ADDR` | Alamat listen webhook | `127.0.0.1:6104` |
| `WEBHOOK_TOKEN` | Bearer token webhook; **kosong = server webhook tidak aktif** | kosong |
| `NOTIFICATION_GROUP` | JID grup tujuan notifikasi webhook | kosong |
| `RUST_LOG` / `LOG_LEVEL` | Level log | `info` |
| `LOG_PRETTY` | `false` = log JSON | `true` |

Owner selalu dibebaskan dari rate limit.

## Webhook

Server webhook hanya berjalan bila `WEBHOOK_TOKEN` diisi.

| Endpoint | Auth | Keterangan |
| --- | --- | --- |
| `GET /health` | tidak perlu | Status, versi, uptime, jumlah plugin |
| `GET /orkut?data=...` | `Authorization: Bearer <WEBHOOK_TOKEN>` | Notifikasi Order Kuota |
| `GET /callback?refid=...&message=...` | `Authorization: Bearer <WEBHOOK_TOKEN>` | Callback transaksi |

Riwayat webhook disimpan maksimal 50 entri per sumber.

## Pengujian & CI

```bash
cargo fmt --all --check
cargo test
```

GitHub Actions (`.github/workflows/ci.yml`) menjalankan `cargo fmt --check`, `cargo check --all-targets`, dan `cargo test` pada push/pull request.
GitHub Actions juga membuild biner Linux statis (musl) untuk `amd64` dan `arm64` lewat `.github/workflows/build-linux.yml`. Artefact bernama `wabot-rust-linux-amd64` dan `wabot-rust-linux-arm64`, masing-masing disertai file `.sha256`. Cache registry Cargo dan direktori build di-key per target agar build ulang lebih cepat.

## Struktur singkat

```
src/
  main.rs              # entrypoint + mode --check
  config.rs            # loader .env, logging, validasi
  handler.rs           # routing command, izin, rate limit
  plugins/             # 49 plugin (owner-only ditandai di metadata)
  lib/
    core/               # koneksi, sesi, webhook, store, rate_limit
    downloader.rs       # HTTP client + batas ukuran
    userDb.rs           # model data pengguna
database/               # sesi & riwayat webhook
backups/                # hasil /backup (diabaikan git)
```

## Keamanan

- **Jangan commit `.env`** dengan kredensial asli. Gunakan `.env.example` sebagai templat dan lakukan rotasi jika kunci pernah terekspos.
- Perintah `/file` menolak file `.env`, file sesi (`.db/.sqlite`), dan path keluar proyek.
- `/backup` hanya untuk owner; folder `backups/` diabaikan git.

## Status migrasi

Migrasi selesai: seluruh kode adalah Rust (`*.rs`), tanpa file JavaScript. Plugin didaftarkan saat kompilasi melalui `mod.rs`; perubahan plugin memerlukan build ulang.

## Lisensi

Apache-2.0 (lihat `Cargo.toml`).
