/**
 * @file Owner User Database Manager Plugin
 * @module plugins/owner/user-manage
 * @description Allows bot owner to manage database users, view stats, and toggle registration.
 * @license Apache-2.0
 */

import { userDb } from "../../lib/userDb.js";
import moment from "moment-timezone";

let handler = async (m, { command, text, isOwner }) => {
    if (!isOwner) return m.reply("❌ Perintah ini hanya dapat dijalankan oleh Owner!");

    const allUsers = userDb.getAllUsers();
    const registeredCount = allUsers.filter((u) => u.registered).length;
    const unregisteredCount = allUsers.length - registeredCount;

    if (command === "userlist" || command === "listuser") {
        let txt = `📊 *DATABASE USER BOT*\n\n`;
        txt += `👥 *Total User:* ${allUsers.length}\n`;
        txt += `✅ *Terdaftar:* ${registeredCount}\n`;
        txt += `❌ *Unregistered:* ${unregisteredCount}\n\n`;
        txt += `----------------------------------------\n`;

        allUsers.slice(0, 50).forEach((u, i) => {
            const status = u.registered ? "✅" : "❌";
            txt += `${i + 1}. ${status} *${u.name}* (${u.phone})\n`;
            txt += `   📅 Join: ${u.join?.date || "-"} ${u.join?.time || "-"}\n`;
        });

        if (allUsers.length > 50) {
            txt += `\n...dan ${allUsers.length - 50} user lainnya.`;
        }

        return m.reply(txt);
    }

    if (command === "unreguser" || command === "accuser") {
        if (!text) {
            return m.reply(`⚠️ Gunakan: .${command} <nomor_user>\nContoh: .${command} 6282323780821`);
        }

        const clean = userDb.cleanNumber(text);
        const user = userDb.getUser(clean);

        if (!user) {
            return m.reply(`❌ User dengan nomor *${clean}* tidak ditemukan di database.`);
        }

        if (command === "unreguser") {
            userDb.updateUser(clean, { registered: false });
            return m.reply(`✅ User *${user.name}* (${user.phone}) berhasil di-unregister!`);
        } else {
            const now = moment().tz("Asia/Jakarta").format("DD/MM/YYYY HH:mm:ss");
            userDb.updateUser(clean, { registered: true, registeredAt: now });
            return m.reply(`✅ User *${user.name}* (${user.phone}) berhasil disetujui / di-register manual!`);
        }
    }
};

handler.help = ["userlist", "unreguser", "accuser"];
handler.tags = ["owner"];
handler.command = /^(userlist|listuser|unreguser|accuser)$/i;
handler.owner = true;

export default handler;
