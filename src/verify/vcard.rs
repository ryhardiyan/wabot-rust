//! Parser vCard minimalis serta normalisasi nomor telepon untuk verifikasi kontak.

use anyhow::Result;

/// Hasil ekstraksi `FN` dan `TEL` dari sebuah vCard.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VCard {
    pub name: String,
    pub phone: String,
}

/// Membersihkan nomor: membuang semua karakter non-angka, `08...` -> `628...`,
/// dan `6208...` (format internasional dengan prefix nol) -> `628...`.
pub fn normalize_phone(raw: &str) -> String {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if let Some(rest) = digits.strip_prefix('0') {
        format!("62{rest}")
    } else if let Some(rest) = digits.strip_prefix("620") {
        format!("62{rest}")
    } else {
        digits
    }
}

/// Mengubah nomor menjadi JID WhatsApp kanonik.
///
/// Menerima format lokal Indonesia (`08...` -> `628...`) dan nomor
/// internasional apa pun (mis. `+1 415...`). Server `@lid` sengaja ditolak
/// karena LID bukan nomor telepon.
pub fn phone_jid(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if let Some((_, server)) = trimmed.split_once('@') {
        anyhow::ensure!(
            server.eq_ignore_ascii_case("s.whatsapp.net") || server.eq_ignore_ascii_case("c.us"),
            "Nomor telepon tidak valid: {raw}"
        );
    }
    let phone = normalize_phone(trimmed);
    anyhow::ensure!(
        (8..=15).contains(&phone.len()),
        "Nomor telepon tidak valid: {raw}"
    );
    Ok(format!("{phone}@s.whatsapp.net"))
}

/// Melepaskan lipatan baris vCard (baris lanjutan diawali spasi/tab).
fn unfold(card: &str) -> String {
    let mut out = String::new();
    for line in card.lines() {
        if (line.starts_with(' ') || line.starts_with('\t')) && !out.is_empty() {
            out.push_str(line.trim_start());
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
        }
    }
    out
}

/// Mengambil field `FN:` (nama lengkap) dan `TEL` (nomor) dari vCard.
/// Parameter `TEL;waid=<nomor>:...` lebih diutamakan karena paling akurat di WhatsApp.
pub fn parse(card: &str) -> VCard {
    let mut result = VCard::default();
    for line in unfold(card).lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_uppercase();
        if key == "FN" || key.starts_with("FN;") {
            if result.name.is_empty() {
                result.name = value.trim().to_string();
            }
        } else if key == "TEL" || key.starts_with("TEL;") {
            let preferred = key
                .split(';')
                .find_map(|part| part.strip_prefix("WAID="))
                .map(str::trim)
                .unwrap_or(value.trim());
            let phone = normalize_phone(preferred);
            if !phone.is_empty() && result.phone.is_empty() {
                result.phone = phone;
            }
        }
    }
    result
}

/// Menyusun vCard sederhana yang bisa disimpan oleh penerima sebagai kontak.
pub fn build(display_name: &str, phone: &str) -> String {
    let digits = normalize_phone(phone);
    let escaped = display_name
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', " ")
        .replace('\r', " ");
    let international = digits.strip_prefix("62").unwrap_or(&digits);
    format!(
        "BEGIN:VCARD\nVERSION:3.0\nFN:{escaped}\nTEL;waid={digits}:+62 {international}\nEND:VCARD"
    )
}

/// Mengekstrak nomor dari teks notifikasi yang dikirim bot ke owner
/// (format: `Nomor: 628123456789`).
pub fn phone_from_notice(text: &str) -> Option<String> {
    // Cari baris yang kuncinya `Nomor`, bukan `find("nomor:")` pada seluruh
    // teks: `push_name` pengirim bisa saja mengandung kata "nomor:".
    // Ambil baris `Nomor:` terakhir: bot selalu menaruh nomor otoritatif di
    // akhir notifikasi, sehingga `Nama:`/push_name yang berisi baris `Nomor:`
    // palsu (newline injection) tidak bisa menggeser target persetujuan.
    let token = text
        .lines()
        .rev()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("nomor")
                .then(|| value.trim())
        })?
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | ' ' | '(' | ')'))
        .take(40)
        .collect::<String>();
    let phone = normalize_phone(&token);
    (8..=15).contains(&phone.len()).then_some(phone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_waid_and_name() {
        let card = "BEGIN:VCARD\nVERSION:3.0\nFN:Rahardiyan\nTEL;waid=628123456789:+62 812-3456-789\nEND:VCARD";
        assert_eq!(
            parse(card),
            VCard {
                name: "Rahardiyan".into(),
                phone: "628123456789".into()
            }
        );
    }

    #[test]
    fn normalizes_local_and_international_numbers() {
        assert_eq!(normalize_phone("0812-3456 789"), "628123456789");
        assert_eq!(normalize_phone("+62 812 3456 789"), "628123456789");
        assert_eq!(normalize_phone("6208123456789"), "628123456789");
        assert_eq!(
            phone_jid("08123456789").unwrap(),
            "628123456789@s.whatsapp.net"
        );
        assert!(phone_jid("abc").is_err());
    }

    #[test]
    fn supports_international_numbers_and_rejects_lid() {
        assert_eq!(
            phone_jid("+1 415-555-2671").unwrap(),
            "14155552671@s.whatsapp.net"
        );
        assert!(phone_jid("123456789012@lid").is_err());
        assert_eq!(
            phone_from_notice("Nomor: +1 415 555 2671").as_deref(),
            Some("14155552671")
        );
    }

    #[test]
    fn handles_folded_lines_and_escapes() {
        let card = "FN:Rahardiyan\r\n X\nTEL;TYPE=CELL:08123456789";
        assert_eq!(parse(card).name, "RahardiyanX");
        assert_eq!(parse(card).phone, "628123456789");
        let built = build("Rahardiyan; Bot", "08123456789");
        assert!(built.contains("FN:Rahardiyan\\; Bot"));
        assert!(built.contains("TEL;waid=628123456789"));
    }

    #[test]
    fn extracts_number_from_notice() {
        let notice = "🔔 PERMINTAAN\n\nNomor: 628123456789\nSilakan simpan kontak.";
        assert_eq!(phone_from_notice(notice).as_deref(), Some("628123456789"));
        assert!(phone_from_notice("tidak ada nomor").is_none());
    }

    #[test]
    fn notice_with_nomor_in_display_name_still_parses() {
        // `push_name` dipasang mentah di baris `Nama:`; kata "nomor:" di dalamnya
        // tidak boleh menggeser ekstraksi ke baris yang salah.
        let notice = "Nama: Nomor: Budi\nNomor: 0812-3456-789";
        assert_eq!(phone_from_notice(notice).as_deref(), Some("628123456789"));
    }

    #[test]
    fn notice_ignores_injected_nomor_line_from_display_name() {
        // Jika push_name berisi newline + baris `Nomor:` palsu, nomor asli
        // (baris terakhir yang ditulis bot) tetap yang dipilih.
        let notice = "Nama: Budi\nNomor: 628999999999\nNomor: 628123456789";
        assert_eq!(phone_from_notice(notice).as_deref(), Some("628123456789"));
    }
}
