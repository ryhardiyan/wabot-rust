/**
 * @file Terminal command handler
 * @module plugins/owner/terminal
 * @license Apache-2.0
 * @author Naruya Izumi
 */

import { exec } from "node:child_process";

const blocked = [
    "rm -rf /",
    "rm -rf *",
    "rm --no-preserve-root -rf /",
    "mkfs.ext4",
    "dd if=",
    "chmod 777 /",
    "chown root:root /",
    "mv /",
    "cp /",
    "shutdown",
    "reboot",
    "poweroff",
    "halt",
    "kill -9 1",
    ">:(){ :|: & };:",
];

const handler = async (m, { sock, isOwner }) => {
    if (!isOwner) return;
    const txt = m.text || "";
    if (!txt.startsWith("$ ")) return;

    let cmd = txt.slice(2).trim();
    if (!cmd) return;

    const flags = {
        cwd: null,
        env: {},
        quiet: true,
        timeout: 60000,
    };

    const re = /^--(\w+)(?:=(.+?))?(?:\s+|$)/;
    while (re.test(cmd)) {
        const match = cmd.match(re);
        const [all, f, v] = match;

        if (f === "cwd") {
            flags.cwd = v;
        } else if (f === "env") {
            const [k, val] = v.split("=");
            flags.env[k] = val;
        } else if (f === "timeout") {
            flags.timeout = parseInt(v);
        } else if (f === "verbose") {
            flags.quiet = false;
        }

        cmd = cmd.slice(all.length);
    }

    if (blocked.some((b) => cmd.startsWith(b))) {
        return sock.sendMessage(m.chat, {
            text: ["Command blocked", `> ${cmd}`].join("\n"),
        });
    }

    let out;
    try {
        const options = {
            timeout: flags.timeout || 60000,
            env: { ...process.env, ...flags.env },
            cwd: flags.cwd || process.cwd(),
            maxBuffer: 1024 * 1024 * 10,
        };

        const { stdout, stderr } = await new Promise((resolve) => {
            exec(cmd, options, (error, stdout, stderr) => {
                resolve({
                    error,
                    stdout: stdout || "",
                    stderr: stderr || (error ? error.message : ""),
                });
            });
        });

        const output = stdout || stderr || "(no output)";
        const parts = [`${cmd}`, "─".repeat(30)];

        if (output.trim()) {
            parts.push(output.trim());
        }

        const foot = [];
        if (flags.cwd) {
            foot.push(`📁 ${flags.cwd}`);
        }

        if (foot.length > 0) {
            parts.push("", foot.join(" • "));
        }

        out = parts.join("\n");
    } catch (e) {
        out = [`${cmd}`, "─".repeat(30), `Error: ${e.message || String(e)}`, ""].join("\n");
    }

    await sock.sendMessage(m.chat, { text: out });
};

handler.customPrefix = /^\$ /;
handler.command = /(?:)/i;

export default handler;
