/**
 * @file User Registration Plugin
 * @module plugins/user/register
 * @description Handles user registration via Google Contacts API and contact VCard verification.
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

    return { name: name || "User", phone };
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
        `1️⃣ Simpan nomor Owner bot terlebih dahulu:\n` +
        `   👉 Nomor: *${ownerNum}*\n` +
        `   👉 Wajib dengan nama: *Rahardiyan*\n\n` +
        `2️⃣ Kirimkan *Kartu Kontak (Contact VCard)* Anda sendiri ke chat ini.\n\n` +
        `3️⃣ Bot akan otomatis memverifikasi nomor Anda dengan cara save back ke Google Contacts dan mencocokkan data.\n\n` +
        `*Catatan:* Pastikan nomor owner disimpan dengan nama *Rahardiyan* dan kontak yang Anda kirim adalah nomor Anda sendiri.`
    );
};

// Listener for contact VCard messages sent by users
handler.all = async function (m) {
    if (!m || !m.message) return;

    // Detect contactMessage or contactsArrayMessage
    const msgType = m.mtype || Object.keys(m.message)[0];
    if (msgType !== "contactMessage" && msgType !== "contactsArrayMessage") return;

    const user = userDb.getUser(m.sender, { pushName: m.pushName });
    const ownerNum = process.env.OWNER_NUMBER || "082323780821";

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
        return m.reply("❌ Gagal membaca data kontak VCard yang Anda kirim.");
    }

    const senderClean = userDb.cleanNumber(m.sender);
    const cardPhoneClean = userDb.cleanNumber(parsed.phone);

    // Verify contact number matches sender number
    if (senderClean !== cardPhoneClean) {
        return m.reply(
            `⚠️ *VERIFIKASI GAGAL!*\n\n` +
            `Nomor di kartu kontak (*${cardPhoneClean}*) tidak sesuai dengan nomor pengirim WhatsApp Anda (*${senderClean}*).\n` +
            `Silakan kirim kartu kontak Anda sendiri.`
        );
    }

    const contactName = parsed.name || m.pushName || user.name || "User";

    await m.reply("⏳ *Memproses verifikasi dengan Google Contacts API... Mohon tunggu.*");

    try {
        if (!googleContacts.hasCredentials()) {
            // Provide informative response if credentials are not configured yet
            return m.reply(
                `⚠️ *KREDENSIAL GOOGLE CONTACTS BELUM DIKONFIGURASI*\n\n` +
                `Google Contacts API memerlukan GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET, dan GOOGLE_REFRESH_TOKEN di file \`.env\`.\n\n` +
                `Silakan lengkapi konfigurasi .env untuk mengaktifkan verifikasi otomatis ke Google Contacts.`
            );
        }

        const res = await googleContacts.verifyAndSaveContact(contactName, cardPhoneClean);

        if (res.success) {
            const now = moment().tz("Asia/Jakarta").format("DD/MM/YYYY HH:mm:ss");
            userDb.updateUser(m.sender, {
                name: contactName,
                registered: true,
                registeredAt: now,
            });

            await m.reply(
                `✅ *VERIFIKASI REGISTRASI BERHASIL!*\n\n` +
                `👤 *Nama:* ${contactName}\n` +
                `📱 *Nomor:* ${cardPhoneClean}\n` +
                `📅 *Tanggal Join:* ${user.join?.date || "-"} ${user.join?.time || "-"}\n` +
                `⏰ *Waktu Verifikasi:* ${now}\n` +
                `📌 *Status:* Terdaftar (Registered)\n\n` +
                `🎉 Nomor Anda telah sukses tersimpan di Google Contacts owner dan akun bot Anda sudah aktif!`
            );
        } else {
            await m.reply("❌ Verifikasi data kontak gagal. Data kontak tidak cocok di Google Contacts API.");
        }
    } catch (err) {
        await m.reply(`❌ *Terjadi Kesalahan Verifikasi Google Contacts API:*\n\`\`\`${err.message}\`\`\``);
    }
};

handler.help = ["register", "reg", "daftar", "cekreg"];
handler.tags = ["user"];
handler.command = /^(register|reg|daftar|cekreg)$/i;

export default handler;
