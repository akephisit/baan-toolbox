use crate::engine::{engine, format_size, Engine, Model};
use dioxus::prelude::*;

#[component]
pub fn App() -> Element {
    let model = use_signal(Model::default);
    let mut init_error = use_signal(String::new);
    use_hook(move || {
        if let Err(e) = Engine::init(move |m| {
            let mut state = model;
            state.set(m);
        }) {
            init_error.set(e);
        }
    });
    let mut page = use_signal(|| "send".to_owned());
    let mut search = use_signal(String::new);
    let mut room_input = use_signal(String::new);
    let mut show_pair = use_signal(|| false);
    let m = model.read().clone();
    let total = m.files.iter().fold(0u64, |a, f| a.saturating_add(f.size));
    let selected = m.selected.len();
    let peer_count = m.peers.len();
    let status = if m.online {
        "พร้อมเชื่อมต่อ"
    } else if m.connecting {
        "กำลังเชื่อมต่อ…"
    } else {
        "ยังไม่เชื่อมต่อ"
    };
    let mut name_draft = use_signal(String::new);
    let mut editing_name = use_signal(|| false);
    rsx! {
        div { class:"app-shell",
            aside { class:"sidebar",
                a {class:"brand",href:"#",onclick:move|e|{e.prevent_default();page.set("home".into());},
                    span {class:"brand-mark", "บ"} div { strong {"บ้านเครื่องมือ"} span {"ของใช้ดิจิทัลประจำวัน"} }
                }
                div {class:"nav-label","WORKSPACE"}
                button {class:if page()=="home"{"nav-item active"}else{"nav-item"},onclick:move|_|page.set("home".into()),span {"▦"} "เครื่องมือทั้งหมด"}
                button {class:if page()=="send"{"nav-item active"}else{"nav-item"},onclick:move|_|page.set("send".into()),span {"↗"} "ส่งไฟล์ใกล้ตัว" span {class:"nav-dot"}}
                div {class:"sidebar-bottom",
                    div {class:"local-badge",span {class:"shield","◇"} div {strong {"ไฟล์ของคุณ อยู่กับคุณ"} p {"ส่งตรงระหว่างอุปกรณ์\nไม่มีการฝากไฟล์บนเซิร์ฟเวอร์"}}}
                    div {class:"version","บ้านเครื่องมือ / 01"}
                }
            }
            main {class:"main",
                header {class:"topbar",
                    div {class:"breadcrumb","เครื่องมือ" span {"/"} strong {if page()=="send"{"ส่งไฟล์ใกล้ตัว"}else{"ทั้งหมด"}}}
                    div {class:if m.online{"connection online"}else{"connection"},span {class:"status-dot"} "{status}"}
                }
                div {class:"content",
                    if !init_error().is_empty(){div {class:"notice error", "เริ่มต้นแอปไม่ได้: {init_error}"}}
                    if !m.notice.is_empty(){div {class:"notice",role:"alert",span {"{m.notice}"}button {class:"icon-button",aria_label:"ปิดข้อความ",onclick:move|_|engine().dismiss(),"×"}}}
                    if page()=="home"{
                        div {class:"page-heading",div {span {class:"eyebrow","YOUR EVERYDAY KIT"}h1 {"เครื่องมือเล็ก ๆ ที่ช่วยได้ทุกวัน"}p {"เรียบง่าย ใช้ง่าย และประมวลผลบนอุปกรณ์ของคุณ"}}}
                        div {class:"search-field",span {"⌕"}input {placeholder:"ค้นหาเครื่องมือ…",value:"{search}",oninput:move|e|search.set(e.value())}}
                        if search().is_empty()||"ส่งไฟล์ใกล้ตัว รูป วิดีโอ file transfer".contains(&search().to_lowercase()){
                            button {class:"tool-card",onclick:move|_|page.set("send".into()),span {class:"tool-icon","↗"}span {class:"small-tag","พร้อมใช้"}h2 {"ส่งไฟล์ใกล้ตัว"}p {"ส่งรูป วิดีโอ และไฟล์ให้หลายอุปกรณ์\nผ่านการเชื่อมต่อโดยตรง"}div {class:"card-footer","เปิดเครื่องมือ" span {"↗"}}}
                        }else{div {class:"empty-state",h3 {"ยังไม่พบเครื่องมือ"}p {"ลองค้นหาด้วยคำว่า ส่งไฟล์ รูป หรือวิดีโอ"}}}
                    }else{
                        div {class:"page-heading",div {span {class:"eyebrow","A LITTLE CLOSER. A LOT EASIER."}h1 {"ส่งไฟล์ใกล้ตัว"}p {"จากเครื่องนี้ ถึงคนข้าง ๆ เลือกไฟล์ เลือกคน แล้วส่งได้เลย"}}span {class:"heading-icon","↗"}}
                        div {class:"device-strip",
                            div {class:"device-avatar","▣"}div {class:"device-details",span {class:"muted small","อุปกรณ์ของคุณ"}
                                if editing_name(){div {class:"rename-row",input {id:"device-name",maxlength:40,value:"{name_draft}",aria_label:"ชื่ออุปกรณ์",oninput:move|e|name_draft.set(e.value())}button {class:"text-button",onclick:move|_|{engine().rename(name_draft());editing_name.set(false);},"บันทึก"}}}
                                else{button {class:"device-name",onclick:move|_|{name_draft.set(engine_name());editing_name.set(true);},"{m.name}" span {"✎"}}}
                            }
                            div {class:"strip-actions",button {class:"secondary small-button",onclick:move|_|show_pair.set(!show_pair()),"เชื่อมต่อด้วยรหัส / QR"}if !m.online&&!m.connecting{button {class:"text-button",onclick:move|_|engine().reconnect(),"เชื่อมต่อใหม่"}}}
                        }
                        if show_pair(){
                            section {class:"pair-panel",
                                div {h3 {"เจอกันได้ แม้หาอุปกรณ์ไม่พบ"}p {class:"muted","สร้างกลุ่มแล้วส่งลิงก์หรือ QR ให้ผู้รับ กลุ่มมีอายุ 30 นาที"}
                                    div {class:"join-row",input {aria_label:"ลิงก์หรือรหัสกลุ่ม",placeholder:"วางลิงก์หรือรหัสกลุ่ม",value:"{room_input}",oninput:move|e|room_input.set(e.value())}button {class:"secondary",disabled:m.busy,onclick:move|_|engine().join_room(&room_input()),"เข้ากลุ่ม"}}
                                    div {class:"pair-actions",button {class:"text-button",disabled:m.busy,onclick:move|_|engine().new_room(),"＋ สร้างกลุ่มใหม่"}if !m.room.is_empty(){button {class:"text-button",disabled:m.busy,onclick:move|_|engine().auto_group(),"กลับไปค้นหาอัตโนมัติ"}}}
                                    if !m.room.is_empty(){div {class:"room-code",span {"รหัสกลุ่ม"}code {"{m.room}"}}div {class:"share-link",input {readonly:true,value:"{m.room_link}",aria_label:"ลิงก์กลุ่ม — เลือกและคัดลอก"}p {class:"muted small","แตะเลือกลิงก์เพื่อคัดลอก หรือให้ผู้รับสแกน QR"}}}
                                }
                                if !m.qr.is_empty(){div {class:"qr",dangerous_inner_html:"{m.qr}"}}
                            }
                        }
                        for offer in m.offers.iter(){
                            {let accept=offer.batch.clone();let reject=offer.batch.clone();let names=offer.files.iter().map(|f|f.name.clone()).collect::<Vec<_>>().join(", ");let bytes=offer.files.iter().fold(0u64,|a,f|a.saturating_add(f.size));rsx!{
                                section {class:"incoming-request",key:"{offer.batch}",span {class:"request-icon","↙"}div {h3 {"{offer.sender} ต้องการส่งไฟล์ให้คุณ"}p {"{names}"}span {class:"muted small","{offer.files.len()} ไฟล์ · {format_size(bytes)}"}}div {class:"request-actions",button {class:"secondary",onclick:move|_|engine().reject(&reject),"ปฏิเสธ"}button {class:"primary",onclick:move|_|engine().accept(&accept),"รับไฟล์"}}}
                            }}
                        }
                        div {class:"send-grid",
                            section {class:"panel files-panel",
                                div {class:"panel-heading",h2 {span {class:"step","01"}"เลือกไฟล์"}span {class:"muted small","ไฟล์ต้นฉบับ ไม่ลดคุณภาพ"}}
                                div {id:"dropzone",class:"dropzone",onmounted:move|_|engine().bind_drop(),ondragover:move|e|e.prevent_default(),
                                    div {class:"upload-icon","↥"}h3 {"วางไฟล์ของคุณตรงนี้"}p {"รูป วิดีโอ เอกสาร หรือไฟล์อะไรก็ได้"}
                                    label {class:if m.busy{"secondary disabled"}else{"secondary choose-files"},r#for:"file-input","＋ เลือกไฟล์จากเครื่อง"}
                                    input {id:"file-input",r#type:"file",multiple:true,disabled:m.busy,onchange:move|_|engine().pick_input(),hidden:true}
                                    span {class:"drop-footnote","ไม่กำหนดเพดานขนาดไฟล์ · ขึ้นกับอุปกรณ์และพื้นที่ว่าง"}
                                }
                                if !m.files.is_empty(){
                                    div {class:"file-summary",span {"เลือกแล้ว {m.files.len()} ไฟล์"}button {class:"text-button",disabled:m.busy,onclick:move|_|engine().clear_files(),"ล้างทั้งหมด"}}
                                    div {class:"file-list",for file in m.files.iter(){
                                        {let remove=file.id.clone();let icon=if file.mime.starts_with("image/"){ "▧" }else if file.mime.starts_with("video/"){"▷"}else{"▤"};rsx!{
                                            div {class:"file-row",key:"{file.id}",span {class:"file-icon","{icon}"}div {class:"file-info",strong {"{file.name}"}span {"{format_size(file.size)}"}}button {class:"icon-button",aria_label:"นำไฟล์ออก",disabled:m.busy,onclick:move|_|engine().remove_file(&remove),"×"}}
                                        }}
                                    }}
                                }
                            }
                            section {class:"panel recipients-panel",
                                div {class:"panel-heading",h2 {span {class:"step","02"}"เลือกผู้รับ"}span {class:"count-badge","{peer_count}"}}
                                div {class:"recipient-note",span {class:"tiny-dot"}if m.room.is_empty(){span {"อุปกรณ์ที่อาจอยู่เครือข่ายเดียวกัน"}}else{span {"อุปกรณ์ในกลุ่มของคุณ"}}}
                                if m.peers.is_empty(){div {class:"recipient-empty",div {class:"radar","◎"}h3 {if m.online{"รอเพื่อนเข้ามาใกล้ ๆ"}else{"กำลังรอการเชื่อมต่อ"}}p {"เปิดเว็บนี้บนอุปกรณ์อีกเครื่อง\nแล้วชื่อผู้รับจะปรากฏที่นี่"}button {class:"text-button",onclick:move|_|show_pair.set(true),"หาไม่เจอ? ใช้รหัสหรือ QR ↗"}}}
                                else{div {class:"recipient-toolbar",span {class:"muted small","เลือกได้มากกว่าหนึ่งคน"}button {class:"text-button",onclick:move|_|engine().select_all(),"เลือกทั้งหมด"}}div {class:"recipient-list",for peer in m.peers.iter(){
                                    {let pid=peer.id.clone();let checked=m.selected.contains(&peer.id);rsx!{
                                        button {class:if checked{"recipient selected"}else{"recipient"},key:"{peer.id}",aria_pressed:checked,onclick:move|_|engine().toggle(&pid),span {class:"peer-avatar","▣"}div {strong {"{peer.name}"}span {"พร้อมรับไฟล์ · {peer.id.chars().take(4).collect::<String>()}"}}span {class:"checkmark",if checked{"✓"}else{""}}}
                                    }}
                                }}}
                                div {class:"send-footer",div {span {class:"muted small","พร้อมส่ง"}strong {"{m.files.len()} ไฟล์ · {format_size(total)}"}}
                                    button {class:"primary send-button",disabled:!m.online||!m.worker_ready||m.busy||selected==0||m.files.is_empty(),onclick:move|_|engine().send(),if m.busy{"กำลังส่ง…"}else{ "ส่งให้ {selected} คน" }span {"↗"}}
                                    p {class:"muted small","ผู้รับจะได้ยืนยันก่อนเริ่มรับไฟล์"}
                                }
                            }
                        }
                        if !m.transfers.is_empty(){section {class:"panel transfers-panel",div {class:"panel-heading",h2 {"การรับส่งไฟล์"}span {class:"muted small","สถานะแยกตามผู้รับ"}}
                            for t in m.transfers.iter().rev(){
                                {let batch=t.batch.clone();let percent=if t.finished&&!t.failed{100.0}else if t.total==0{0.0}else{(t.done as f64/t.total as f64*100.0).min(99.0)};rsx!{
                                    div {class:if t.failed{"transfer failed"}else{"transfer"},key:"{t.batch}",div {class:"transfer-top",span {class:"transfer-direction",if t.incoming{"↙"}else{"↗"}}div {strong {if t.incoming{"รับจาก "}else{"ส่งให้ "}"{t.peer}"}p {"{t.status}"}}span {class:"progress-label","{percent:.0}%"}if !t.finished{button {class:"text-button",onclick:move|_|engine().cancel(&batch,"ยกเลิกการส่งแล้ว"),"ยกเลิก"}}}div {class:"progress-track",div {class:"progress-fill",style:"width:{percent}%"}}div {class:"transfer-size","{format_size(t.done)} / {format_size(t.total)}"}}
                                }}
                            }
                        }}
                        if !m.downloads.is_empty(){section {class:"panel downloads-panel",div {class:"panel-heading",h2 {"ไฟล์ที่รับแล้ว"}span {class:"verified-label","✓ ตรวจสอบ SHA-256 ผ่าน"}}
                            for d in m.downloads.iter(){
                                {let job=d.job.clone();rsx!{div {class:"download-row",key:"{d.job}",span {class:"file-icon","✓"}div {class:"file-info",strong {"{d.name}"}span {"{format_size(d.size)}"}}if d.direct{span {class:"muted small","บันทึกแล้ว"}}else{a {class:"secondary small-button",href:"{d.url}",download:"{d.name}","บันทึกไฟล์ ↧"}}button {class:"icon-button",aria_label:"ล้างรายการและไฟล์ชั่วคราว",onclick:move|_|engine().delete_download(&job),"×"}}}}
                            }
                            p {class:"muted small","บันทึกไฟล์ก่อนปิดหน้าเว็บ การล้างรายการจะลบสำเนาชั่วคราวของเว็บด้วย"}
                        }}
                        div {class:"bottom-notes",div {span {"◇"}strong {"ส่งตรงและเข้ารหัส"}p {"ไม่ฝากไฟล์ไว้บนคลาวด์"}}div {span {"∞"}strong {"ไฟล์ใหญ่ก็ทยอยส่ง"}p {"ตามพื้นที่และความสามารถของเครื่อง"}}div {span {"☀"}strong {"เปิดหน้านี้ไว้ขณะส่ง"}p {"มือถืออาจหยุดทำงานเมื่อสลับแอป"}}}
                        footer {class:"page-footer",span {"สร้างพื้นที่เล็ก ๆ ให้ชีวิตง่ายขึ้น"}span {"ส่งตรงเท่านั้น · ไม่มี relay"}}
                    }
                }
            }
        }
    }
}
fn engine_name() -> String {
    // The saved name is safe to read without exposing engine internals.
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("toolbox.name").ok().flatten())
        .unwrap_or_default()
}
