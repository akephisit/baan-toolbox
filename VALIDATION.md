# ผลการตรวจรับ

ทดสอบจาก workspace นี้และ deployment จริงที่ https://baan-toolbox-ake.pages.dev

## ผ่านแล้ว

- Release build ของ Dioxus frontend, processor WASM และ Rust signaling Worker
- `cargo test --locked`: 6 tests ครอบคลุม incremental SHA-256, empty digest, offset เกิน 4 GiB, frame/overflow validation, ชื่อไฟล์ภาษาไทย และ keyed/domain-separated discovery
- `cargo check --locked --workspace --target wasm32-unknown-unknown`
- Chromium: ส่งจริงพร้อมกันสองผู้รับ หลายไฟล์ ไฟล์ว่าง และชื่อภาษาไทย; ดาวน์โหลดผ่านปุ่มบันทึกแล้วเทียบ SHA-256 กับต้นฉบับใน Node
- ปฏิเสธหนึ่งผู้รับโดยผู้รับอีกคนยังรับจบได้
- เลือกสี่ผู้รับ: ทำงานสามราย รายที่เหลือเข้าคิว และเริ่มได้เมื่อมีช่องว่าง
- กลุ่ม QR/ลิงก์แยกจากกลุ่มค้นหาอัตโนมัติ
- Chromium ↔ WebKit ในโปรไฟล์ปกติ: เริ่มเชื่อมต่อและส่งสวนกันได้ พร้อมรับลง OPFS และดาวน์โหลดไฟล์กลับมาเทียบ SHA-256
- FileSystemFileHandle: streamed writer และเนื้อหาที่บันทึกถูกต้อง โดยตัวเลือกไฟล์ OS ถูกจำลองด้วย handle จริงจาก browser filesystem
- ยกเลิกก่อนยอมรับ และปิดผู้รับขณะกำลังรับไฟล์ ระบบยุติงานและแจ้งล้มเหลว
- จำลองการเชื่อมต่อโดยตรงล้มเหลว: ไม่มี fallback อัปโหลด และ configuration ไม่มี TURN
- Worker: offset ผิด, digest ไม่ตรง, quota preflight, abort และการอ่าน/hash ทีละส่วน
- หน้า desktop และ viewport มือถือ 390 × 844 ไม่มีแนวนอนล้น
- ตรวจ HTTPS frontend และ Worker `/health` รวมถึง WebSocket/signaling บน Cloudflare จริง

## ไฟล์ใหญ่

ผ่านการส่งไฟล์ **4,295,000,067 bytes (เกิน 4 GiB)** จาก deployment จริง รับลง OPFS ดาวน์โหลดผ่านปุ่มบันทึก และคำนวณ SHA-256 แบบ stream ใน Node ตรงกับต้นฉบับ:

`885281d9112c7db9706d9a910747d9193657fc92cd2390cfd1087ad8c43e6a48`

ใช้โปรไฟล์ Chromium ปกติบนดิสก์ที่รายงานโควตา 10 GiB ตัวอย่าง RSS รวมทุก process ของ Chromium สอง instance ระหว่างส่งอยู่ที่ 2,307–2,655 MiB ซึ่งรวมต้นทุนของ browser ทั้งหมด ไม่ใช่เฉพาะ buffer ของแอป

ในการทดสอบบนโปรไฟล์ที่ใช้พื้นที่ชั่วคราวก่อนหน้านี้ พบการเขียนล้มเหลวใกล้ 4 GiB ระบบยุติงาน แจ้งข้อจำกัดพื้นที่ และไม่แสดงผลว่าส่งสำเร็จ จึงยังไม่รับประกันขนาดเดียวกันบนทุกอุปกรณ์

## ข้อจำกัดการตรวจสอบ

- WebKit บน Linux ไม่ใช่ Safari บน iPhone/iPad จริง ยังต้องตรวจการ export ไฟล์หลาย GB, การล็อกหน้าจอ/พักแอป และการขอสิทธิ์พื้นที่บนอุปกรณ์จริง
- ทดสอบ Chromium ไม่ได้แทนการตรวจ Chrome/Edge ทุกเวอร์ชัน
- ทดสอบเส้นทาง streamed writer ด้วย FileSystemFileHandle จริง แต่ไม่ได้กดเลือกที่บันทึกผ่าน native OS picker ในระบบทดสอบอัตโนมัติ
- โหมดส่วนตัวของ WebKit ที่ทดสอบเปิด OPFS ไม่ได้ แอปแจ้งให้ใช้แท็บปกติหรือ browser อื่น ก่อนส่งเนื้อหาไฟล์
- โควตาที่ `storage.estimate()` รายงานเป็นการประมาณและการเขียนอาจยังล้มเหลวระหว่างรับ แอปจะแจ้งว่าไม่สำเร็จและล้างไฟล์บางส่วน
- RSS รวมทุก process ของ browser มี cache และหน่วยความจำของ browser เอง ไม่ใช่ขนาด transfer buffer; ตัว algorithm ใช้ window/buffer ขนาดคงที่ ไม่ได้อ่านไฟล์ทั้งก้อนเข้า WASM
