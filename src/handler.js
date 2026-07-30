/**
 * @file WhatsApp message handler and command processor
 * @module core/handler
 * @description Main command handler for Liora bot - processes incoming messages,
 * validates permissions, executes plugins, and manages command routing.
 * @license Apache-2.0
 * @author Naruya Izumi
 */

import { smsg } from "#core/smsg.js";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { userDb } from "./lib/userDb.js";

const CMD_PREFIX_RE = /^[/]/;

const safe = async (fn, fallback = undefined) => {
    try {
        return await fn();
    } catch {
        return fallback;
    }
};

const parsePrefix = (connPrefix, pluginPrefix) => {
    if (pluginPrefix) return pluginPrefix;
    if (connPrefix) return connPrefix;
    return CMD_PREFIX_RE;
};

const matchPrefix = (prefix, text) => {
    if (prefix instanceof RegExp) {
        return [[prefix.exec(text), prefix]];
    }

    if (Array.isArray(prefix)) {
        return prefix.map((p) => {
            const re =
                p instanceof RegExp ? p : new RegExp(p.replace(/[|\\{}()[\]^$+*?.]/g, "\\$&"));
            return [re.exec(text), re];
        });
    }

    if (typeof prefix === "string") {
        const escaped = prefix.replace(/[^a-zA-Z0-9_-]/g, "\\$&");
        const regex = new RegExp(`^${escaped}`, "i");
        return [[regex.exec(text), regex]];
    }

    return [[[], new RegExp()]];
};

const isCmdMatch = (cmd, rule) => {
    if (rule instanceof RegExp) return rule.test(cmd);
    if (Array.isArray(rule))
        return rule.some((r) => (r instanceof RegExp ? r.test(cmd) : r === cmd));
    if (typeof rule === "string") return rule === cmd;
    return false;
};

const getGroupMetadata = async (sock, chat) => {
    try {
        const chatData = await sock.getChat(chat);

        if (chatData?.metadata) {
            return chatData.metadata;
        }

        return chatData || {};
    } catch {
        return {};
    }
};

async function printMessage(
    m,
    sock = {
        user: {},
        decodeJid: (id) => id,
        getName: async () => "Unknown",
        logger: console,
    }
) {
    try {
        if (!m || !m.sender || !m.chat) return;

        const time = new Date().toTimeString().slice(0, 8);
        const sender = sock.decodeJid ? sock.decodeJid(m.sender) : m.sender;
        const senderNum = sender ? sender.split("@")[0] : "Unknown";
        let userName = m.pushName || (typeof m.name === "string" ? m.name : "");
        if (!userName || userName === "Unknown") {
            try {
                userName = (await sock.getName?.(sender)) || "Unknown";
            } catch {
                userName = "Unknown";
            }
        }

        const chatJid = sock.decodeJid ? sock.decodeJid(m.chat) : m.chat;
        const isGroup = chatJid?.endsWith("@g.us");
        const chatContext = isGroup ? `Group (${chatJid})` : `Private (${chatJid})`;

        const mtype = m.mtype || "unknown";
        const content = typeof m.text === "string" && m.text.trim()
            ? m.text.trim()
            : m.caption
            ? m.caption.trim()
            : `[${mtype}]`;

        console.log(`\n==================================================`);
        console.log(`📩 [${time}] PESAN MASUK`);
        console.log(`👤 User    : ${userName} (${senderNum})`);
        console.log(`💬 Chat    : ${chatContext}`);
        console.log(`📌 Type    : ${mtype}`);
        console.log(`📄 Message : ${content}`);
        console.log(`==================================================`);
    } catch {
        // Silent fail
    }
}

export async function handler(chatUpdate) {
    try {
        if (!chatUpdate) return;

        this.pushMessage(chatUpdate.messages);

        const messages = chatUpdate.messages;
        if (!messages || messages.length === 0) return;

        const m = smsg(this, messages[messages.length - 1]);
        if (!m || m.isBaileys) return;

        // Prevent bot from responding to its own messages / self number
        const botJid = this.user?.id ? (this.decodeJid ? this.decodeJid(this.user.id) : this.user.id) : "";
        const botLid = this.user?.lid ? (this.decodeJid ? this.decodeJid(this.user.lid) : this.user.lid) : "";
        const senderJid = m.sender ? (this.decodeJid ? this.decodeJid(m.sender) : m.sender) : "";

        if (m.fromMe || (botJid && senderJid === botJid) || (botLid && senderJid === botLid)) {
            return;
        }

        await safe(() => printMessage(m, this));

        const rawSenderId = m.sender ? m.sender.split("@")[0].split(":")[0] : "";
        const cleanSenderNum = userDb.cleanNumber(m.sender);
        const primaryId = userDb.resolveId(m.sender);
        const regOwners = global.config.owner.map((id) => id.toString().split("@")[0].split(":")[0]);
        const isOwner = regOwners.includes(rawSenderId) || regOwners.includes(cleanSenderNum) || regOwners.includes(primaryId);

        // Auto-save user to DB (creates user record if new, links JID and LID to avoid duplicate entries)
        const dbUser = userDb.getUser(m.sender, {
            pushName: m.pushName || m.name,
            jid: m.sender?.endsWith("@s.whatsapp.net") ? m.sender : undefined,
            lid: m.sender?.endsWith("@lid") ? m.sender : undefined,
        });

        if (isOwner && dbUser && !dbUser.registered) {
            userDb.updateUser(m.sender, { registered: true });
        }

        if (!m.fromMe && global.config.self && !isOwner) {
            return;
        }
/*
Jika pesan ini bukan (!) 
dari nomor bot itu sendiri (m.fromMe)
dan (&&) 
bot sedang diatur dalam mode private (global.config.self)
dan (&&) 
pengirimnya bukan (!) 
salah satu owner (!isOwner) 
maka abaikan pesan ini jangan direspons (return) 
> 
Mari kita bedah artinya per bagian agar lebih mudah dipahami:
 * **!m.fromMe** : Artinya *"Bukan dari saya (nomor bot)"*. Tanda seru (!) berarti "bukan" atau "tidak".
 * **global.config.self** : Artinya mengecek apakah di konfigurasi/.env kamu diatur SELF=true.
 * **!isOwner** : Artinya *"Pengirimnya bukan Owner"*.
 * **&&** : Artinya *"DAN"*. Semua kondisi di kiri dan kanannya **harus terpenuhi bersamaan** agar perintah return (berhenti) dijalankan.
### Bagaimana efeknya di dalam bot?
Efek dari kode ini tergantung pada setelan .env kamu:
#### 1. Jika di .env diset SELF=true (Mode Privat)
Maka baris kode ini akan aktif dan menyaring siapa saja yang chat:
 * Jika **orang lain/user biasa** yang chat: Karena mereka *bukan bot* (!m.fromMe bernilai benar) dan mereka *bukan owner* (!isOwner bernilai benar), maka bot akan **berhenti (return)** dan mencuekin mereka.
 * Jika **Owner** yang chat: Karena mereka Owner (!isOwner bernilai salah), bot **tidak akan berhenti** dan akan merespons chat-nya.
#### 2. Jika di .env diset SELF=false (Mode Publik)
Maka kondisi global.config.self bernilai salah (false). Karena menggunakan logika && (DAN), jika ada satu saja yang salah, maka seluruh rangkaian if tersebut dianggap gagal.
 * **Efeknya:** Bot akan melewati perintah return ini begitu saja. Artinya, **semua orang (Public) bisa mengakses dan menggunakan bot kamu dengan normal.**
*/

        let groupMetadata = {};
        let participants = [];
        let participantMap = {};
        let user = {};
        let bot = {};
        let isRAdmin = false;
        let isAdmin = false;
        let isBotAdmin = false;

        if (m.isGroup) {
            groupMetadata = await getGroupMetadata(this, m.chat);
            participants = groupMetadata?.participants || [];
            participantMap = Object.fromEntries(participants.map((p) => [p.id, p]));

            const botId = this.decodeJid(this.user.lid);
            user = participantMap[m.sender] || {};
            bot = participantMap[botId] || {};
            isRAdmin = user?.admin === "superadmin";
            isAdmin = isRAdmin || user?.admin === "admin";
            isBotAdmin = bot?.admin === "admin" || bot?.admin === "superadmin";
        }

        const __dirname = dirname(fileURLToPath(import.meta.url));
        const pluginDir = join(__dirname, "./plugins");

        for (const name in global.plugins) {
            const plugin = global.plugins[name];
            if (!plugin || plugin.disabled) continue;

            const __filename = join(pluginDir, name);

            if (typeof plugin.all === "function") {
                await safe(() =>
                    plugin.all.call(this, m, {
                        chatUpdate,
                        __dirname: pluginDir,
                        __filename,
                    })
                );
            }

            if (typeof plugin !== "function") continue;

            const prefix = parsePrefix(this.prefix, plugin.customPrefix);
            const body = typeof m.text === "string" ? m.text : "";
            const match = matchPrefix(prefix, body).find((p) => p[1]);

            let usedPrefix;
            if ((usedPrefix = (match?.[0] || "")[0])) {
                const noPrefix = body.replace(usedPrefix, "");
                const parts = noPrefix.trim().split(/\s+/);
                const [rawCmd, ...argsArr] = parts;
                const command = (rawCmd || "").toLowerCase();
                const text = parts.slice(1).join(" ");

                if (!isCmdMatch(command, plugin.command)) continue;

                m.plugin = name;

                // Check registration status for non-owners
                if (dbUser && !dbUser.registered && !isOwner) {
                    const allowedCmds = ["register", "reg", "daftar", "vcard", "cekreg"];
                    if (!allowedCmds.includes(command)) {
                        const ownerNum = process.env.OWNER_NUMBER || "082323780821";
                        await safe(() =>
                            m.reply(
                                `⚠️ *AKSES DITOLAK - BELUM TERDAFTAR*\n\n` +
                                `Nomor Anda (*${dbUser.phone}*) belum terdaftar di database bot.\n\n` +
                                `*Cara Registrasi:*\n` +
                                `1. Simpan nomor Owner (*${ownerNum}*) dengan nama *Rahardiyan*\n` +
                                `2. Kirim *Kartu Kontak (Contact VCard)* Anda ke chat ini.\n` +
                                `3. Bot akan otomatis memverifikasi & menyimpan nomor Anda ke Google Contacts.\n\n` +
                                `Atau ketik *.register* untuk instruksi lengkap.`
                            )
                        );
                        break;
                    }
                }

                const fail = plugin.fail || global.dfail;

                if (plugin.owner && !isOwner) {
                    fail("owner", m, this);
                    continue;
                }

                if (plugin.group && !m.isGroup) {
                    fail("group", m, this);
                    continue;
                }

                if (plugin.botAdmin && !isBotAdmin) {
                    fail("botAdmin", m, this);
                    continue;
                }

                if (plugin.admin && !isAdmin) {
                    fail("admin", m, this);
                    continue;
                }

                const extra = {
                    match,
                    usedPrefix,
                    noPrefix,
                    args: argsArr,
                    command,
                    text,
                    sock: this,
                    participants,
                    groupMetadata,
                    user,
                    bot,
                    isOwner,
                    isRAdmin,
                    isAdmin,
                    isBotAdmin,
                    chatUpdate,
                    __dirname: pluginDir,
                    __filename,
                };

                try {
                    await plugin.call(this, m, extra);
                } catch (e) {
                    const time = new Date().toTimeString().slice(0, 8);
                    const userName = m.pushName || m.name || "Unknown";
                    const senderNum = m.sender ? m.sender.split("@")[0] : "Unknown";
                    const errorMsg = e?.message || String(e);
                    const errorStack = e?.stack || errorMsg;

                    console.error(`\n==================================================`);
                    console.error(`❌ [${time}] ERROR SAAT MENJALANKAN FITUR: ${name}`);
                    console.error(`👤 User    : ${userName} (${senderNum})`);
                    console.error(`💬 Chat    : ${m.chat}`);
                    console.error(`⚡ Command : ${usedPrefix}${command}`);
                    console.error(`⚠️ Error   : ${errorMsg}`);
                    console.error(`📜 Stack   :\n${errorStack}`);
                    console.error(`==================================================\n`);

                    if (global.logger?.error) {
                        global.logger.error(
                            {
                                plugin: name,
                                command: `${usedPrefix}${command}`,
                                user: senderNum,
                                chat: m.chat,
                                error: errorMsg,
                                stack: errorStack,
                            },
                            `Plugin Execution Error in ${name}`
                        );
                    }

                    await safe(() =>
                        m.reply(`⚠️ *Terjadi Kesalahan pada fitur (${usedPrefix}${command}):*\n\`\`\`${errorMsg}\`\`\``)
                    );
                }

                break;
            }
        }

        await safe(() => printMessage(m, this));
    } catch {
        // Silent fail
    }
}