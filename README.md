# บ้านเครื่องมือ — ส่งไฟล์ใกล้ตัว

**เปิดใช้งาน:** https://baan-toolbox-ake.pages.dev

Signaling deployment: https://baan-toolbox-signaling.kruakemaths.workers.dev

เว็บรวมเครื่องมือภาษาไทย เครื่องมือแรกส่งไฟล์ รูป และวิดีโอให้หลายอุปกรณ์ผ่าน WebRTC โดยตรง ใช้ Dioxus และ Rust/WASM ฝั่ง client พร้อม Cloudflare Worker/Durable Object สำหรับรายชื่อและ signaling เท่านั้น

## เริ่มใช้งานเว็บ

1. เปิดเว็บบนทั้งสองอุปกรณ์ ตั้งชื่อเครื่องด้วยปุ่มรูปดินสอ
2. เลือกไฟล์ แล้วเลือกผู้รับได้หลายคน กดส่ง
3. ผู้รับกดรับไฟล์ ถ้า browser รองรับจะถามตำแหน่งบันทึก มิฉะนั้นรับลงพื้นที่ชั่วคราวแล้วกด **บันทึกไฟล์**
4. ถ้าหาชื่อไม่เจอ ให้เปิด **เชื่อมต่อด้วยรหัส / QR** สร้างกลุ่ม แล้วให้อีกเครื่องเปิดลิงก์/สแกน QR หรือกรอกรหัส กลุ่มหมดอายุใน 30 นาที

การค้นหาอัตโนมัติใช้ public IP โดยประมาณ จึงอาจพบคนที่อยู่อินเทอร์เน็ตทางออกเดียวกัน แต่ไม่ใช่ Wi-Fi เดียวกัน หรือไม่พบคนใน Wi-Fi เดียวกันที่ใช้ IPv6/VPN ต่างกัน ผู้รับยืนยันก่อนเริ่มส่งเนื้อหาเสมอ

**ไม่ตั้งเพดานขนาดไฟล์โดยตัวแอป** แต่ข้อจำกัดจริงขึ้นกับ API, พื้นที่ว่าง, โควตา, browser และอุปกรณ์ โดยเฉพาะการ export ไฟล์ใหญ่บน iPhone/iPad เปิดหน้าเว็บทั้งสองเครื่องไว้ขณะส่ง ไม่รับรองการทำงานเมื่อล็อกหน้าจอ/สลับแอป ไม่มี resume หลัง reload หรือหลุดกลางทาง

## พัฒนาในเครื่อง

ต้องมี Node.js 22+ และ Rust 1.98.1 ตาม `rust-toolchain.toml` (rustup จะเตรียม target WASM ให้)

```bash
npm ci
npm run dev
```

เปิด `http://localhost:8080` จะมี signaling ที่ `http://localhost:8787` สคริปต์สร้าง secret สำหรับ local ใน `.dev.vars` ให้อัตโนมัติ อย่า commit ไฟล์นี้

`npm run dev` build เมื่อเริ่มต้น; ถ้าแก้ frontend ให้รัน `npm run build` แล้ว reload หน้าเว็บ ส่วน Wrangler เฝ้าดู source signaling และ build ใหม่ให้อัตโนมัติ

```bash
npm run check        # unit tests และตรวจ compile ทั้ง workspace
npm run build        # release browser bundles ใน dist/
npx playwright install chromium webkit
npm run test:processor
npm run test:e2e
npm run test:compat
npm run test:failures
npm run test:direct-save
npm run test:queue
npm run test:large   # ส่งไฟล์เกิน 4 GiB จริง ใช้เวลาและพื้นที่ดิสก์หลาย GB
```

ตั้ง `CHROMIUM_PATH` / `WEBKIT_PATH` หากต้องใช้ executable ที่ติดตั้งอยู่แล้ว และ `TEST_URL` หากทดสอบ deployment จริง ทดสอบ browser ต้องเปิด dev servers ไว้ก่อน `LARGE_BYTES` เปลี่ยนขนาดไฟล์ใน large test ได้

## โครงสร้าง

- `crates/app`: Dioxus UI, state, WebRTC, WebSocket และ bridge ไป Web Worker
- `crates/core`: incremental SHA-256, binary frame, filename และการตรวจ offset
- `crates/protocol`: public Rust types สำหรับ signaling/control/file metadata; protocol version 1
- `crates/processor`: Rust/WASM ใน module Worker อ่าน File ทีละส่วน, hashing, OPFS และ streamed file writer
- `crates/signaling`: Rust workers-rs, HMAC discovery groups, WebSocket presence และ SDP relay
- `assets`: CSS และ JavaScript bootstrap เท่านั้น ไม่มี logic ประมวลผลไฟล์ใน JavaScript
- `scripts`: build/check/dev/deploy; `tests`: browser integration tests

ใช้ wasm-pack สร้าง Dioxus CSR และ processor เป็น WASM คนละโมดูล จึงไม่ต้องติดตั้ง Dioxus CLI ใน pipeline ปัจจุบัน ทุก dependency ถูกตรึงด้วย Cargo.lock/package-lock.json

## ข้อตกลงข้อมูลและการไหล

Cloudflare รับเฉพาะชื่ออุปกรณ์ รายชื่อสมาชิก และ SDP เพื่อจับคู่ ไม่มี API อัปโหลดไฟล์ และไม่มี TURN relay ชื่อไฟล์/ขนาด/เนื้อหาถูกส่งหลังเปิด DataChannel แล้ว ผ่านการเข้ารหัส DTLS ของ WebRTC

- WebSocket `/signal`: Hello, Rename, Signal, Ping; server กำหนด peer ID และตรวจสิทธิ์การส่งข้ามกลุ่ม
- `/health`: ตรวจสถานะ deployment และ protocol version
- DataChannel: OfferStart → OfferFile แต่ละรายการ → OfferEnd → ผู้รับ Accept/Reject → Start/Ready → binary chunks/Ack → End/SHA-256 → Verified → BatchDone
- เฟรมไบนารีไม่เกิน 16 KiB ประกอบด้วย job ID 16 bytes, offset แบบ little-endian u64 8 bytes และ payload
- ส่งแบบ reliable/ordered; จำกัด window ไว้ 16 chunks ต่อผู้รับ และ ACK หลังเขียนข้อมูลสำเร็จ เพื่อไม่ให้ผู้รับที่ช้าทำให้คิวหน่วยความจำโต
- ส่งพร้อมกันสูงสุดสามผู้รับ รายที่เหลือเข้าคิว อ่านไฟล์ต้นฉบับแยกกันตามความเร็วผู้รับ ความคืบหน้าอ้างอิง bytes ที่ผู้รับเขียนแล้ว
- ไม่ลดคุณภาพ บีบอัด หรือแปลงไฟล์อัตโนมัติ
- ข้อมูลไฟล์ใช้ u64 ใน Rust; browser File API ใช้เลขที่แม่นยำได้ถึง 2^53-1 จึงตรวจขอบเขต API นี้ด้วย
- กลุ่มรับสมาชิกสูงสุด 100 การเชื่อมต่อ คำขอหนึ่งชุดรับ metadata ได้สูงสุด 4,096 ไฟล์เพื่อจำกัดหน่วยความจำ รายการไฟล์ยาวกว่านี้ควรแบ่งส่งหลายชุด

OPFS อยู่ใน directory `baan-toolbox-transfers` เมื่อกดยกเลิกจะลบไฟล์บางส่วนของงานนั้น เมื่อรับสำเร็จให้บันทึกไฟล์ก่อนกด × เพื่อล้างสำเนาชั่วคราว หาก browser ถูกปิดฉับพลันอาจมีไฟล์ชั่วคราวค้าง ล้างข้อมูลเว็บไซต์ผ่านการตั้งค่า browser เพื่อคืนพื้นที่ หลีกเลี่ยงการล้างข้อมูลขณะมีแท็บอื่นรับไฟล์อยู่

ไฟล์ที่บันทึกลงโฟลเดอร์โดยตรงจะมี prefix สุ่มเพื่อไม่เขียนทับไฟล์เดิมที่ชื่อซ้ำ ส่วนกรณีเลือก Save File รายไฟล์ browser เป็นผู้ถามยืนยันตำแหน่ง/การเขียนทับ

## Deploy บนบัญชี Cloudflare ของคุณ

ใช้ Pages สำหรับ static frontend และ Worker แยกสำหรับ Durable Object (ไม่สามารถประกาศ/สร้าง Durable Object ภายใน Pages project โดยตรง)

```bash
npx wrangler login
npx wrangler pages project create YOUR_PAGES_PROJECT --production-branch main --force
npx wrangler deploy --var 'ALLOWED_ORIGINS:https://YOUR_PAGES_PROJECT.pages.dev'
npx wrangler secret put DISCOVERY_SECRET
```

สำหรับ `DISCOVERY_SECRET` ให้ใช้ค่าสุ่มอย่างน้อย 32 characters เช่นสร้างด้วย `openssl rand -hex 32` และส่งเข้าทาง stdin; อย่าใส่ secret ใน source, CLI arguments หรือ frontend

จากนั้นใช้ URL Worker ที่ Wrangler แสดง:

```bash
export PAGES_PROJECT=YOUR_PAGES_PROJECT
export SIGNALING_URL=https://YOUR_WORKER.YOUR_SUBDOMAIN.workers.dev
export ALLOWED_ORIGINS=https://YOUR_PAGES_PROJECT.pages.dev
npm run deploy
```

หากใช้ custom domain ให้เพิ่ม origin แบบตรงตัวใน `ALLOWED_ORIGINS` คั่นด้วย comma ไม่มี wildcard แก้ชื่อ Worker ใน `wrangler.jsonc` ก่อน deploy หากใช้ชื่ออื่น การติดตั้ง worker-build ครั้งแรกใช้เวลาสักครู่ สคริปต์ใช้ legacy shim ของ workers-rs (`--no-panic-recovery`) ซึ่ง build ได้กับ toolchain ที่ตรึงไว้

Wrangler รุ่นที่ตรึงไว้มีการนำคำสั่ง Pages ไปใช้ Workers โดยอัตโนมัติ ขั้นตอนสร้าง Pages project ใช้ `--force` เพื่อเลือก Cloudflare Pages โดยตรงตามสถาปัตยกรรมที่ต้องการ หลังสร้าง project แล้วคำสั่ง deploy ปกติจะใช้ Pages โดยตรง

ไม่ต้องมี VPS, R2, D1 หรือ storage สำหรับไฟล์บน Cloudflare `observability` ปิดเป็นค่าเริ่มต้น ไม่ log SDP, IP หรือชื่อไฟล์ใน application code

## ขอบเขตการตรวจรับ

ผลการทดสอบจริงบันทึกใน `VALIDATION.md` การทดสอบ WebKit บน Linux ช่วยตรวจความเข้ากันได้ แต่ไม่แทน Safari บน iPhone/iPad จริง ต้องทดสอบการพักแอปและ export ไฟล์ใหญ่บนอุปกรณ์จริงเพิ่มเติม
