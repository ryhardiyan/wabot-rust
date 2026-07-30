/**
 * @file User Database Manager
 * @module lib/userDb
 * @description Manages user records, registration status, join timestamps, and JID/LID mapping to avoid duplicate entries.
 * @license Apache-2.0
 */

import fs from "node:fs";
import path from "node:path";
import moment from "moment-timezone";

const DB_PATH = path.resolve("./src/database/users.json");

class UserDatabase {
    constructor() {
        this.data = {
            users: {},   // Keyed by primary ID (e.g., "6282323780821")
            jidMap: {},  // Maps JID -> primary ID
            lidMap: {},  // Maps LID -> primary ID
        };
        this.load();
    }

    /**
     * Load database from file
     */
    load() {
        try {
            if (fs.existsSync(DB_PATH)) {
                const raw = fs.readFileSync(DB_PATH, "utf-8");
                const parsed = JSON.parse(raw);
                this.data = {
                    users: parsed.users || {},
                    jidMap: parsed.jidMap || {},
                    lidMap: parsed.lidMap || {},
                };
            } else {
                this.save();
            }
        } catch (e) {
            console.error("❌ Failed to load user database:", e.message);
        }
    }

    /**
     * Save database to file
     */
    save() {
        try {
            const dir = path.dirname(DB_PATH);
            if (!fs.existsSync(dir)) {
                fs.mkdirSync(dir, { recursive: true });
            }
            fs.writeFileSync(DB_PATH, JSON.stringify(this.data, null, 2), "utf-8");
        } catch (e) {
            console.error("❌ Failed to save user database:", e.message);
        }
    }

    /**
     * Clean/normalize phone numbers or identifiers
     * Converts "08..." -> "628..."
     * @param {string} input
     * @returns {string}
     */
    cleanNumber(input) {
        if (!input) return "";
        let str = String(input).trim().split("@")[0].split(":")[0];
        const digitsOnly = str.replace(/\D/g, "");
        if (digitsOnly.length >= 8) {
            let phone = digitsOnly;
            if (phone.startsWith("0")) {
                phone = "62" + phone.slice(1);
            }
            return phone;
        }
        return str;
    }

    /**
     * Resolve sender JID or LID or extra parameters to primary User ID
     * @param {string} sender
     * @param {Object} [extra]
     * @returns {string}
     */
    resolveId(sender, extra = {}) {
        if (!sender) return "";
        const clean = String(sender).trim();
        const rawId = clean.split("@")[0].split(":")[0];

        if (this.data.jidMap[clean]) return this.data.jidMap[clean];
        if (this.data.jidMap[rawId]) return this.data.jidMap[rawId];
        if (this.data.lidMap[clean]) return this.data.lidMap[clean];
        if (this.data.lidMap[rawId]) return this.data.lidMap[rawId];

        if (extra.jid) {
            const jidClean = String(extra.jid).trim();
            const jidRaw = jidClean.split("@")[0].split(":")[0];
            if (this.data.jidMap[jidClean]) return this.data.jidMap[jidClean];
            if (this.data.jidMap[jidRaw]) return this.data.jidMap[jidRaw];
            if (jidClean.endsWith("@s.whatsapp.net")) return this.cleanNumber(jidRaw);
        }

        if (extra.lid) {
            const lidClean = String(extra.lid).trim();
            const lidRaw = lidClean.split("@")[0].split(":")[0];
            if (this.data.lidMap[lidClean]) return this.data.lidMap[lidClean];
            if (this.data.lidMap[lidRaw]) return this.data.lidMap[lidRaw];
        }

        if (clean.endsWith("@s.whatsapp.net")) {
            return this.cleanNumber(rawId);
        }

        return this.cleanNumber(rawId);
    }

    /**
     * Get or auto-create a user record in DB
     * @param {string} sender - JID or LID of user
     * @param {Object} [extra] - Additional context like pushName, jid, lid
     * @returns {Object} User record
     */
    getUser(sender, extra = {}) {
        if (!sender) return null;

        const senderStr = String(sender).trim();
        const isLid = senderStr.endsWith("@lid");
        const isJid = senderStr.endsWith("@s.whatsapp.net");

        let primaryId = this.resolveId(senderStr, extra);
        if (!primaryId) {
            primaryId = this.cleanNumber(senderStr);
        }

        const now = moment().tz("Asia/Jakarta");
        const tanggal = now.format("DD/MM/YYYY");
        const jam = now.format("HH:mm:ss");

        let user = this.data.users[primaryId];

        if (!user) {
            user = {
                id: primaryId,
                jid: isJid ? senderStr : (extra.jid || `${primaryId}@s.whatsapp.net`),
                lid: isLid ? senderStr : (extra.lid || ""),
                phone: primaryId,
                name: extra.pushName || extra.name || "User",
                registered: false,
                join: {
                    date: tanggal,
                    time: jam,
                },
                registeredAt: null,
            };
            this.data.users[primaryId] = user;
        }

        // Link JID and LID to prevent double entries in DB
        if (isJid || extra.jid) {
            const jidKey = isJid ? senderStr : extra.jid;
            const jidRaw = jidKey.split("@")[0].split(":")[0];
            this.data.jidMap[jidKey] = primaryId;
            this.data.jidMap[jidRaw] = primaryId;
            if (!user.jid) user.jid = jidKey;
        }

        if (isLid || extra.lid) {
            const lidKey = isLid ? senderStr : extra.lid;
            const lidRaw = lidKey.split("@")[0].split(":")[0];
            this.data.lidMap[lidKey] = primaryId;
            this.data.lidMap[lidRaw] = primaryId;
            if (!user.lid) user.lid = lidKey;
        }

        if (extra.pushName && (!user.name || user.name === "User")) {
            user.name = extra.pushName;
        }

        this.save();
        return user;
    }

    /**
     * Update user details
     * @param {string} sender
     * @param {Object} updates
     * @returns {Object|null}
     */
    updateUser(sender, updates = {}) {
        const user = this.getUser(sender);
        if (!user) return null;

        Object.assign(user, updates);
        this.data.users[user.id] = user;
        this.save();
        return user;
    }

    /**
     * Check if user is registered
     * @param {string} sender
     * @returns {boolean}
     */
    isRegistered(sender) {
        const user = this.getUser(sender);
        return user ? !!user.registered : false;
    }

    /**
     * List all users
     * @returns {Array<Object>}
     */
    getAllUsers() {
        return Object.values(this.data.users);
    }
}

export const userDb = new UserDatabase();
export default userDb;
