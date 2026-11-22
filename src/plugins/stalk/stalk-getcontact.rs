//! Plugin JS asli tidak memiliki handler.command aktif. Modul ini tetap tidak
//! didaftarkan agar migrasi tidak mengaktifkan layanan berbayar yang sebelumnya mati.
//! Kredensial hardcoded JS tidak disalin; aktivasi memerlukan adapter pembayaran
//! dengan identitas transaksi unik, bukan pengecekan nominal global.
pub const DISABLED_REASON: &str = "Command Getcontact nonaktif pada sumber JS asli";
