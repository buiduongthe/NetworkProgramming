# Bài mở rộng MR6.14 – Đặt vé xe khách theo chặng: xe giường nằm 40 chỗ, xe ghế 50 chỗ (Rust)

| Thông tin | Nội dung |
|---|---|
| Loại | **Bài mở rộng**, không có trong giáo trình và không có mã giả tương ứng. Bài thứ chín của nhóm "tranh chấp tài nguyên" MR6.6–MR6.14, dùng chung khung của [MR6.6](../BaiMR6.6_DauGia/) và mở rộng [MR6.8](../BaiMR6.8_DatVePhim/) (giữ chỗ có thời hạn) |
| Liên hệ với giáo trình | Chương 6: Bài 6.1 (server đa luồng, một luồng mỗi kết nối), Bài 6.7 (kiểm thử tải); Chương 3: Bài 3.2 (định khung thông điệp theo dòng); bài mở rộng MR5.4/MR5.5 (luồng, đồng bộ) |
| Thư viện | Thư viện chuẩn Rust: `std::net`, `std::thread`, `std::sync` (`Mutex`, `RwLock`, `Condvar`, `AtomicU64`, `mpsc`), `std::time::Instant`; crate `serde_json` 1.0.154 (`preserve_order`), `socket2` 0.5.10, `ctrlc` 3.4.7, `chrono` 0.4.41, `miniz_oxide` 0.8.9 (PNG). Ứng dụng giao diện: `eframe`/`egui` 0.29.1 (ô chọn đoạn là `ComboBox`). Mọi crate ghim bằng `=` và `Cargo.lock` |
| Bản ngôn ngữ khác | [Python](../../../python/Chuong06/BaiMR6.14_DatVeXeKhach/), [C#](../../../csharp/Chuong06/BaiMR6.14_DatVeXeKhach/), [Java](../../../java/Chuong06/BaiMR6.14_DatVeXeKhach/), [Go](../../../go/Chuong06/BaiMR6.14_DatVeXeKhach/) |

## Mục tiêu
Bài mô phỏng một ứng dụng đặt vé xe khách đường dài. Chuyến xe chạy qua **4 trạm**: Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang, tức **3 chặng**. Có hai loại xe, đổi được khi đặt lại:
- **xe giường nằm 40 chỗ** (`giuong`): hai tầng, tầng dưới D01–D20, tầng trên T01–T20, mỗi tầng 3 dãy × 7 hàng (hàng cuối 2 giường);
- **xe ghế ngồi 50 chỗ** (`ghe`): ghế 01–50, 11 hàng × 4 ghế có lối đi ở giữa, hàng cuối 6 ghế liền.

Khách chọn **một đoạn đường** (đi từ trạm `tu` đến trạm `den`), chọn chỗ rồi **giữ chỗ** có thời hạn, sau đó **thanh toán** hoặc **nhả**. Khác với rạp chiếu phim ở MR6.8, **một chỗ bán được cho nhiều người** nếu các đoạn đường không chồng nhau: An mua giường D01 Sài Gòn → Đà Lạt, Chi mua đúng giường đó Đà Lạt → Nha Trang. Vì vậy tài nguyên không phải là "ghế" mà là **cặp (chỗ, chặng)**, tên ô `D01#1` (chặng 1 = Bảo Lộc → Đà Lạt). Giữ 2 giường cho cả tuyến là khóa 6 ô. Mọi thao tác là "đọc → kiểm tra (giả lập độ trễ cơ sở dữ liệu 0–2 ms) → ghi", xử lý theo **bốn cách**: không khóa, khóa loại trừ theo từng ô, lạc quan, hàng đợi một luồng. **Sổ kiểm toán** đếm (chỗ, chặng) bị giữ/bán cho hai người và thanh toán sai. **Bộ mô phỏng tải** chạy hai tình huống: 500 người cùng giữ giường "nóng" D01 với đoạn ngẫu nhiên, và 300 người × 3 lần thử chọn 1–2 chỗ ngẫu nhiên cho đoạn ngẫu nhiên.

Phần khung (`src/chung.rs`, `src/khach.rs`, `src/mo_phong.rs`, `src/server.rs`, `src/gui_chung.rs`) chép nguyên từ MR6.6. Bài này chỉ viết nghiệp vụ, người đặt vé dòng lệnh, ứng dụng giao diện, bảng tài nguyên của máy chủ và kịch bản demo.

## Kiến thức chính
Số liệu dưới đây lấy từ `ket_qua.txt`, là một lần chạy thật trên máy cục bộ (127.0.0.1, Rust 1.97, bản release, độ trễ xử lý ngẫu nhiên 0–2 ms). Mỗi lần chạy, số liệu lệch đi đôi chút.

```
500 người cùng giữ giường nóng D01, đoạn ngẫu nhiên (giữ được thì thanh toán ngay):
  Chế độ       Thành công Sai phạm Xung đột  yc/s  p99 ms
  khong_khoa            6        5        0 10927    10.6
  khoa                  6        0        0   962   513.1
  lac_quan              4        0       11 13114    32.8
  hang_doi              6        0        0   878   560.9
300 người × 3 lần thử, 1–2 chỗ và đoạn ngẫu nhiên (40 giường × 3 chặng = 120 ô):
  Chế độ       Thành công Sai phạm Xung đột  yc/s  p99 ms
  khong_khoa          155       53        0 25762     5.3
  khoa                156        0        0  4138   180.5
  lac_quan            162        0       20 28665     6.3
  hang_doi            132        0        0   851   352.5
```
("Thành công" đếm cả trả lời giữ chỗ lẫn thanh toán được chấp nhận; "Sai phạm" do sổ kiểm toán đếm; "Xung đột" là số lần so sánh-và-đổi thất bại; "yc/s" và "p99" (khứ hồi) đo ở bộ mô phỏng.)

- **Tài nguyên là (chỗ, chặng), không phải chỗ.** Với giường nóng D01 và đoạn ngẫu nhiên, ở ba chế độ đúng D01 bán được 3, 2 và 3 vé (`khoa`, `lac_quan`, `hang_doi`; mỗi vé là 2 trả lời thành công: giữ rồi thanh toán). Số vé tùy đoạn của những người đến trước: một vé Sài Gòn → Bảo Lộc và một vé Bảo Lộc → Nha Trang không chạm chung chặng nào. Ở rạp chiếu phim MR6.8, ghế nóng E7 chỉ bán được 1 vé. Nếu máy chủ coi cả giường là một ô, nhà xe sẽ bỏ phí các chặng còn trống.
- **Tranh chấp (race condition) trên từng chặng.** Ở `khong_khoa`, thao tác giữ chỗ "đọc các ô → chờ → ghi" không đồng bộ, nên nhiều người cùng được báo "đã giữ" cho cùng một (chỗ, chặng) và lượt ghi sau đè lượt ghi trước: 5 sai phạm với giường nóng, 53 sai phạm với nhiều chỗ. Sổ kiểm toán ghi từng ô, ví dụ `D02 (Sài Gòn–Bảo Lộc) giữ cho Bình khi An đang giữ`. Một lượt giữ cả tuyến bị đè ở 3 chặng thì sổ đếm 3 sai phạm.
- **Khóa theo từng ô cho song song thật.** Ở `khoa`, mỗi lần giữ khóa đúng các ô `chỗ#chặng` mình chạm vào, theo thứ tự tên ô (không khóa chết). Hai đoạn không chồng nhau của cùng một giường chạm các ô khác nhau, nên chạy song song. Ở tình huống nhiều chỗ, `khoa` đạt **4138 yc/s** so với **851 yc/s** của `hang_doi`: hàng đợi một luồng xử lý lần lượt dù các yêu cầu không đụng nhau. Ngược lại, với giường nóng D01, mọi yêu cầu chạm chung ô D01#… nên `khoa` cũng chỉ còn 962 yc/s.
- **Lạc quan với nhiều ô.** `lac_quan` đọc (giá trị, phiên bản) của mọi ô trong yêu cầu, quyết định ngoài khóa, rồi chỉ ghi nếu không ô nào đổi phiên bản. Đoạn càng dài thì càng nhiều ô có thể bị người khác đổi, nên xung đột tăng theo độ dài đoạn (lần chạy này: 11 xung đột với giường nóng, 20 với nhiều chỗ). Mỗi người ảo chỉ thử `--lan` lần, nên mỗi lần xung đột cũng tiêu một lượt thử của khách.
- **Giữ chỗ có thời hạn (TTL) theo từng ô.** Như MR6.8: hạn tính theo đồng hồ đơn điệu, luồng nền dọn ô quá hạn mỗi 100 ms, và lượt giữ quá hạn coi như trống ngay cả khi chưa dọn. Lượt chạy dòng lệnh (giữ chỗ 3 s) cho thấy:
  - An giữ D01 Sài Gòn → Đà Lạt, Chi giữ và mua được D01 Đà Lạt → Nha Trang ngay lúc đó (mã vé `XK0730-D01-DLNT`).
  - Bình giữ D01 Sài Gòn → Bảo Lộc khi An còn hạn thì bị từ chối "chỗ D01 chặng Sài Gòn–Bảo Lộc đang có người giữ". Hết hạn, Bình giữ và mua được (`XK0730-D01-SGBL`). Lệnh `xem` của Bình in `D01:V.b`: chặng 1 là vé của Bình, chặng 2 trống, chặng 3 đã bán cho người khác.
  - An thanh toán muộn thì nhận "TỪ CHỐI: giữ chỗ D01 đã hết hạn". Máy chủ ưu tiên báo "hết hạn" khi còn dấu vết lượt giữ của An ở một chặng nào đó, dù chặng đầu đã là của Bình.
- **Ảnh chụp trạng thái có số thứ tự.** Như các bài trước, mỗi lần ghi được đánh số (`kq["phien"]`), và client chỉ kết luận "chỗ đã bị lấy" hoặc "đã hết hạn" từ bản `cap_nhat` có `phien` ≥ số của lần giữ của mình.

### Giao thức (giống hệt ở 5 ngôn ngữ)
Khung chung (chào, quản trị, `kq`, `cap_nhat`, `su_kien`, `thong_ke`) xem [README MR6.6](../BaiMR6.6_DauGia/README.md#giao-thức-giống-hệt-ở-5-ngôn-ngữ). Phần riêng của bài:

| Chiều | Thông điệp |
|---|---|
| người dùng → server | `{"loai":"giu_cho","ma_yc":k,"ghe":["D01","D02"],"tu":0,"den":2}`, `{"loai":"thanh_toan",…}`, `{"loai":"nha",…}` cùng trường (1–6 mã chỗ của loại xe hiện tại, không trùng; `0 ≤ tu < den ≤ 3` là chỉ số trạm). Kết nối gộp thêm `"nguoi":"u17"` |
| quản trị → server | `{"loai":"dat_lai","ma_yc":k,"ttl_s":30,"loai_xe":"giuong"\|"ghe","ghe_nong":"D01"\|"","chuyen":"07:30","nha_xe":…}` (đổi loại xe mà ghế nóng không thuộc xe mới thì ghế nóng về chỗ đầu: D01 hoặc 01) |
| server → client | giữ: `{"loai":"kq","ok":true,"ghe","tu","den","het_han_ms":30000,"tien","nguoi","phien"}`; thanh toán: `{"ok":true,"ghe","tu","den","ma_ve":["XK0730-D01-SGDL"],"tien","nguoi","phien"}`; nhả: `{"ok":true,"ghe","tu","den","nguoi","phien"}`; từ chối: `"ma_loi":"dang_giu"\|"da_ban"\|"khong_giu"\|"het_han"\|"xung_dot"\|"sai_du_lieu"\|…` (giữ thất bại kèm `"ghe_ban":[…]`, `"tu"`, `"den"`) |
| server → tất cả | `{"loai":"cap_nhat","che_do","phien","nha_xe","chuyen","loai_xe","ten_xe","tram":[…],"gia_chang":[220000,130000,170000],"ttl_s","ghe_nong","ghe":["D01",…],"bo_tri":[[tầng,hàng,cột],…],"so_do":["b.g",…] (mỗi chỗ một chuỗi 3 ký tự, một ký tự mỗi chặng: . trống, g đang giữ, b đã bán),"chu":{"D01#0":"An",…},"con_lai_ms":{"D01#0":27000,…},"dem":{"trong","giu","ban"}}` (đếm theo ô chỗ-chặng); `su_kien` cho người dùng thật: "An giữ D01 (Sài Gòn → Đà Lạt, 30 s)", "Chi mua 1 vé D01 Đà Lạt → Nha Trang (170.000 đ)", "Hết hạn giữ D01 (Bảo Lộc–Đà Lạt) (An), chỗ trống lại" |

Giá mỗi chặng là **giá minh họa** (giường 220.000 / 130.000 / 170.000 đ, ghế 170.000 / 100.000 / 130.000 đ); giá một vé là tổng giá các chặng của đoạn. Sai phạm trong `thong_ke`: `trùng chỗ`, `thanh toán sai`; `tom_tat`: `Bán 118/120 chỗ-chặng, giữ 0, sổ 101 vé`.

## Cấu trúc tệp
Gói Cargo `mr6_14_dat_ve_xe_khach` (một chương trình, chọn vai trò bằng lệnh con):
- **Khung chung (chép nguyên từ MR6.6):** `src/chung.rs`, `src/khach.rs`, `src/mo_phong.rs`, `src/server.rs`, `src/gui_chung.rs`.
- **Nghiệp vụ:** `src/may_chu.rs` (trạm, giá chặng, `bo_tri(loai_xe)` – sơ đồ hai loại xe dùng chung cho máy chủ và giao diện; `DatVeXe: NghiepVu`, ô `Cho`, `LuatDatVeXe: LuatKiemToan`, dọn ô quá hạn trong `dinh_ky`, `ghe_nong`), `src/nguoi_ao.rs` (`NguCanh`: ô bận theo những gì kết nối biết, chọn đoạn và chỗ trống; `NguoiAo`: lần đầu giữ ghế nóng cho đoạn ngẫu nhiên, giữ được thì thanh toán, bị từ chối thì đổi đoạn và chỗ), `src/dat_ve_xe.rs` (người đặt vé dòng lệnh, in sơ đồ xe theo chặng).
- **Giao diện:** `src/dat_ve_xe_gui.rs` (sơ đồ xe `SoDoXe` dùng chung với máy chủ, băng rôn nhà xe, chọn đoạn, thanh tuyến, ứng dụng đặt vé), `src/server_gui.rs` (khung "Tài nguyên"), `src/demo_gui.rs`, `src/main.rs`.
- `Cargo.toml`, `Cargo.lock`, `demo.json`, `ket_qua.txt`, `ket_qua.png`, `ket_qua_gui.png`, `ket_qua_gui_tranh_chap.png`; `*_windows.*`: kết quả chạy trên Windows.

## Cách chạy
Trong thư mục bài (`cargo build -q --release` một lần; `B=../../target/release/mr6_14_dat_ve_xe_khach`, Windows: `..\..\target\release\mr6_14_dat_ve_xe_khach.exe`):
```bash
$B server 127.0.0.1 9000 [--ttl 30] [--loai-xe giuong] [--ghe-nong D01]   # cửa sổ 1 (gõ lệnh ở đây)
$B mo_phong 127.0.0.1 9000 --n 500 --so-sanh              # 500 người giữ giường nóng D01, đoạn ngẫu nhiên
$B khach 127.0.0.1 9000 dat_lai ghe_nong=                 # bỏ ghế nóng: chọn ngẫu nhiên
$B mo_phong 127.0.0.1 9000 --n 300 --lan 3 --so-sanh      # nhiều chỗ
$B khach 127.0.0.1 9000 dat_lai loai_xe=ghe ttl_s=10      # xe ghế 50 chỗ
$B dat_ve_xe An 127.0.0.1 9000                            # gõ giu SG DL D01 / tt / nha / xem / thoat
```
- Lệnh máy chủ, quản trị, tùy chọn `--che-do`, `--tre-ms` như MR6.6; tham số đặt lại `ttl_s` (1–600), `loai_xe` (`giuong`/`ghe`), `ghe_nong` (mã chỗ của loại xe hoặc rỗng), `chuyen`, `nha_xe`. Lệnh của `dat_ve_xe`: `giu SG DL D01 [D02 …]` (hai mã trạm `SG`, `BL`, `DL`, `NT` rồi các chỗ), `tt`, `nha` (các chỗ vừa giữ), `xem` (mỗi chỗ 3 ký tự, một ký tự mỗi chặng: `.` trống, `g` người khác giữ, `b` đã bán, `G` mình giữ, `V` vé của mình), `thoat`.
- Tái lập kết quả từ thư mục gốc của kho: `python3 tools/demo.py rust/Chuong06/BaiMR6.14_DatVeXeKhach` (cổng 44780, giao diện 44782).

## Ứng dụng giao diện
![Ứng dụng giao diện](ket_qua_gui.png)

Ảnh trên là lần chạy thật của `demo_gui` (Linux, Xvfb + Mesa llvmpipe, egui 0.29.1): bảng điều khiển máy chủ (trên trái), An (trên phải), Bình và Chi (dưới). Kịch bản đã chạy so sánh 4 chế độ ngay trong giao diện (500 người giữ giường nóng D01, cùng tiến trình với cửa sổ; lần chạy này `khong_khoa` không lộ sai phạm nào (4 thành công / 0 sai phạm, như `khoa` và `hang_doi`; `lac_quan` 6 / 0) – tranh chấp phụ thuộc thời điểm các luồng xen nhau, bảng dòng lệnh ở trên mới cho thấy sai phạm), rồi đặt lại thành **xe ghế 50 chỗ**, giữ chỗ 6 s. An giữ ghế 05, 06 Sài Gòn → Bảo Lộc mà không thanh toán. Bình chọn Sài Gòn → Đà Lạt, bấm 06 thì thấy "Chỗ 06 đang có người giữ trên đoạn này". Hết hạn, Bình giữ lại 05, 06 Sài Gòn → Đà Lạt và mua được (540.000 đ, xanh dương trong ứng dụng của Bình). Chi mua **đúng ghế 05** cho Đà Lạt → Nha Trang (130.000 đ). Lúc chụp, An đang giữ 10, 11 cả tuyến (xanh lá, đồng hồ 00:05, nút "THANH TOÁN 800.000 đ"), còn Chi đang chọn 21. Dải vạch nhỏ ở chân mỗi chỗ cho thấy từng chặng: ghế 05 có hai vạch xanh dương và một vạch xám trong cửa sổ của Bình.

![Tranh chấp với người dùng thật](ket_qua_gui_tranh_chap.png)

Ở bước trước đó, xe giường nằm và máy chủ ở `khong_khoa` với độ trễ 0–20 ms. Cả ba chọn D02 Sài Gòn → Nha Trang rồi cùng bấm "GIỮ CHỖ", và cả ba đều nhận trả lời "ĐÃ GIỮ D02". Bản cập nhật kế tiếp cho thấy cả 3 chặng của D02 thuộc Chi, hai người kia thấy "Chỗ D02 đã bị Chi lấy mất!". Ô **SAI PHẠM** chuyển đỏ (trùng chỗ 6 = 2 người bị đè × 3 chặng). Ở bước 2 (chế độ `khoa`) An và Bình cùng giữ D01 Sài Gòn → Đà Lạt, còn Chi giữ D01 Đà Lạt → Nha Trang: Bình giữ được, An nhận "Không giữ được: chỗ D01 chặng Sài Gòn–Bảo Lộc đang có người giữ", và Chi cũng giữ được; Bình trả 350.000 đ, Chi 170.000 đ. Cùng một giường bán hai vé.

- **Máy chủ** (`server_gui`): phần chung giống MR6.6; **tài nguyên**: sơ đồ xe thu nhỏ (mỗi chỗ có dải 3 vạch theo chặng, ghế nóng viền đỏ), số ô trống – đang giữ – đã bán, số chỗ đã bán theo từng chặng, ghế nóng đang thuộc ai ở chặng nào, ô **Giữ chỗ (s)**, **Loại xe** (`giuong`/`ghe`), **Ghế nóng** và nút **Đặt lại**.
- **Đặt vé** (`dat_ve_xe_gui`): băng rôn nhà xe (tên, chuyến, loại xe, thời hạn giữ chỗ); chọn **Đi từ** và **Đến**, thanh tuyến 4 trạm tô đoạn đã chọn kèm giá từng chặng; **sơ đồ xe** có vô lăng ở đầu xe (xe giường: tầng dưới, tầng trên đặt cạnh nhau; xe ghế: lối đi giữa, hàng cuối 6 ghế); màu theo đoạn đang chọn: **trống**, **đang chọn**, **người khác giữ**, **đã bán**, **bạn giữ**, **vé của bạn**, kèm chú giải và **dải vạch theo chặng** ở chân mỗi chỗ; bấm chỗ để chọn/bỏ chọn (tối đa 6); "Chỗ đã chọn", tạm tính theo đoạn; nút **GIỮ CHỖ**; **đồng hồ giữ chỗ** đếm ngược (đỏ khi còn dưới 10 s); nút **THANH TOÁN** ghi tổng tiền; nút **Nhả chỗ**; khi đang giữ chỗ thì khóa ô chọn đoạn; dòng thông báo; "Vé của bạn" (chỗ và đoạn, ví dụ `05 SG→DL`). Sơ đồ đổi ngay khi người khác giữ hoặc mua (≤ 10 cập nhật/giây).


### Cách chạy
```bash
cargo build --release
$B server_gui 127.0.0.1 44782                      # bấm START SERVER (giữ chỗ 30 s, xe giường nằm)
$B dat_ve_xe_gui An 127.0.0.1 44782
$B dat_ve_xe_gui Bình 127.0.0.1 44782
```
- **Windows:** Rust bản MSVC như các bài Rust khác; giao diện không cần cài thêm gì. Kịch bản tự động (Linux, cần `xvfb`): `xvfb-run -a -s "-screen 0 2600x1700x24" $B demo_gui 127.0.0.1 44782`.
- Chạy riêng từng cửa sổ: nhật ký (tiền tố `[server]`, `[An]`) và cứ 5 giây một dòng số liệu cũng in ra dòng lệnh.
- Cửa sổ người dùng nối được với máy chủ của mọi ngôn ngữ (và ngược lại). Thử trên nhiều máy: máy chủ nghe IP card mạng (hoặc `0.0.0.0`), mở tường lửa cho cổng, **chỉ dùng `khoa`, `lac_quan`, `hang_doi`**.

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

## Ghi chú
- **Chạy chéo đã thử** (500 người cùng giữ giường nóng D01, so sánh 4 chế độ; thứ tự `khong_khoa` / `khoa` / `lac_quan` / `hang_doi`, mỗi cặp là thành công / sai phạm): bộ mô phỏng Python → máy chủ C#: 93 / 134, 6 / 0, 6 / 0 (10 xung đột), 4 / 0; C# → Python: 9 / 5, 4 / 0, 4 / 0, 6 / 0; Java → Rust: 7 / 1, 4 / 0, 4 / 0, 6 / 0; Rust → Go: 27 / 36, 6 / 0, 4 / 0, 4 / 0; Go → Java: 20 / 23, 6 / 0, 4 / 0, 6 / 0. Bốn người dùng dòng lệnh Python (An), Java (Chi), Rust (Bình) và Go (Dũng, giữ và mua T05, T06 Bảo Lộc → Nha Trang, 600.000 đ) cùng chạy kịch bản giữ chỗ 3 s trên máy chủ C#: diễn biến giống hệt lượt chạy một ngôn ngữ, sổ kiểm toán 0 sai phạm, `Bán 6/120 chỗ-chặng, giữ 0, sổ 4 vé`.
- Hàm kiểm tham số của khung là hàm tĩnh, nên loại xe hiện tại (để kiểm ghế nóng khi đặt lại mà không đổi loại xe) được giữ trong một `Mutex<String>` toàn cục, đặt lại ở `khoi_tao`. Sơ đồ chỗ của loại xe hiện tại nằm trong `Mutex<Vec<ChoXe>>` của `DatVeXe`.
- Ở tình huống nhiều chỗ, `khong_khoa` kết thúc với "Bán 103/120 chỗ-chặng, giữ 16, sổ 85 vé": 16 ô bị "giữ mồ côi" bởi những người đã bị đè (họ thanh toán thì bị từ chối), và các ô này chỉ trống lại sau 30 s. Ba chế độ đúng bán được 118–119/120 chỗ-chặng (88–103 vé).
- Tham số đặt lại của bộ mô phỏng rỗng: `--so-sanh` đặt lại sơ đồ xe mà vẫn giữ loại xe, thời hạn và ghế nóng hiện tại của máy chủ.
- Trong kịch bản giao diện, máy chủ, bộ mô phỏng và cửa sổ cùng chạy trong một tiến trình, nên yc/s thấp hơn bản dòng lệnh.
- Kiểm thử: Linux, Rust 1.97.0, tools/demo.py; giao diện egui 0.29.1 dưới Xvfb + Mesa llvmpipe, và Windows (ảnh `*_windows.png`).

## Bài tập gợi ý
1. Đo số vé bán được cho giường nóng D01 khi máy chủ coi **cả giường là một ô** (sửa `khoi_tao` để mỗi chỗ chỉ có ô `D01#0`, và `o_yc` luôn là ô đó). So với cách chia theo chặng, nhà xe mất bao nhiêu doanh thu trong tình huống 300 người × 3 lần?
2. Chạy tình huống nhiều chỗ với đoạn luôn là cả tuyến (`NguCanh::doan` trả về `(0, 3)`). Thông lượng của `khoa` và số xung đột của `lac_quan` thay đổi thế nào? Giải thích bằng số ô mỗi yêu cầu chạm vào.
3. Thêm thao tác `doi_cho` (đổi từ chỗ đang giữ sang chỗ khác, nguyên tử: chỉ nhả chỗ cũ khi giữ được chỗ mới). Những ô nào phải khóa cùng lúc? Sắp thứ tự thế nào để không khóa chết?
4. Thêm luật "giường tầng dưới ưu tiên người cao tuổi": người dùng mang cờ `uu_tien` mới giữ được D01–D20 trong 10 phút đầu mở bán. Luật này đặt ở `xu_ly_yeu_cau` hay trong hàm thuần truyền cho `thuc_hien`? Vì sao?

## Đọc thêm
- H. T. Kung, J. T. Robinson (1981). *On Optimistic Methods for Concurrency Control*. ACM Transactions on Database Systems 6(2).
- E. G. Coffman, M. J. Elphick, A. Shoshani (1971). *System Deadlocks*. ACM Computing Surveys 3(2): điều kiện khóa chết và cách phòng bằng thứ tự khóa (giữ nhiều chỗ cho nhiều chặng = khóa nhiều ô theo thứ tự tên ô).
- Tài liệu Rust: [`std::sync`](https://doc.rust-lang.org/std/sync/index.html), [`std::sync::atomic`](https://doc.rust-lang.org/std/sync/atomic/index.html), [`std::time::Instant`](https://doc.rust-lang.org/std/time/struct.Instant.html); egui: [`ComboBox`](https://docs.rs/egui/0.29.1/egui/containers/combo_box/struct.ComboBox.html), [`Painter`](https://docs.rs/egui/0.29.1/egui/struct.Painter.html).

Mã nguồn của bài được viết mới cho kho này.
