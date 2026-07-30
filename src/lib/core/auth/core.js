/**
 * @file JSON-based authentication database for Baileys
 * @module database/auth
 * @description JSON file storage system for WhatsApp session persistence (`database.json`)
 * @license Apache-2.0
 * @author Naruya Izumi
 */

import { readFileSync, writeFileSync, existsSync, mkdirSync, renameSync } from "node:fs";
import { dirname } from "node:path";
import { BufferJSON } from "@whiskeysockets/baileys";
import { DEFAULT_DB } from "./config.js";

export class JsonAuthDatabase {
    constructor(dbPath = DEFAULT_DB) {
        this.dbPath = dbPath;
        this.data = { creds: null, keys: {} };
        this.saveTimer = null;
        this.dirty = false;
        this._load();
    }

    _load() {
        try {
            if (existsSync(this.dbPath)) {
                const raw = readFileSync(this.dbPath, "utf-8");
                if (raw.trim()) {
                    const parsed = JSON.parse(raw, BufferJSON.reviver);
                    this.data = {
                        creds: parsed.creds || null,
                        keys: parsed.keys || {},
                    };
                }
            }
        } catch (e) {
            console.error("Failed to load JSON auth database:", e.message);
            this.data = { creds: null, keys: {} };
        }
    }

    save() {
        try {
            mkdirSync(dirname(this.dbPath), { recursive: true });
            const jsonStr = JSON.stringify(this.data, BufferJSON.replacer, 2);
            const tmpPath = `${this.dbPath}.tmp`;
            writeFileSync(tmpPath, jsonStr, "utf-8");
            renameSync(tmpPath, this.dbPath);
            this.dirty = false;
        } catch (e) {
            console.error("Failed to save JSON auth database:", e.message);
        }
    }

    scheduleSave(delay = 100) {
        this.dirty = true;
        if (!this.saveTimer) {
            this.saveTimer = setTimeout(() => {
                this.saveTimer = null;
                this.save();
            }, delay);
        }
    }

    getCreds() {
        return this.data.creds;
    }

    setCreds(creds) {
        this.data.creds = creds;
        this.save();
    }

    getKey(type, id) {
        const key = `${type}-${id}`;
        return this.data.keys[key];
    }

    setKey(type, id, value) {
        const key = `${type}-${id}`;
        if (value === null || value === undefined) {
            delete this.data.keys[key];
        } else {
            this.data.keys[key] = value;
        }
        this.scheduleSave(100);
    }

    clearKeys() {
        this.data.keys = {};
        this.save();
    }

    flush() {
        if (this.saveTimer) {
            clearTimeout(this.saveTimer);
            this.saveTimer = null;
        }
        this.save();
    }
}

let dbInstance = null;

export function getAuthDatabase(dbPath = DEFAULT_DB) {
    if (!dbInstance) {
        dbInstance = new JsonAuthDatabase(dbPath);
    }
    return dbInstance;
}

export default getAuthDatabase();