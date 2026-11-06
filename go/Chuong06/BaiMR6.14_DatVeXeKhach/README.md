# Bài mở rộng MR6.14 – Đặt vé xe khách theo chặng: xe giường nằm 40 chỗ, xe ghế 50 chỗ (Go)

| Thông tin | Nội dung |
|---|---|
| Loại | **Bài mở rộng**, không có trong giáo trình và không có mã giả tương ứng. Bài thứ chín của nhóm "tranh chấp tài nguyên" MR6.6–MR6.14, dùng chung khung của [MR6.6](../BaiMR6.6_DauGia/) và mở rộng [MR6.8](../BaiMR6.8_DatVePhim/) (giữ chỗ có thời hạn) |
| Liên hệ với giáo trình | Chương 6: Bài 6.1 (server đa luồng, một luồng mỗi kết nối), Bài 6.7 (kiểm thử tải); Chương 3: Bài 3.2 (định khung thông điệp theo dòng); bài mở rộng MR5.4/MR5.5 (luồng, đồng bộ) |
| Thư viện | Bản dòng lệnh: chỉ thư viện chuẩn Go (`net`, `sync`, `sync/atomic`, `encoding/json`, `math/rand/v2`). Ứng dụng giao diện (`gui/`, mô-đun riêng): Fyne v2.6.3 (cần cgo/gcc; ô chọn đoạn là `widget.Select`, sơ đồ xe vẽ thành ảnh) |
| Bản ngôn ngữ khác | [Python](../../../python/Chuong06/BaiMR6.14_DatVeXeKhach/), [C#](../../../csharp/Chuong06/BaiMR6.14_DatVeXeKhach/), [Java](../../../java/Chuong06/BaiMR6.14_DatVeXeKhach/), [Rust](../../../rust/Chuong06/BaiMR6.14_DatVeXeKhach/) |

## Mục tiêu
Bài mô phỏng một ứng dụng đặt vé xe khách đường dài. Chuyến xe chạy qua **4 trạm**: Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang, tức **3 chặng**. Có hai loại xe, đổi được khi đặt lại:
- **xe giường nằm 40 chỗ** (`giuong`): hai tầng, tầng dưới D01–D20, tầng trên T01–T20, mỗi tầng 3 dãy × 7 hàng (hàng cuối 2 giường);
- **xe ghế ngồi 50 chỗ** (`ghe`): ghế 01–50, 11 hàng × 4 ghế có lối đi ở giữa, hàng cuối 6 ghế liền.

Khách chọn **một đoạn đường** (đi từ trạm `tu` đến trạm `den`), chọn chỗ rồi **giữ chỗ** có thời hạn, sau đó **thanh toán** hoặc **nhả**. Khác với rạp chiếu phim ở MR6.8, **một chỗ bán được cho nhiều người** nếu các đoạn đường không chồng nhau: An mua giường D01 Sài Gòn → Đà Lạt, Chi mua đúng giường đó Đà Lạt → Nha Trang. Vì vậy tài nguyên không phải là "ghế" mà là **cặp (chỗ, chặng)**, tên ô `D01#1` (chặng 1 = Bảo Lộc → Đà Lạt). Giữ 2 giường cho cả tuyến là khóa 6 ô. Mọi thao tác là "đọc → kiểm tra (giả lập độ trễ cơ sở dữ liệu 0–2 ms) → ghi", xử lý theo **bốn cách**: không khóa, khóa loại trừ theo từng ô, lạc quan, hàng đợi một luồng. **Sổ kiểm toán** đếm (chỗ, chặng) bị giữ/bán cho hai người và thanh toán sai. **Bộ mô phỏng tải** chạy hai tình huống: 500 người cùng giữ giường "nóng" D01 với đoạn ngẫu nhiên, và 300 người × 3 lần thử chọn 1–2 chỗ ngẫu nhiên cho đoạn ngẫu nhiên.

Phần khung (gói `tranhchap/` trừ `nghiepvu.go`, `nguoiao.go`; `gui/chung_gui.go`, `gui/ve.go`) chép nguyên từ MR6.6. Bài này chỉ viết nghiệp vụ, người đặt vé dòng lệnh, ứng dụng giao diện, bảng tài nguyên của máy chủ và kịch bản demo.

## Kiến thức chính
Số liệu dưới đây lấy từ `ket_qua.txt`, là một lần chạy thật trên máy cục bộ (127.0.0.1, Go 1.24, độ trễ xử lý ngẫu nhiên 0–2 ms). Mỗi lần chạy, số liệu lệch đi đôi chút.

```
500 người cùng giữ giường nóng D01, đoạn ngẫu nhiên (giữ được thì thanh toán ngay):
  Chế độ       Thành công Sai phạm Xung đột  yc/s  p99 ms
  khong_khoa           31       41        0 26797    15.9
  khoa                  6        0        0   690   715.3
  lac_quan              2        0       20 33387    14.3
  hang_doi              4        0        0   613   793.1
300 người × 3 lần thử, 1–2 chỗ và đoạn ngẫu nhiên (40 giường × 3 chặng = 120 ô):
  Chế độ       Thành công Sai phạm Xung đột  yc/s  p99 ms
  khong_khoa          124       14        0 15077    23.7
  khoa                148        0        0  3089   211.6
  lac_quan            148        0       27 15814    29.5
  hang_doi            148        0        0   584   560.6
```
("Thành công" đếm cả trả lời giữ chỗ lẫn thanh toán được chấp nhận; "Sai phạm" do sổ kiểm toán đếm; "Xung đột" là số lần so sánh-và-đổi thất bại; "yc/s" và "p99" (khứ hồi) đo ở bộ mô phỏng.)

- **Tài nguyên là (chỗ, chặng), không phải chỗ.** Với giường nóng D01 và đoạn ngẫu nhiên, ở ba chế độ đúng D01 bán được 3, 1 và 2 vé (`khoa`, `lac_quan`, `hang_doi`; mỗi vé là 2 trả lời thành công: giữ rồi thanh toán). Số vé tùy đoạn của những người đến trước: một vé Sài Gòn → Bảo Lộc và một vé Bảo Lộc → Nha Trang không chạm chung chặng nào. Ở rạp chiếu phim MR6.8, ghế nóng E7 chỉ bán được 1 vé. Nếu máy chủ coi cả giường là một ô, nhà xe sẽ bỏ phí các chặng còn trống.
- **Tranh chấp (race condition) trên từng chặng.** Ở `khong_khoa`, thao tác giữ chỗ "đọc các ô → chờ → ghi" không đồng bộ, nên nhiều người cùng được báo "đã giữ" cho cùng một (chỗ, chặng) và lượt ghi sau đè lượt ghi trước: 41 sai phạm với giường nóng, 14 sai phạm với nhiều chỗ. Sổ kiểm toán ghi từng ô, ví dụ `D02 (Sài Gòn–Bảo Lộc) giữ cho Bình khi An đang giữ`. Một lượt giữ cả tuyến bị đè ở 3 chặng thì sổ đếm 3 sai phạm.
- **Khóa theo từng ô cho song song thật.** Ở `khoa`, mỗi lần giữ khóa đúng các ô `chỗ#chặng` mình chạm vào, theo thứ tự tên ô (không khóa chết). Hai đoạn không chồng nhau của cùng một giường chạm các ô khác nhau, nên chạy song song. Ở tình huống nhiều chỗ, `khoa` đạt **3089 yc/s** so với **584 yc/s** của `hang_doi`: hàng đợi một goroutine xử lý lần lượt dù các yêu cầu không đụng nhau. Ngược lại, với giường nóng D01, mọi yêu cầu chạm chung ô D01#… nên `khoa` cũng chỉ còn 690 yc/s.
- **Lạc quan với nhiều ô.** `lac_quan` đọc (giá trị, phiên bản) của mọi ô trong yêu cầu, quyết định ngoài khóa, rồi chỉ ghi nếu không ô nào đổi phiên bản. Đoạn càng dài thì càng nhiều ô có thể bị người khác đổi, nên xung đột tăng theo độ dài đoạn (lần chạy này: 20 xung đột với giường nóng, 27 với nhiều chỗ). Mỗi người ảo chỉ thử `--lan` lần, nên mỗi lần xung đột cũng tiêu một lượt thử của khách.
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
- `tranhchap/`: khung chung (như MR6.6) + `nghiepvu.go` (trạm, giá chặng, `BoTri(loaiXe)` – sơ đồ hai loại xe dùng chung cho máy chủ và giao diện; `MayChuDatVeXe`: giữ chỗ, thanh toán, nhả theo đoạn, dọn ô quá hạn trong `dinhKy`, số thứ tự lần ghi, `GheNong()`; luật sổ kiểm toán), `nguoiao.go` (`nguCanh`: ô bận theo những gì kết nối biết, chọn đoạn và chỗ trống; `nguoiAo`: lần đầu giữ ghế nóng cho đoạn ngẫu nhiên, giữ được thì thanh toán, bị từ chối thì đổi đoạn và chỗ; `ThamSoDatLai = {}`).
- `main.go` (lệnh con `server`, `mo_phong`, `khach`, `dat_ve_xe`), `chung_cli.go`, `server.go` (tùy chọn `--ttl`, `--loai-xe`, `--ghe-nong`), `dat_ve_xe.go` (người đặt vé dòng lệnh, lệnh `xem` in sơ đồ xe theo chặng).
- `gui/`: `chung_gui.go`, `ve.go` (khung chung), `server_gui.go`, `dat_ve_xe_gui.go` (`soDoXe`: sơ đồ xe vẽ thành ảnh, dùng ở cả hai cửa sổ; băng rôn nhà xe, thanh tuyến), `demo_gui.go`, `main.go`.
- `demo.json`, `ket_qua.txt`, `ket_qua.png`: kịch bản và kết quả thực thi thật.

## Cách chạy
Chạy trong thư mục bài (Go 1.24+):
```bash
go build -o out/app .
out/app server 127.0.0.1 9000                                  # cửa sổ 1 (gõ lệnh ở đây)
out/app mo_phong 127.0.0.1 9000 --n 500 --so-sanh              # 500 người cùng giữ giường nóng D01
out/app khach 127.0.0.1 9000 dat_lai ghe_nong=                 # bỏ ghế nóng: chọn chỗ ngẫu nhiên
out/app mo_phong 127.0.0.1 9000 --n 300 --lan 3 --so-sanh
out/app khach 127.0.0.1 9000 dat_lai loai_xe=ghe ttl_s=10      # xe ghế 50 chỗ, giữ chỗ 10 s
out/app dat_ve_xe An 127.0.0.1 9000                            # gõ: giu SG DL D01 / tt / nha / xem
```
- Lệnh của máy chủ: `che_do khong_khoa|khoa|lac_quan|hang_doi`, `tre <ms>`, `dat_lai [ttl_s=… loai_xe=… ghe_nong=… chuyen=… nha_xe=…]`, `thong_ke`, `thoat`. Tùy chọn khi chạy: `--che-do`, `--tre-ms`, `--ttl`, `--loai-xe`, `--ghe-nong`.
- Lệnh của `dat_ve_xe`: `giu SG DL D01 [D02 …]` (hai mã trạm `SG`, `BL`, `DL`, `NT` rồi các chỗ), `tt`, `nha` (các chỗ vừa giữ), `xem`, `thoat`; dòng `@t lệnh` chờ tới giây thứ t.

Muốn tái lập kết quả, chạy từ thư mục gốc của kho: `python3 tools/demo.py go/Chuong06/BaiMR6.14_DatVeXeKhach` (cổng 41785, giao diện 41787).

## Ứng dụng giao diện
![Ứng dụng giao diện](ket_qua_gui.png)

*(Ảnh `ket_qua_gui.png` và `ket_qua_gui_tranh_chap.png` do `demo_gui` tự chụp khi chạy; máy dựng kho không tải được mô-đun Fyne nên bản trong kho chưa có ảnh Linux.)*

Kịch bản `demo_gui` (cùng các bước và dòng `[demo] …` như `demo_gui.py` của bản Python): xe giường nằm, `khoa`: An và Bình cùng giữ D01 Sài Gòn → Đà Lạt, Chi giữ D01 Đà Lạt → Nha Trang – một trong hai người đầu và Chi giữ được, hai người thanh toán (cùng một giường bán hai vé); `khong_khoa`, trễ 0–20 ms: cả ba cùng giữ D02 Sài Gòn → Nha Trang → máy chủ trả lời "ĐÃ GIỮ" cho cả ba, sau `cap_nhat` hai người thấy "Chỗ D02 đã bị … lấy mất!" → SAI PHẠM (trùng chỗ 6 = 2 người bị đè × 3 chặng), chụp `ket_qua_gui_tranh_chap.png`; `khoa`: So sánh 4 chế độ; đặt lại thành xe ghế 50 chỗ, giữ chỗ 6 s: An giữ 05, 06 Sài Gòn → Bảo Lộc rồi hết hạn, Bình mua 05, 06 Sài Gòn → Đà Lạt, Chi mua 05 Đà Lạt → Nha Trang, An giữ 10, 11, Chi chọn 21; chụp 4 cửa sổ thành `ket_qua_gui.png`; STOP SERVER.

- **Máy chủ** (`server_gui`): phần chung giống MR6.6; **tài nguyên**: sơ đồ xe thu nhỏ (mỗi chỗ có dải 3 vạch theo chặng, ghế nóng viền đỏ), số ô trống – đang giữ – đã bán, số chỗ đã bán theo từng chặng, ghế nóng đang thuộc ai ở chặng nào, ô **Giữ chỗ (s)**, **Loại xe** (`giuong`/`ghe`), **Ghế nóng** và nút **Đặt lại**.
- **Đặt vé** (`dat_ve_xe_gui`): băng rôn nhà xe (tên, chuyến, loại xe, thời hạn giữ chỗ); chọn **Đi từ** và **Đến**, thanh tuyến 4 trạm tô đoạn đã chọn kèm giá từng chặng; **sơ đồ xe** có vô lăng ở đầu xe (xe giường: tầng dưới, tầng trên đặt cạnh nhau; xe ghế: lối đi giữa, hàng cuối 6 ghế); màu theo đoạn đang chọn: **trống**, **đang chọn**, **người khác giữ**, **đã bán**, **bạn giữ**, **vé của bạn**, kèm chú giải và **dải vạch theo chặng** ở chân mỗi chỗ; bấm chỗ để chọn/bỏ chọn (tối đa 6); "Chỗ đã chọn", tạm tính theo đoạn; nút **GIỮ CHỖ**; **đồng hồ giữ chỗ** đếm ngược (đỏ khi còn dưới 10 s); nút **THANH TOÁN** ghi tổng tiền; nút **Nhả chỗ**; khi đang giữ chỗ thì khóa ô chọn đoạn; dòng thông báo; "Vé của bạn" (chỗ và đoạn, ví dụ `05 SG→DL`). Sơ đồ đổi ngay khi người khác giữ hoặc mua (≤ 10 cập nhật/giây).


### Cách chạy
```bash
go -C gui mod tidy                                   # lần đầu (cần mạng)
go -C gui build -o ../out/app_gui .
out/app_gui server_gui 127.0.0.1 41787               # bấm START SERVER (giữ chỗ 30 s, xe giường nằm)
out/app_gui dat_ve_xe_gui An 127.0.0.1 41787
out/app_gui dat_ve_xe_gui Bình 127.0.0.1 41787
```
- **Windows:** Go 1.24+ và gcc cho cgo (như [MR6.6](../BaiMR6.6_DauGia/README.md#cách-chạy-1)). Kịch bản tự động: `python tools\demo_windows.py go\Chuong06\BaiMR6.14_DatVeXeKhach` (Linux, cần `xvfb`, `imagemagick`: `xvfb-run -a -s "-screen 0 2600x1700x24" out/app_gui demo_gui 127.0.0.1 41787`).
- Chạy riêng từng cửa sổ: mọi dòng nhật ký và cứ 5 giây một dòng số liệu cũng in ra dòng lệnh.
- Thử trên nhiều máy: cả lớp cùng tranh giường D01 với các đoạn khác nhau trên máy chủ của thầy; khi đó **chỉ dùng `khoa`, `lac_quan`, `hang_doi`**.

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

## Ghi chú
- **Chạy chéo đã thử** (500 người cùng giữ giường nóng D01, so sánh 4 chế độ; thứ tự `khong_khoa` / `khoa` / `lac_quan` / `hang_doi`, mỗi cặp là thành công / sai phạm): bộ mô phỏng Python → máy chủ C#: 93 / 134, 6 / 0, 6 / 0 (10 xung đột), 4 / 0; C# → Python: 9 / 5, 4 / 0, 4 / 0, 6 / 0; Java → Rust: 7 / 1, 4 / 0, 4 / 0, 6 / 0; Rust → Go: 27 / 36, 6 / 0, 4 / 0, 4 / 0; Go → Java: 20 / 23, 6 / 0, 4 / 0, 6 / 0. Bốn người dùng dòng lệnh Python (An), Java (Chi), Rust (Bình) và Go (Dũng, giữ và mua T05, T06 Bảo Lộc → Nha Trang, 600.000 đ) cùng chạy kịch bản giữ chỗ 3 s trên máy chủ C#: diễn biến giống hệt lượt chạy một ngôn ngữ, sổ kiểm toán 0 sai phạm, `Bán 6/120 chỗ-chặng, giữ 0, sổ 4 vé`.
- `tu`/`den` phải là số nguyên JSON (như `isinstance(x, int)` của Python): `2.0` bị từ chối `sai_du_lieu` ở mọi ngôn ngữ. Đổi chế độ hoặc đặt lại (kể cả đổi loại xe) đi qua cổng `TamDung` của khung.
- Ở tình huống nhiều chỗ, `khong_khoa` kết thúc với "Bán 105/120 chỗ-chặng, giữ 12, sổ 79 vé": 12 ô bị "giữ mồ côi" bởi những người đã bị đè (họ thanh toán thì bị từ chối), và các ô này chỉ trống lại sau 30 s. Ba chế độ đúng bán được 115–120/120 chỗ-chặng (95–99 vé).
- Tham số đặt lại của bộ mô phỏng rỗng: `--so-sanh` đặt lại sơ đồ xe mà vẫn giữ loại xe, thời hạn và ghế nóng hiện tại của máy chủ.
- Kiểm thử: Linux, Go 1.24.7 (bản dòng lệnh); giao diện Fyne biên dịch và chạy trên Windows (trong máy dựng kho không tải được mô-đun Fyne nên bản Linux chưa có ảnh giao diện).

## Bài tập gợi ý
1. Đo số vé bán được cho giường nóng D01 khi máy chủ coi **cả giường là một ô** (sửa `khoiTao` để mỗi chỗ chỉ có ô `D01#0`, và `oYC` luôn là ô đó). So với cách chia theo chặng, nhà xe mất bao nhiêu doanh thu trong tình huống 300 người × 3 lần?
2. Chạy tình huống nhiều chỗ với đoạn luôn là cả tuyến (`nguCanh.doan()` trả về `0, 3`). Thông lượng của `khoa` và số xung đột của `lac_quan` thay đổi thế nào? Giải thích bằng số ô mỗi yêu cầu chạm vào.
3. Thêm thao tác `doi_cho` (đổi từ chỗ đang giữ sang chỗ khác, nguyên tử: chỉ nhả chỗ cũ khi giữ được chỗ mới). Những ô nào phải khóa cùng lúc? Sắp thứ tự thế nào để không khóa chết?
4. Thêm luật "giường tầng dưới ưu tiên người cao tuổi": người dùng mang cờ `uu_tien` mới giữ được D01–D20 trong 10 phút đầu mở bán. Luật này đặt ở `xuLyYeuCau` hay trong hàm thuần `xuLy`? Vì sao?

## Đọc thêm
- H. T. Kung, J. T. Robinson (1981). *On Optimistic Methods for Concurrency Control*. ACM Transactions on Database Systems 6(2).
- E. G. Coffman, M. J. Elphick, A. Shoshani (1971). *System Deadlocks*. ACM Computing Surveys 3(2): điều kiện khóa chết và cách phòng bằng thứ tự khóa (giữ nhiều chỗ cho nhiều chặng = khóa nhiều ô theo thứ tự tên ô).
- Tài liệu Go: [`sync`](https://pkg.go.dev/sync), [`sync/atomic`](https://pkg.go.dev/sync/atomic), [`time`](https://pkg.go.dev/time) (đồng hồ đơn điệu), [The Go Memory Model](https://go.dev/ref/mem); Fyne: [`widget.Select`](https://docs.fyne.io/widget/select), [Goroutines – `fyne.Do`](https://docs.fyne.io/started/goroutines).

Mã nguồn của bài được viết mới cho kho này.
