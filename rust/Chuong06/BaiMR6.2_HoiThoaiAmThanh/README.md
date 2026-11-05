# Bài mở rộng MR6.2 – Phòng hội thoại âm thanh nhiều client (Rust)

| Thông tin | Nội dung |
|---|---|
| Loại | **Bài mở rộng** – không có trong giáo trình, không có mã giả tương ứng |
| Liên hệ với giáo trình | Chương 6: Bài 6.1 (server đa luồng), Bài 6.4 (server chat nhiều client); Chương 4: Bài 4.1 (UDP); Chương 3: Bài 3.2 (định khung thông điệp) |
| Thư viện | Thư viện chuẩn Rust: `std::net` (`TcpStream`, `UdpSocket`), `std::thread`, `Mutex`, `to_be_bytes`/`to_le_bytes`; crate `serde_json` 1.0.154 (kênh điều khiển, tệp mốc), `ctrlc` 3.4.7 (in tổng kết khi Ctrl+C). WAV và Goertzel tự viết. Ứng dụng giao diện: `eframe`/`egui` 0.29.1 (OpenGL qua `glow`), `cpal` 0.15.3 (micro/loa thật), `miniz_oxide` 0.8.9 (PNG ảnh chụp cửa sổ) |
| Bản ngôn ngữ khác | [C#](../../../csharp/Chuong06/BaiMR6.2_HoiThoaiAmThanh/), [Java](../../../java/Chuong06/BaiMR6.2_HoiThoaiAmThanh/), [Python](../../../python/Chuong06/BaiMR6.2_HoiThoaiAmThanh/), [Go](../../../go/Chuong06/BaiMR6.2_HoiThoaiAmThanh/) |

## Mục tiêu
Xây dựng một "phòng họp thoại" thu nhỏ: n client nối tới một server; mỗi client gửi âm thanh micro dạng PCM qua UDP, server **trộn tiếng** và gửi cho mỗi người tiếng của *những người khác* ("mix-minus"). Kênh điều khiển TCP cho phép server tắt/bật mic của tất cả hoặc từng người, đặt độ lợi đầu vào từng người, âm lượng chung; mỗi client tự tắt mic và tự chỉnh âm lượng nghe. Client ghi lại những gì nghe được thành tệp WAV; chương trình phân tích tần số (thuật toán Goertzel) **chứng minh bằng số liệu** rằng mỗi lệnh điều khiển đã có hiệu lực.

## Kiến thức chính
- **Âm thanh số.** Lấy mẫu 16 000 lần/giây, mỗi mẫu 16 bit có dấu, một kênh (mono) → 16 000 × 16 = 256 kbit/s. Khung 20 ms = 320 mẫu = 640 byte → 50 gói/giây. Phụ phí header mỗi gói 12 (RTP) + 8 (UDP) + 20 (IPv4) = 40 byte → 16 kbit/s; tổng ≈ 272 kbit/s mỗi chiều cho mỗi client (đúng với số `272 kbit/s gồm header` client in ra khi mic mở suốt).
- **Hai kênh, hai giao thức.** Điều khiển cần tin cậy, đúng thứ tự → TCP, mỗi dòng một JSON (định khung theo dòng như Bài 3.2). Âm thanh cần đến đúng lúc → UDP: mất một gói chỉ mất 20 ms, còn truyền lại kiểu TCP sẽ đến quá muộn để phát.
- **Gói RTP (RFC 3550).** Header 12 byte: `V=2`, payload type (96 = âm thanh L16, 127 = gói giữ kết nối rỗng), **số thứ tự** 16 bit (phát hiện mất, đảo thứ tự), **nhãn thời gian** 32 bit tính bằng mẫu (tăng 320 mỗi khung, *kể cả khi tắt mic*), **SSRC** = mã client. Mẫu L16 theo thứ tự byte mạng (big-endian, RFC 3551, `i16::to_be_bytes`), trong khi WAV lưu little-endian (`to_le_bytes`).
- **Bộ đệm jitter.** Gói đến không đều (jitter). Bên nhận giữ vài khung rồi mới phát mỗi 20 ms một khung theo số thứ tự: thiếu khung → phát khoảng lặng (che lỗi), rỗng → *cạn bộ đệm*, chờ nạp lại; đến sau lượt phát → *trễ*, bỏ. Đệm sâu hơn chịu jitter tốt hơn nhưng tăng độ trễ (3 khung = 60 ms). Jitter được ước lượng theo RFC 3550 mục 6.4.1: `J += (|D| − J)/16`.
- **Trộn tiếng, mix-minus, cắt ngọn.** Mỗi nhịp 20 ms: tổng `T = Σ gᵢ·xᵢ` của các client đang mở mic (gᵢ là độ lợi đầu vào); người nghe k nhận `(T − gₖ·xₖ) × âm_lượng_chung` – không nghe tiếng vọng của mình. Tổng có thể vượt phạm vi 16 bit → **cắt ngọn** (clipping) về −32768…32767, gây méo (sinh hài). Server đếm số mẫu bị cắt ngọn cho từng người nghe.
- **dBFS.** Mức RMS so với toàn thang: sin biên độ 0,25 → −15,1 dBFS; nhân 2 → +6,0 dB; nhân 0,5 → −6,0 dB; hai âm cùng mức cộng lại → +3 dB.
- **Goertzel.** Tính một điểm của DFT bằng bộ lọc bậc hai `s[n] = x[n] + 2cos(ω)s[n−1] − s[n−2]`, rẻ hơn FFT khi chỉ cần vài tần số (cách điện thoại giải mã phím DTMF). Mỗi client phát một tần số riêng (A 440 Hz, B 620 Hz, C 880 Hz), nên đo mức ở từng tần số là biết ai đang nói, to bao nhiêu.
- **Không tin client.** Server tự bỏ gói từ client đang bị tắt mic (kể cả client cố tình gửi), chỉ nhận UDP từ địa chỉ đã đăng ký và cùng IP với kết nối TCP; client gọi `UdpSocket::connect()` để chỉ nhận gói từ server.

### Quy tắc quyền điều khiển

| # | Quy tắc |
|---|---|
| 1 | Mic của X mở **khi và chỉ khi** X không tự tắt **và** server không khóa mic của X |
| 2 | Server tắt (khóa) thắng client: đang bị khóa, client xin bật bị **từ chối** |
| 3 | Server bật = gỡ khóa, **không bật hộ** mic mà người dùng đã tự tắt (quyền riêng tư: server làm người dùng im lặng được, nhưng không tự ý mở micro của họ) |
| 4 | Độ lợi đầu vào từng người và âm lượng chung do server đặt, áp dụng khi trộn |
| 5 | Âm lượng nghe do từng client tự đặt, áp dụng tại client; server không có lệnh đổi giá trị này |
| 6 | Tự tắt mic có hiệu lực ngay tại client; tự bật phải chờ server xác nhận |

Âm lượng người A thực sự nghe thấy tiếng B: `độ_lợi_B × âm_lượng_chung × âm_lượng_nghe_A`.

## Cấu trúc tệp
- `Cargo.toml`, `Cargo.lock` – gói Cargo `mr6_2_hoi_thoai_am_thanh`.
- `src/main.rs` – chọn vai trò `server` / `khach` / `phan_tich` (dòng lệnh) và `server_gui` / `khach_gui` / `demo_gui` (giao diện), đọc tham số.
- `src/chung.rs` – hằng số âm thanh, đóng/tách gói RTP, PCM ↔ byte, dBFS, thanh đo mức, `BoDemJitter`, Goertzel, đọc/ghi WAV (tự viết tiêu đề RIFF 44 byte); có `cargo test` (sin 0,25 → −15,1 dBFS, số thứ tự RTP quay vòng).
- `src/server.rs` – lõi server `MayChu` (dùng chung cho dòng lệnh và giao diện): kênh TCP (mỗi client một luồng), luồng nhận UDP, luồng trộn 20 ms (thêm nguồn tiếng server và đầu ra "nghe" khi chạy giao diện), các hàm điều khiển `khoa_mic`, `dat_do_loi`, `dat_am_luong_chung`, `dung_lai`; bảng điều khiển đọc stdin; mọi trạng thái chung trong một `Mutex`.
- `src/khach.rs` – client: nguồn âm (sin / WAV), vòng 20 ms thu–gửi–nhận–phát, ghi WAV và tệp mốc sự kiện `*_moc.json`.
- `src/phan_tich.rs` – chia bản ghi theo mốc sự kiện, đo mức từng âm bằng Goertzel, kiểm tra mix-minus.
- `src/thiet_bi.rs` – micro/loa thật qua `cpal` (trộn kênh về mono, đổi tần số lấy mẫu về 16 kHz và ngược lại, hàng đợi mẫu có giới hạn) và nguồn giả lập `NguonMic::GiaLap` (sin).
- `src/gui_chung.rs` – phông chữ tiếng Việt, nút màu, khung nhóm, thanh đo mức dBFS, ghi ảnh chụp cửa sổ thành PNG.
- `src/server_gui.rs` – cửa sổ "Bảng điều khiển âm thanh" của server.
- `src/khach_gui.rs` – cửa sổ "Audio Client".
- `src/demo_gui.rs` – kịch bản tự thao tác: server + 3 client trong một cửa sổ, in số liệu, chụp `ket_qua_gui.png`.
- `demo.json`, `ket_qua.txt`, `ket_qua.png` – kịch bản minh họa và kết quả thực thi thật (phần dòng lệnh + phần giao diện); `ket_qua_gui.png` – ảnh chụp cửa sổ giao diện trong lần chạy đó.

## Cách chạy
Trong thư mục bài (`cargo build -q --release` một lần; `B=../../target/release/mr6_2_hoi_thoai_am_thanh`):
```bash
$B server 127.0.0.1 9000 9001                                  # cửa sổ 1 (gõ lệnh điều khiển ở đây)
$B khach B 127.0.0.1 9000 --tan-so 620                         # cửa sổ 2
$B khach C 127.0.0.1 9000 --tan-so 880                         # cửa sổ 3
$B khach A 127.0.0.1 9000 --tan-so 440 --ghi out/ghi_A.wav --thoi-gian 20
$B phan_tich out/ghi_A.wav 440:A 620:B 880:C                   # sau khi A kết thúc
```
Lệnh server: `tat_mic tat_ca`, `tat_mic B`, `bat_mic tat_ca`, `do_loi C 2`, `am_luong_chung 0.5`, `trang_thai`, `thoat`. Lệnh client: `tat_mic`, `bat_mic`, `am_luong 0.5`, `thoat`. Dòng `@t lệnh` chờ tới giây thứ t rồi mới chạy lệnh (dùng cho kịch bản tự động).

Tùy chọn client: `--wav tệp.wav` (nguồn là tệp WAV 16 bit mono 16 kHz, phát lặp), `--mat-goi 0.05` (giả lập mất 5 % gói gửi), `--dem 3` (độ sâu bộ đệm jitter), `--thoi-gian`, `--bien-do`.

**Micro và loa thật:** client *dòng lệnh* vẫn chỉ dùng nguồn tổng hợp/WAV (không cài `--mic`/`--loa`; nếu truyền vào, client báo và dùng nguồn tổng hợp). Micro/loa thật có trong **ứng dụng giao diện** bên dưới (crate `cpal`).

Tái lập kết quả từ thư mục gốc của kho: `python3 tools/demo.py rust/Chuong06/BaiMR6.2_HoiThoaiAmThanh` (cổng 44620/44621; tệp `out/ghi_A.wav` sinh ra khi chạy, không đưa vào kho).

## Ứng dụng giao diện
![Ứng dụng giao diện – server và 3 client](ket_qua_gui.png)

Cùng chương trình có thêm ba vai trò giao diện (egui). Giao thức trên dây giữ nguyên, chỉ **thêm** một thông điệp server → client `{"loai":"do_loi","gia_tri":g}` (gửi một lần sau `chao` và mỗi khi server đổi độ lợi của client đó) để client hiện "Server Vol: xx%"; client cũ (dòng lệnh, mọi ngôn ngữ) bỏ qua loại thông điệp không biết.

- **Server – "Bảng điều khiển âm thanh"** (`server_gui`): nhóm *Kết nối* (IP, cổng TCP/UDP, Start/Stop Server); *Điều khiển chung* (Mở mic tất cả = gỡ khóa, Tắt mic tất cả = khóa, Âm lượng chung); *Micro của server* (chọn micro thật hoặc "Giả lập (sin 330 Hz)", Start/Stop Mic, Mic Volume, thanh đo mức) – tiếng server là **một nguồn trong bộ trộn** với `SSRC_SERVER`, mọi client đều nghe; *Nghe* – phát ra loa máy server tổng tiếng các client (không gồm tiếng server). Dòng trạng thái "Server đang chạy tại …", "Số client: n"; mỗi client một hàng: mã, tên, mic (mở / tự tắt / bị khóa), thanh đo mức vào (dBFS, ~10 lần/giây), nút Khóa/Gỡ khóa, thanh trượt độ lợi 0–200 %, âm lượng nghe client báo lên; nhật ký sự kiện cuộn.
- **Client – "Audio Client"** (`khach_gui`): *Kết nối* (IP, cổng, tên, Connect/Disconnect); *Nhận âm thanh* (Volume 0–200 % = âm lượng nghe, Start/Stop Audio = phát/không phát ra loa, "Server Vol: xx%", thanh đo mức tiếng nhận); *Micro* (chọn micro thật hoặc "Giả lập (sin 440 Hz)", Start/Stop Mic, Mic Volume, thanh đo mức, trạng thái mic theo server); dòng trạng thái (Đã kết nối / Bị từ chối / Mất kết nối).
- **Thiết bị:** danh sách micro thật lấy từ `cpal` (WASAPI trên Windows, ALSA/PulseAudio trên Linux, CoreAudio trên macOS); mục đầu tiên luôn là nguồn giả lập. Micro **không tự bật**: kết nối xong client gửi ngay `tu_tat_mic`, chỉ khi bấm Start Mic mới mở thiết bị và gửi `tu_bat_mic`. Dòng "Nguồn: THẬT – …" / "GIẢ LẬP – …" cho biết đang dùng gì; mở micro thật lỗi thì tự chuyển sang giả lập; không có loa thì vẫn chạy, chỉ đo mức ("không có loa – chỉ đo mức"). Thiết bị thật thường là 48 kHz, 2 kênh, f32: `thiet_bi.rs` trộn về mono, đổi về 16 kHz khi thu và nội suy lên tần số của loa khi phát.

**Chạy trên Windows** (cần Rust bản MSVC như các bài khác; `cpal`, `eframe` không cần cài thêm gì):
```powershell
cargo build --release
..\..\target\release\mr6_2_hoi_thoai_am_thanh.exe server_gui 127.0.0.1 9000 9001
..\..\target\release\mr6_2_hoi_thoai_am_thanh.exe khach_gui A 127.0.0.1 9000
..\..\target\release\mr6_2_hoi_thoai_am_thanh.exe khach_gui B 127.0.0.1 9000 --tan-so 620
```
Muốn nói chuyện giữa hai máy: server đặt IP là địa chỉ LAN của máy (hoặc `0.0.0.0`) và mở cổng TCP/UDP trên tường lửa; mặc định chỉ nghe `127.0.0.1`.

**Chạy trên Linux:** cần thư viện phát triển ALSA để biên dịch `cpal` (`sudo apt-get install -y libasound2-dev pkg-config`), rồi cùng các lệnh trên với `../../target/release/mr6_2_hoi_thoai_am_thanh`. Máy không có card âm thanh (`/proc/asound/cards` trống) thì chương trình bỏ qua bước dò thiết bị và chỉ dùng giả lập. Kịch bản tự động: `xvfb-run -a ../../target/release/mr6_2_hoi_thoai_am_thanh demo_gui 44625 44626 ket_qua_gui.png`.

**Luồng và quy tắc chạm điều khiển:**
```
 luồng giao diện (egui update ~10 lần/s) ── đọc/sửa ──► ChiaSe / Phong (Mutex) ◄── sửa ── luồng nền
   │  vẽ, nhận thao tác, mở/đóng cpal::Stream                                      │
   │                                                                                ├─ luồng kết nối (connect, vao, chao)
   ├─ callback cpal micro ──► hàng đợi 16 kHz ──► luồng 20 ms ── RTP/UDP ──► server ├─ luồng đọc TCP (mic, do_loi, su_kien, tu_choi)
   └─ callback cpal loa   ◄── hàng đợi 16 kHz ◄── luồng 20 ms ◄── bộ đệm jitter ◄─────┴─ luồng nhận UDP
 server: luồng nhận kết nối, mỗi client một luồng TCP, luồng nhận UDP, luồng trộn 20 ms (thêm nguồn server, đẩy "nghe" ra loa)
```
Chỉ luồng giao diện chạm vào điều khiển: luồng nền chỉ sửa trạng thái trong `Mutex` rồi gọi `ctx.request_repaint()`; giao diện đọc trạng thái (sao chép nhanh rồi thả khóa) mỗi lần vẽ. `cpal::Stream` không chuyển được giữa các luồng trên mọi nền tảng nên do luồng giao diện tạo và giữ; callback của cpal chạy trên luồng âm thanh của hệ điều hành và chỉ chép mẫu vào hàng đợi. Kết nối TCP chạy ở luồng riêng nên cửa sổ không bị đơ khi server không trả lời.

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Kịch bản: B (620 Hz), C (880 Hz), A (440 Hz, giả lập mất 5 % gói gửi) vào phòng; A ghi những gì nghe được. Server lần lượt: tắt mic tất cả (2,5 s) → bật lại (3,5 s) → tắt mic B và đặt độ lợi C = 2 (4,5 s) → B xin bật mic, **bị từ chối** → bật mic B và đặt âm lượng chung 0,5 (5,5 s); A tự đặt âm lượng nghe 0,5 (≈ 6,5 s); C tự tắt mic (≈ 7,5 s); server "bật mic tất cả" (8,0 s) nhưng C **vẫn tắt** (quy tắc 3). Kết quả phân tích bản ghi của A khớp đúng giá trị lý thuyết:

| Đoạn | Trạng thái | B 620 Hz | C 880 Hz | Lý thuyết |
|---|---|---|---|---|
| 1, 3 | mọi mic mở | −15,1 | −15,1 | sin 0,25 → −15,1 dBFS |
| 2 | server tắt tất cả | vắng | vắng | |
| 4 | B bị khóa, độ lợi C = 2 | vắng | −9,0 | +6,0 dB |
| 5 | âm lượng chung 0,5 | −21,1 | −15,1 | −6,0 dB (C: 2 × 0,5 = 1) |
| 6 | A tự đặt âm lượng nghe 0,5 | −27,1 | −21,1 | thêm −6,0 dB |
| 7, 8 | C tự tắt; server bật tất cả | −27,1 | vắng | C vẫn tắt |

Âm 440 Hz của chính A vắng ở mọi đoạn – mix-minus đúng. Bản Rust đo đúng các giá trị trên ở mọi đoạn (xem `ket_qua.txt`). Server đếm được 14 gói mất trong luồng của A, đúng bằng số gói A giả lập làm rơi (bộ sinh ngẫu nhiên xorshift của bản Rust khác `random.Random` của Python nên số gói rơi khác bản Python), các khung thiếu được thay bằng khoảng lặng. Jitter trên máy cục bộ khoảng 0,5–2 ms; trong lần chạy này A cạn bộ đệm 1 lần nhưng không rơi vào đoạn đo nên mức vẫn đúng tới 0,1 dB. Nếu lần cạn rơi vào giữa một đoạn, đoạn đó có thêm vài chục ms im lặng và đo thấp hơn vài phần mười dB. Mức nghe in mỗi giây là trung bình của cả giây nên thay đổi dần qua các mốc; bảng phân tích bỏ 0,15 s đầu mỗi đoạn để loại độ trễ (≈ 80 ms: bộ đệm jitter + một nhịp trộn).

**Kết quả ứng dụng giao diện** (cửa sổ 6 trong `ket_qua.txt`, ảnh `ket_qua_gui.png`): chạy trong container Linux, không có card âm thanh, cửa sổ vẽ bằng OpenGL phần mềm (Mesa llvmpipe) dưới `xvfb-run`; mọi micro là **giả lập**, loa "không có – chỉ đo mức". Kịch bản: Start Server → A, B, C Connect → Start Mic (sin 440/620/880 Hz) và Start Audio → server Start Mic (330 Hz, Mic Volume 50 %) và bật Nghe → server Khóa B → B Stop Mic rồi Start Mic, **bị từ chối** → server kéo độ lợi C = 150 % (C hiện "Server Vol: 150%"), A kéo âm lượng nghe 50 %. Mức đo trên thanh đo khớp lý thuyết (cộng công suất các âm khác tần số):

| Thanh đo | Thành phần | Đo được | Lý thuyết |
|---|---|---|---|
| A nghe | (C × 1,5 + server × 0,5) × 0,5 | −17,1 | −17,1 |
| B nghe | A + C × 1,5 + server × 0,5 | −9,6 | −9,6 |
| C nghe | A + server × 0,5 (B bị khóa) | −14,0 | −14,1 |
| Mic server | sin 330 Hz × 0,5 | −21,0 | −21,1 |
| Server nghe | A + C × 1,5 (không gồm tiếng server) | −9,9 | −9,9 |

Micro và loa thật (cpal) chưa chạy được trong container; cần thử trên máy có card âm thanh.

## Ghi chú
- Thời gian và số gói có thể chênh vài đơn vị giữa các lần chạy (lịch luồng của hệ điều hành); mức dBFS trong bảng phân tích ổn định tới 0,1 dB.
- Luồng trộn chạy theo đồng hồ của server, luồng gửi theo đồng hồ của từng client; trên máy thật hai đồng hồ trôi lệch nhau (clock drift) nên bộ đệm sẽ dần đầy hoặc cạn – hệ thống thực tế co giãn nhẹ âm thanh để bù.
- Hệ thống thực tế nén âm thanh (Opus, RFC 6716 – khoảng 16–32 kbit/s cho thoại), mã hóa và xác thực gói (SRTP, RFC 3711), khử vọng, tự động cân bằng mức; bài giữ PCM không nén để dễ quan sát. Không có SRTP, bất kỳ ai trên đường truyền đều nghe được và có thể giả gói với SSRC của người khác – kiểm tra địa chỉ nguồn chỉ là lớp phòng vệ tối thiểu.
- Làm tròn mẫu bằng `f64::round_ties_even()` (nửa về số chẵn) giống `round()` của Python, nên âm sin tổng hợp và kết quả trộn trùng từng mẫu với bản Python.
- Giao thức giống hệt bản Python và Go: đã thử client Rust (B) nối vào server Python cùng client Python (A); bộ phân tích Rust và Python đọc cùng tệp WAV cho cùng kết quả.
- Kiểm thử: Linux, Rust 1.97.0 (không có thiết bị âm thanh); giao diện dưới Xvfb + Mesa llvmpipe.
- Thay đổi lõi server cho bản giao diện (tách `MayChu`, luồng nền có cờ dừng, gửi `do_loi`) không đổi đầu ra dòng lệnh: chạy lại `tools/demo.py`, phần dòng lệnh của `ket_qua.txt` chỉ khác ở cổng ngẫu nhiên, jitter và độ lệch 0,1 dB/0,02 s của số đo.

## Bài tập gợi ý
1. Đổi `--dem` của client thành 1 và chạy kèm `--mat-goi 0.1`; đếm số lần cạn bộ đệm và nghe thử bản ghi. Giải thích sự đánh đổi giữa độ trễ và độ ổn định.
2. Đặt `do_loi C 5`: quan sát số mẫu cắt ngọn trong `trang_thai`/tổng kết và đo hài bậc 3 (2640 Hz) bằng `phan_tich_wav.py ... 2640:H3`. Cài đặt giới hạn mềm (soft limiter) thay cho cắt cứng.
3. Thay phát khoảng lặng khi mất gói bằng lặp lại khung trước (packet loss concealment) và so sánh mức đo được của âm bị mất gói.
4. Thêm lệnh server `chi_nghe <tên>` (chế độ người nghe) và lệnh client `xin_phat_bieu` (giơ tay) – server duyệt rồi mới gỡ khóa.
5. Nâng cao: thêm gói báo cáo kiểu RTCP (RFC 3550 mục 6.4) để mỗi client định kỳ báo cho server tỉ lệ mất và jitter của luồng mình nhận; server hiển thị trong `trang_thai`.
6. Dùng lại `thiet_bi.rs` để thêm `--mic`/`--loa` cho client dòng lệnh `src/khach.rs`.
7. Giao diện: thêm nút "Giơ tay" ở client (thông điệp mới `gio_tay`) và cột "Giơ tay" ở bảng server; server bấm Gỡ khóa thì cờ tự xóa. Chỉ luồng giao diện được sửa điều khiển – thông điệp đến từ luồng đọc TCP phải đi qua trạng thái chung.
8. Giao diện: vẽ đồ thị mức âm 10 giây gần nhất của từng client (egui `Painter::line_segment` hoặc crate `egui_plot`) và đánh dấu thời điểm mỗi sự kiện điều khiển.

## Đọc thêm
- RFC 3550 – *RTP: A Transport Protocol for Real-Time Applications* (header RTP mục 5.1; ước lượng jitter mục 6.4.1; tính số gói mất phụ lục A.3).
- RFC 3551 – *RTP Profile for Audio and Video Conferences with Minimal Control* (payload type động, định dạng L16 mục 4.5.11).
- RFC 7667 – *RTP Topologies* (Topo-Mixer – server trộn như bài này; Topo-SFU).
- RFC 768 – *User Datagram Protocol*.
- G. Goertzel, “An Algorithm for the Evaluation of Finite Trigonometric Series”, *The American Mathematical Monthly*, 65(1), 1958, tr. 34–37.
- Định dạng WAV (RIFF): Microsoft/IBM, *Multimedia Programming Interface and Data Specifications 1.0*, 1991 (khối `fmt `, `data`); crate [cpal](https://docs.rs/cpal) cho micro/loa thật.
- Tài liệu [egui](https://docs.rs/egui/0.29.1) / [eframe](https://docs.rs/eframe/0.29.1) (immediate mode GUI, `ViewportCommand::Screenshot`).

Mã nguồn của bài được viết mới cho kho này.
