/**
 * @file User Registration Plugin
 * @module plugins/user/register
 * @description Handles user registration via Owner contact VCard verification and Google Contacts API.
 * @license Apache-2.0
 */

import { userDb } from "../../lib/userDb.js";
import { googleContacts } from "../../lib/googleContacts.js";
import moment from "moment-timezone";

/**
 * Parse VCard text to extract name and phone number
 * @param {string} vcard
 * @returns {{ name: string, phone: string }|null}
 */
function parseVCard(vcard) {
    if (!vcard || typeof vcard !== "string") return null;

    let fnMatch = vcard.match(/FN[^\n:]*:(.+)/i);
    let name = fnMatch ? fnMatch[1].trim() : "";

    let waidMatch = vcard.match(/waid=(\d+)/i);
    let telMatch = vcard.match(/TEL[^\n:]*:(.+)/i);

    let phone = "";
    if (waidMatch) {
        phone = waidMatch[1];
    } else if (telMatch) {
        phone = telMatch[1].replace(/\D/g, "");
    }

    if (phone.startsWith("0")) {
        phone = "62" + phone.slice(1);
    }

    return { name: name || "", phone };
}

let handler = async (m, { sock, command, text, isOwner }) => {
    const user = userDb.getUser(m.sender, { pushName: m.pushName });
    const ownerNum = process.env.OWNER_NUMBER || "082323780821";

    if (command === "cekreg") {
        const statusStr = user.registered ? "✅ TERDAFTAR (REGISTERED)" : "❌ BELUM TERDAFTAR (UNREGISTERED)";
        return m.reply(
            `📊 *STATUS REGISTRASI USER*\n\n` +
            `👤 *Nama:* ${user.name}\n` +
            `📱 *Nomor:* ${user.phone}\n` +
            `🆔 *JID/LID:* ${user.jid || user.lid || "-"}\n` +
            `📅 *Tanggal Join:* ${user.join?.date || "-"} jam ${user.join?.time || "-"}\n` +
            `📌 *Status:* ${statusStr}`
        );
    }

    if (user.registered) {
        return m.reply(`✅ Nomor Anda (*${user.phone}*) sudah terdaftar di database bot!`);
    }

    return m.reply(
        `📱 *INSTRUKSI REGISTRASI USER*\n\n` +
        `Untuk mendaftar dan menggunakan bot, silakan ikuti langkah berikut:\n\n` +
        `1️⃣ Simpan nomor Owner bot di kontak HP Anda:\n` +
        `   👉 Nomor: *${ownerNum}*\n` +
        `   👉 Wajib disimpan dengan nama: *Rahardiyan*\n\n` +
        `2️⃣ Bagikan / Kirimkan *Kartu Kontak (Contact VCard) Owner* (kontak Rahardiyan yang tersimpan di HP Anda) ke chat ini.\n\n` +
        `3️⃣ Bot akan memverifikasi bahwa kontak yang Anda kirim adalah nomor Owner dan tersimpan dengan nama *Rahardiyan*.\n\n` +
        `4️⃣ Setelah verifikasi berhasil, Bot akan otomatis menyimpan (*save back*) nomor HP Anda ke Google Contacts Owner.\n\n` +
        `*Catatan:* Pastikan kontak yang Anda kirim adalah nomor Owner dengan nama *Rahardiyan*.`
    );
};

// Listener for contact VCard messages sent by users
handler.all = async function (m) {
    if (!m || !m.message) return;

    // Detect contactMessage or contactsArrayMessage
    const msgType = m.mtype || Object.keys(m.message)[0];
    if (msgType !== "contactMessage" && msgType !== "contactsArrayMessage") return;

    const user = userDb.getUser(m.sender, { pushName: m.pushName });

    // If user is already registered, skip verification
    if (user.registered) return;

    const ownerNum = process.env.OWNER_NUMBER || "082323780821";
    const ownerClean = userDb.cleanNumber(ownerNum);

    // Get contact card payload
    let vcards = [];
    if (msgType === "contactMessage" && m.message.contactMessage?.vcard) {
        vcards.push(m.message.contactMessage.vcard);
    } else if (msgType === "contactsArrayMessage" && Array.isArray(m.message.contactsArrayMessage?.contacts)) {
        vcards = m.message.contactsArrayMessage.contacts.map((c) => c.vcard).filter(Boolean);
    }

    if (vcards.length === 0) return;

    const parsed = parseVCard(vcards[0]);
    if (!parsed || !parsed.phone) {
        return m.reply("❌ Gagal membaca data kartu kontak yang Anda kirim.");
    }

    const cardPhoneClean = userDb.cleanNumber(parsed.phone);

    // Step 1: Verify sent contact number matches Owner's number
    if (cardPhoneClean !== ownerClean) {
        return m.reply(
            `⚠️ *VERIFIKASI GAGAL!*\n\n` +
            `Kontak yang Anda kirim (*${cardPhoneClean}*) bukan merupakan nomor Owner bot (*${ownerClean}*).\n\n` +
            `Silakan kirim Kartu Kontak *Owner* yang tersimpan di HP Anda.`
        );
    }

    // Step 2: Verify saved contact name matches "Rahardiyan"
    const isNameMatched = /rahardiyan/i.test(parsed.name || "");
    if (!isNameMatched) {
        return m.reply(
            `⚠️ *VERIFIKASI GAGAL!*\n\n` +
            `Nama kontak Owner yang tersimpan di HP Anda saat ini adalah "*${parsed.name || "-"}*".\n` +
            `Syarat verifikasi: Kontak Owner di HP Anda WAJIB disimpan dengan nama *Rahardiyan*.\n\n` +
            `Silakan edit/ubah nama kontak Owner di kontak HP Anda menjadi *Rahardiyan*, lalu kirim ulang kartu kontaknya!`
        );
    }

    // Step 3: Save back USER'S contact to Google Contacts API
    const senderClean = userDb.cleanNumber(m.sender);
    const userName = m.pushName || user.name || `User ${senderClean}`;

    await m.reply("⏳ *Verifikasi kontak Owner berhasil! Memproses simpan kontak Anda ke Google Contacts API... Mohon tunggu.*");

    try {
        if (!googleContacts.hasCredentials()) {
            return m.reply(
                `⚠️ *KREDENSIAL GOOGLE CONTACTS BELUM DIKONFIGURASI*\n\n` +
                `Google Contacts API memerlukan GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET, dan GOOGLE_REFRESH_TOKEN di file \`.env\`.\n\n` +
                `Silakan lengkapi konfigurasi .env untuk mengaktifkan verifikasi otomatis ke Google Contacts.`
            );
        }

        const res = await googleContacts.verifyAndSaveContact(userName, senderClean);

        if (res.success) {
            const now = moment().tz("Asia/Jakarta").format("DD/MM/YYYY HH:mm:ss");
            userDb.updateUser(m.sender, {
                name: userName,
                registered: true,
                registeredAt: now,
            });

            await m.reply(
                `✅ *VERIFIKASI REGISTRASI BERHASIL!*\n\n` +
                `👤 *Nama User:* ${userName}\n` +
                `📱 *Nomor User:* ${senderClean}\n` +
                `👑 *Verifikasi Owner:* Kontak Owner (${ownerClean}) terverifikasi disimpan sebagai "*${parsed.name}*"\n` +
                `⏰ *Waktu Verifikasi:* ${now}\n` +
                `📌 *Status:* Terdaftar (Registered)\n\n` +
                `🎉 Nomor Anda telah sukses tersimpan di Google Contacts owner dan akun bot Anda sudah aktif!`
            );
        } else {
            await m.reply("❌ Verifikasi gagal. Gagal menyimpan kontak ke Google Contacts API.");
        }
    } catch (err) {
        await m.reply(`❌ *Terjadi Kesalahan Google Contacts API:*\n\`\`\`${err.message}\`\`\``);
    }
};

handler.help = ["register", "reg", "daftar", "cekreg"];
handler.tags = ["user"];
handler.command = /^(register|reg|daftar|cekreg)$/i;

export default handler;

