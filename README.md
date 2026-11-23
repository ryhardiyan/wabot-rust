# MyWa Sockets

Bot WhatsApp "Luoyisse" — ditulis penuh dalam **Rust** (Tokio + `whatsapp-rust`). Tidak ada runtime JavaScript/Node/Bun.

## Ringkasan

- Runtime async dengan `tokio`, koneksi WhatsApp via `whatsapp-rust` (penyimpanan sesi SQLite).
- Sistem plugin terdaftar saat kompilasi (`src/plugins/`): downloader, group, info, internet, maker, owner, stalk, tools, user.
- Penanganan pesan: deduplikasi, batas konkurensi, timeout per handler, izin owner/admin, dan **rate limit per pengirim**.
- Semua file database tersimpan terpusat di folder `database/`: user JSON (`USER_DB`) dengan pembaruan atomik, sesi WhatsApp SQLite (`SESSION_DB`), dan database plugin lain.
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
| `/backup` | **(owner)** salin seluruh file database ke `backups/<timestamp>/` |
| `/file` | **(owner)** baca/tulis/hapus file proyek (menolak file kredensial & sesi) |

## Variabel lingkungan

Lihat [`.env.example`](.env.example) untuk daftar lengkap. Ringkasan yang wajib diperhatikan:

| Variabel | Fungsi | Default |
| --- | --- | --- |
| `OWNERS` | Daftar nomor owner (comma-separated atau array JSON) | kosong |
| `PAIRING_NUMBER` / `PAIRING_CODE` | Nomor/kode pairing WhatsApp | kosong |
| `OWNER_NUMBER` | Nomor owner untuk alur registrasi pengguna | kosong |
| `OWNER_JID` | JID owner yang menyetujui verifikasi | `OWNER_PHONE@s.whatsapp.net` |
| `OWNER_PHONE` | Nomor yang harus ada di vCard saat verifikasi | nomor pertama `OWNER_NUMBER` |
| `REQUIRED_NAME` | Kata wajib di field `FN` vCard | `Rahardiyan` |
| `VERIFY_DB` | File SQLite berisi tabel `users` (status verifikasi) | `database/users.db` |
| `SESSION_DB` | File sesi SQLite | `database/session.db` |
| `USER_DB` | File database pengguna JSON | `database/users.json` |
| `MAX_CONCURRENT_COMMANDS` | Batas command bersamaan | `4` |
| `RATE_LIMIT_PER_MINUTE` | Batas command per pengirim per menit (`0` = mati) | `30` |
| `MAX_MEDIA_MB` | Batas unduhan media | `25` |
| `RUST_LOG` / `LOG_LEVEL` | Level log | `info` |
| `LOG_PRETTY` | `false` = log JSON | `true` |

Owner selalu dibebaskan dari rate limit.

## Verifikasi pengguna (mutual contact)

Sebelum bisa menjalankan perintah, pengguna harus terverifikasi:

1. Ketik `/register` untuk melihat petunjuk.
2. Simpan nomor `OWNER_PHONE` di ponsel dengan nama yang mengandung kata `REQUIRED_NAME`.
3. Kirim kartu kontak (vCard) tersebut ke bot. Bot memeriksa `FN` dan `TEL`, lalu membalas dengan status `pending` dan mengirim notifikasi beserta kartu kontak calon pengguna ke `OWNER_JID`.
4. Owner menyimpan kontak calon tersebut, lalu membalas notifikasi/kartu itu dengan `.acc` (boleh juga `.acc 08123456789`). Setelah disetujui, bot mengirim notifikasi berhasil ke calon pengguna dan statusnya berubah menjadi `verified`.

Status disimpan di SQLite `VERIFY_DB` (tabel `users`: `jid`, `status`, `created_at`) dan ikut tercadangkan oleh `/backup`.

## Database

Semua file database disimpan di folder `database/` dan tidak ada data yang diletakkan di dalam `src/`.
Database plugin baru otomatis ikut dicadangkan oleh `/backup`.

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
  plugins/             # 46 plugin (owner-only ditandai di metadata)
  lib/
    core/               # koneksi, sesi, store, rate_limit
    downloader.rs       # HTTP client + batas ukuran
    json_db.rs          # helper database JSON atomik
    userDb.rs           # model data pengguna
database/               # semua file database (users, session, dll)
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
