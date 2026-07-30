import axios from 'axios';
import FormData from 'form-data';
import { delay } from '@whiskeysockets/baileys';

/**
 * ENGINE 1: ImgLarger (Menggunakan Buffer)
 */
async function upscaleEngine1(buffer, rasio = 2) {
    const form = new FormData();
    form.append('myfile', buffer, Date.now() + '.jpg');
    form.append('scaleRadio', rasio);

    const upload = await axios.post('https://get1.imglarger.com/api/UpscalerNew/UploadNew', form, {
        headers: {
            ...form.getHeaders(),
            Origin: 'https://imgupscaler.com',
        },
    });

    if (upload.status !== 200) throw new Error('Engine 1: Gagal Upload');

    for (let i = 0; i < 15; i++) {
        const check = await axios.post('https://get1.imglarger.com/api/UpscalerNew/CheckStatusNew',
            { code: upload.data?.data?.code, scaleRadio: rasio },
            { headers: { 'Content-Type': 'application/json' } }
        );

        if (check.data?.data?.status === 'success') {
            // Ambil URL hasil download biner gambarnya
            const downloadUrl = check.data.data.downloadUrl;
            const file = await axios.get(downloadUrl, { responseType: 'arraybuffer' });
            return Buffer.from(file.data);
        }
        await delay(3000);
    }
    throw new Error('Engine 1: Timeout');
}

/**
 * ENGINE 2: iLoveIMG via Axios (Menggunakan Buffer)
 */
async function upscaleEngine2(buffer, scale = '2') {
    const html = await axios.get('https://www.iloveimg.com/upscale-image').then(r => r.data);
    const token = html.match(/"token":"(eyJ[^"]+)"/)?.[1];
    const task = html.match(/ilovepdfConfig\.taskId\s*=\s*'([^']+)'/)?.[1];
    if (!token || !task) throw new Error('Engine 2: Token tidak ditemukan');

    const form = new FormData();
    form.append('name', `${Date.now()}.jpg`);
    form.append('chunk', '0');
    form.append('chunks', '1');
    form.append('task', task);
    form.append('preview', '1');
    form.append('v', 'web.0');
    form.append('file', buffer, { filename: 'image.jpg' });

    const upload = await axios.post('https://api29g.iloveimg.com/v1/upload', form, {
        headers: {
            ...form.getHeaders(),
            Authorization: `Bearer ${token}`,
            Origin: 'https://www.iloveimg.com'
        }
    });

    const serverFilename = upload.data.server_filename;

    const processForm = new FormData();
    processForm.append('task', task);
    processForm.append('server_filename', serverFilename);
    processForm.append('scale', String(scale));

    const result = await axios.post('https://api29g.iloveimg.com/v1/upscale', processForm, {
        headers: {
            ...processForm.getHeaders(),
            Authorization: `Bearer ${token}`,
            Origin: 'https://www.iloveimg.com'
        },
        responseType: 'arraybuffer'
    });

    return Buffer.from(result.data);
}

/**
 * ENGINE 3: iLoveIMG via Fetch Native (Menggunakan Buffer)
 */
async function upscaleEngine3(buffer, scale = '2') {
    const res = await fetch('https://www.iloveimg.com/id/tingkatkan-gambar', {
        headers: { 'User-Agent': 'Mozilla/5.0' }
    });
    const html = await res.text();
    const token = html.match(/"token":"([^"]+)"/)?.[1];
    const taskId = html.match(/ilovepdfConfig\.taskId\s*=\s*'([^']+)'/)?.[1];
    if (!token || !taskId) throw new Error('Engine 3: Token gagal didapat');

    const form = new global.FormData();
    form.append('name', 'image.jpg');
    form.append('chunk', '0');
    form.append('chunks', '1');
    form.append('task', taskId);
    form.append('preview', '1');
    form.append('v', 'web.0');
    form.append('file', new Blob([buffer]), 'image.jpg');

    const uploadRes = await fetch('https://api1g.iloveimg.com/v1/upload', {
        method: 'POST',
        headers: { 'Authorization': `Bearer ${token}` },
        body: form
    });
    const uploadData = await uploadRes.json();

    const processForm = new global.FormData();
    processForm.append('packaged_filename', 'upscaled');
    processForm.append('multiplier', String(scale)); 
    processForm.append('task', taskId);
    processForm.append('tool', 'upscaleimage');
    processForm.append('files[0][server_filename]', uploadData.server_filename);
    processForm.append('files[0][filename]', 'image.jpg');

    const processRes = await fetch('https://api1g.iloveimg.com/v1/process', {
        method: 'POST',
        headers: { 'Authorization': `Bearer ${token}`, 'Origin': 'https://www.iloveimg.com' },
        body: processForm
    });
    const processData = await processRes.json();
    if (processData.status !== 'TaskSuccess') throw new Error('Engine 3: Gagal memproses');

    const fileRes = await fetch(`https://api1g.iloveimg.com/v1/download/${taskId}`);
    return Buffer.from(await fileRes.arrayBuffer());
}

let handler = async (m, { sock, args, usedPrefix, command }) => {
    // Validasi input media gambar (bisa langsung atau dari reply gambar)
    let q = m.quoted ? m.quoted : m;
    let mime = (q.msg || q).mimetype || '';
    if (!/image\/(jpe?g|png)/.test(mime)) {
        return m.reply(`*Image Upscaler (HD)*\n\n*Gunakan:* Kirim/Balas gambar dengan caption:\n${usedPrefix + command} [2|4]`);
    }

    // Ambil input rasio (default 2, maksimal 4)
    let scale = args[0] && ['2', '4'].includes(args[0]) ? args[0] : '2';

    await global.loading(m, sock);

    try {
        // Download biner media dari WhatsApp
        let imgBuffer = await q.download();
        let resultBuffer = null;

        // --- SISTEM CALLBACK / FALLBACK ENGINE RUNTIME ---
        try {
            console.log('[Upscaler] Mencoba Engine 1 (ImgLarger)...');
            resultBuffer = await upscaleEngine1(imgBuffer, parseInt(scale));
        } catch (err1) {
            console.error(`[Upscaler] Engine 1 Gagal: ${err1.message}`);
            try {
                console.log('[Upscaler] Callback ke Engine 2 (iLoveIMG Axios)...');
                resultBuffer = await upscaleEngine2(imgBuffer, scale);
            } catch (err2) {
                console.error(`[Upscaler] Engine 2 Gagal: ${err2.message}`);
                console.log('[Upscaler] Callback terakhir ke Engine 3 (iLoveIMG Fetch)...');
                resultBuffer = await upscaleEngine3(imgBuffer, scale);
            }
        }

        if (!resultBuffer) throw new Error("Seluruh engine gagal memproses gambar.");

        // Kirim kembali gambar yang sudah HD
        await sock.sendMessage(m.chat, {
            image: resultBuffer,
            caption: `Merasa terbantu dengan bot ini?\n\nDukung kami untuk terus berkembang dan menjaga server tetap aktif 24/7\nhttps://payment.lumiskin.lol\n\nTerimakasih ❤️\n• *Rasio:* ${scale}x HD\n• *Engine:* Auto-Callback System`
        }, { quoted: m });

    } catch (e) {
        console.error(e);
        m.reply(`⚠️ *Error:* ${e.message}`);
    } finally {
        await global.loading(m, sock, true);
    }
};

handler.help = ['upscale'];
handler.tags = ['tools'];
handler.command = /^(upscale|hd|remini|enhancer)$/i;

export default handler;
