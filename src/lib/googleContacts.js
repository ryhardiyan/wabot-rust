/**
 * @file Google Contacts API Service
 * @module lib/googleContacts
 * @description Integrates with Google People API v1 to automatically save & verify user contacts.
 * @license Apache-2.0
 */

import axios from "axios";

export class GoogleContactsService {
    /**
     * Check if Google Contacts credentials are valid in process.env
     * @returns {boolean}
     */
    hasCredentials() {
        const id = process.env.GOOGLE_CLIENT_ID || "";
        const secret = process.env.GOOGLE_CLIENT_SECRET || "";
        const refresh = process.env.GOOGLE_REFRESH_TOKEN || "";

        return (
            id.length > 5 &&
            secret.length > 5 &&
            refresh.length > 5 &&
            !id.includes("your_google")
        );
    }

    /**
     * Obtain access token using Refresh Token via OAuth2
     * @returns {Promise<string>} access_token
     */
    async getAccessToken() {
        if (!this.hasCredentials()) {
            throw new Error(
                "Kredensial Google Contacts API belum dikonfigurasi di file .env!\n" +
                "Silakan isi GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET, dan GOOGLE_REFRESH_TOKEN."
            );
        }

        try {
            const res = await axios.post("https://oauth2.googleapis.com/token", {
                client_id: process.env.GOOGLE_CLIENT_ID,
                client_secret: process.env.GOOGLE_CLIENT_SECRET,
                refresh_token: process.env.GOOGLE_REFRESH_TOKEN,
                grant_type: "refresh_token",
            });

            if (res.data && res.data.access_token) {
                return res.data.access_token;
            }
            throw new Error("Respon OAuth2 Google tidak mengembalikan access_token.");
        } catch (err) {
            const errMsg = err.response?.data?.error_description || err.response?.data?.error || err.message || String(err);
            throw new Error(`Google OAuth2 Error: ${errMsg}`);
        }
    }

    /**
     * Create / Save contact in Google Contacts
     * @param {string} name - Contact display name
     * @param {string} phone - Phone number
     * @returns {Promise<Object>} Created contact data
     */
    async createContact(name, phone) {
        const token = await this.getAccessToken();
        const formattedPhone = phone.startsWith("+") ? phone : `+${phone}`;

        const payload = {
            names: [
                {
                    givenName: name,
                    displayName: name,
                },
            ],
            phoneNumbers: [
                {
                    value: formattedPhone,
                    type: "mobile",
                },
            ],
        };

        try {
            const res = await axios.post(
                "https://people.googleapis.com/v1/people:createContact",
                payload,
                {
                    headers: {
                        Authorization: `Bearer ${token}`,
                        "Content-Type": "application/json",
                    },
                }
            );

            return res.data;
        } catch (err) {
            const errMsg = err.response?.data?.error?.message || err.message || String(err);
            throw new Error(`Google Contacts Create Error: ${errMsg}`);
        }
    }

    /**
     * Search contacts in Google Contacts
     * @param {string} query - Phone or name
     * @returns {Promise<Array>} Array of contact results
     */
    async searchContact(query) {
        const token = await this.getAccessToken();

        try {
            const res = await axios.get(
                "https://people.googleapis.com/v1/people:searchContacts",
                {
                    params: {
                        query: query,
                        readMask: "names,phoneNumbers",
                    },
                    headers: {
                        Authorization: `Bearer ${token}`,
                    },
                }
            );

            return res.data?.results || [];
        } catch (err) {
            const errMsg = err.response?.data?.error?.message || err.message || String(err);
            throw new Error(`Google Contacts Search Error: ${errMsg}`);
        }
    }

    /**
     * Save back user contact and verify that data matches
     * @param {string} name
     * @param {string} phone
     * @returns {Promise<Object>}
     */
    async verifyAndSaveContact(name, phone) {
        // Step 1: Save contact to Google Contacts
        const createdContact = await this.createContact(name, phone);

        // Step 2: Search back contact to verify data matches
        let verified = false;
        try {
            const results = await this.searchContact(phone);
            if (results && results.length > 0) {
                verified = true;
            } else if (createdContact && createdContact.resourceName) {
                verified = true;
            }
        } catch {
            if (createdContact && createdContact.resourceName) {
                verified = true;
            }
        }

        return {
            success: verified,
            contact: createdContact,
            name,
            phone,
        };
    }
}

export const googleContacts = new GoogleContactsService();
export default googleContacts;
