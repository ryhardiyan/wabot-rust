/**
 * @file JSON-based authentication state management for Baileys
 * @module auth/json-auth
 * @description Authentication state persistence with JSON file storage (`database.json`)
 * @license Apache-2.0
 * @author Naruya Izumi
 */

import { initAuthCreds, makeCacheableSignalKeyStore } from "@whiskeysockets/baileys";
import db from "./core.js";

export function useJSONAuthState(_dbPath, options = {}) {
    let creds = db.getCreds();
    if (!creds || typeof creds !== "object") {
        creds = initAuthCreds();
        db.setCreds(creds);
    }

    const keysGet = async (type, ids) => {
        const result = {};
        for (const id of ids) {
            const val = db.getKey(type, id);
            if (val !== undefined && val !== null) {
                result[id] = val;
            }
        }
        return result;
    };

    const keysSet = async (data) => {
        if (!data || typeof data !== "object") return;
        for (const type in data) {
            const bucket = data[type];
            for (const id in bucket) {
                db.setKey(type, id, bucket[id]);
            }
        }
    };

    const keysClear = async () => {
        db.clearKeys();
    };

    function saveCreds() {
        if (creds && typeof creds === "object") {
            db.setCreds(creds);
            return true;
        }
        return false;
    }

    const rawKeys = {
        get: keysGet,
        set: keysSet,
        clear: keysClear,
    };

    const keys = makeCacheableSignalKeyStore(rawKeys, options.logger || global.logger);

    return {
        state: { creds, keys },
        saveCreds,
        _flushNow: async () => {
            db.flush();
        },
        _dispose: async () => {
            db.flush();
        },
    };
}

export const useSQLiteAuthState = useJSONAuthState;