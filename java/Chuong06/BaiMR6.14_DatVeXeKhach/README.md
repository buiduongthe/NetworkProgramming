# Bài mở rộng MR6.14 – Đặt vé xe khách theo chặng: xe giường nằm 40 chỗ, xe ghế 50 chỗ (Java)

| Thông tin | Nội dung |
|---|---|
| Loại | **Bài mở rộng**, không có trong giáo trình và không có mã giả tương ứng. Bài thứ chín của nhóm "tranh chấp tài nguyên" MR6.6–MR6.14, dùng chung khung của [MR6.6](../BaiMR6.6_DauGia/) và mở rộng [MR6.8](../BaiMR6.8_DatVePhim/) (giữ chỗ có thời hạn) |
| Liên hệ với giáo trình | Chương 6: Bài 6.1 (server đa luồng, một luồng mỗi kết nối), Bài 6.7 (kiểm thử tải); Chương 3: Bài 3.2 (định khung thông điệp theo dòng); bài mở rộng MR5.4/MR5.5 (luồng, đồng bộ) |
| Thư viện | JDK 21, chỉ thư viện chuẩn: `java.net`, `java.util.concurrent` (`ReentrantLock` theo từng ô, `LinkedBlockingQueue`, `CyclicBarrier`, `ConcurrentHashMap.newKeySet`, `AtomicLong`), `System.nanoTime` (đồng hồ đơn điệu); bản giao diện dùng thêm Swing (`JComboBox` chọn đoạn, `Graphics2D` vẽ xe), `java.awt.Robot`, `javax.imageio`. `Json.java` tự viết |
| Bản ngôn ngữ khác | [Python](../../../python/Chuong06/BaiMR6.14_DatVeXeKhach/), [C#](../../../csharp/Chuong06/BaiMR6.14_DatVeXeKhach/), [Rust](../../../rust/Chuong06/BaiMR6.14_DatVeXeKhach/), [Go](../../../go/Chuong06/BaiMR6.14_DatVeXeKhach/) |

## Mục tiêu
Bài mô phỏng một ứng dụng đặt vé xe khách đường dài. Chuyến xe chạy qua **4 trạm**: Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang, tức **3 chặng**. Có hai loại xe, đổi được khi đặt lại:
- **xe giường nằm 40 chỗ** (`giuong`): hai tầng, tầng dưới D01–D20, tầng trên T01–T20, mỗi tầng 3 dãy × 7 hàng (hàng cuối 2 giường);
- **xe ghế ngồi 50 chỗ** (`ghe`): ghế 01–50, 11 hàng × 4 ghế có lối đi ở giữa, hàng cuối 6 ghế liền.

Khách chọn **một đoạn đường** (đi từ trạm `tu` đến trạm `den`), chọn chỗ rồi **giữ chỗ** có thời hạn, sau đó **thanh toán** hoặc **nhả**. Khác với rạp chiếu phim ở MR6.8, **một chỗ bán được cho nhiều người** nếu các đoạn đường không chồng nhau: An mua giường D01 Sài Gòn → Đà Lạt, Chi mua đúng giường đó Đà Lạt → Nha Trang. Vì vậy tài nguyên không phải là "ghế" mà là **cặp (chỗ, chặng)**, tên ô `D01#1` (chặng 1 = Bảo Lộc → Đà Lạt). Giữ 2 giường cho cả tuyến là khóa 6 ô. Mọi thao tác là "đọc → kiểm tra (giả lập độ trễ cơ sở dữ liệu 0–2 ms) → ghi", xử lý theo **bốn cách**: không khóa, khóa loại trừ theo từng ô, lạc quan, hàng đợi một luồng. **Sổ kiểm toán** đếm (chỗ, chặng) bị giữ/bán cho hai người và thanh toán sai. **Bộ mô phỏng tải** chạy hai tình huống: 500 người cùng giữ giường "nóng" D01 với đoạn ngẫu nhiên, và 300 người × 3 lần thử chọn 1–2 chỗ ngẫu nhiên cho đoạn ngẫu nhiên.

Phần khung (`Chung.java`, `KetNoiKhach.java`, `Khach.java`, `GiaoDienChung.java`, `Json.java`, phần KHUNG của `MoPhong.java`) chép nguyên từ MR6.6. Bài này chỉ viết nghiệp vụ, người đặt vé dòng lệnh, ứng dụng giao diện, bảng tài nguyên của máy chủ và kịch bản demo.

## Kiến thức chính
Số liệu dưới đây lấy từ `ket_qua.txt`, là một lần chạy thật trên máy cục bộ (127.0.0.1, OpenJDK 21, độ trễ xử lý ngẫu nhiên 0–2 ms). Mỗi lần chạy, số liệu lệch đi đôi chút.

```
500 người cùng giữ giường nóng D01, đoạn ngẫu nhiên (giữ được thì thanh toán ngay):
  Chế độ       Thành công Sai phạm Xung đột  yc/s  p99 ms
  khong_khoa           10        4        0  1485   332.1
  khoa                  4        0        0   915   562.9
  lac_quan              2        0        1 24820    11.0
  hang_doi              2        0        0   878   531.0
300 người × 3 lần thử, 1–2 chỗ và đoạn ngẫu nhiên (40 giường × 3 chặng = 120 ô):
  Chế độ       Thành công Sai phạm Xung đột  yc/s  p99 ms
  khong_khoa          152        9        0  8457    54.6
  khoa                136        0        0  9648    35.3
  lac_quan            146        0       36  3914   120.9
  hang_doi            158        0        0   875   510.0
```
("Thành công" đếm cả trả lời giữ chỗ lẫn thanh toán được chấp nhận; "Sai phạm" do sổ kiểm toán đếm; "Xung đột" là số lần so sánh-và-đổi thất bại; "yc/s" và "p99" (khứ hồi) đo ở bộ mô phỏng.)

- **Tài nguyên là (chỗ, chặng), không phải chỗ.** Với giường nóng D01 và đoạn ngẫu nhiên, ở ba chế độ đúng D01 bán được 2, 1 và 1 vé (`khoa`, `lac_quan`, `hang_doi`; mỗi vé là 2 trả lời thành công: giữ rồi thanh toán). Số vé tùy đoạn của những người đến trước: một vé Sài Gòn → Bảo Lộc và một vé Bảo Lộc → Nha Trang không chạm chung chặng nào. Ở rạp chiếu phim MR6.8, ghế nóng E7 chỉ bán được 1 vé. Nếu máy chủ coi cả giường là một ô, nhà xe sẽ bỏ phí các chặng còn trống.
- **Tranh chấp (race condition) trên từng chặng.** Ở `khong_khoa`, thao tác giữ chỗ "đọc các ô → chờ → ghi" không đồng bộ, nên nhiều người cùng được báo "đã giữ" cho cùng một (chỗ, chặng) và lượt ghi sau đè lượt ghi trước: 4 sai phạm với giường nóng, 9 sai phạm với nhiều chỗ. Sổ kiểm toán ghi từng ô, ví dụ `D02 (Sài Gòn–Bảo Lộc) giữ cho Bình khi An đang giữ`. Một lượt giữ cả tuyến bị đè ở 3 chặng thì sổ đếm 3 sai phạm.
- **Khóa theo từng ô cho song song thật.** Ở `khoa`, mỗi lần giữ khóa đúng các ô `chỗ#chặng` mình chạm vào, theo thứ tự tên ô (không khóa chết). Hai đoạn không chồng nhau của cùng một giường chạm các ô khác nhau, nên chạy song song. Ở tình huống nhiều chỗ, `khoa` đạt **9648 yc/s** so với **875 yc/s** của `hang_doi`: hàng đợi một luồng xử lý lần lượt dù các yêu cầu không đụng nhau. Ngược lại, với giường nóng D01, mọi yêu cầu chạm chung ô D01#… nên `khoa` cũng chỉ còn 915 yc/s.
- **Lạc quan với nhiều ô.** `lac_quan` đọc (giá trị, phiên bản) của mọi ô trong yêu cầu, quyết định ngoài khóa, rồi chỉ ghi nếu không ô nào đổi phiên bản. Đoạn càng dài thì càng nhiều ô có thể bị người khác đổi, nên xung đột tăng theo độ dài đoạn (lần chạy này: 1 xung đột với giường nóng, 36 với nhiều chỗ). Mỗi người ảo chỉ thử `--lan` lần, nên mỗi lần xung đột cũng tiêu một lượt thử của khách.
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
- `Chung.java`, `KetNoiKhach.java`, `Khach.java`, `GiaoDienChung.java`, `Json.java`: **khung chung**, chép nguyên từ MR6.6 bản Java (chỉ đổi dòng đầu chú thích).
- `MayChu.java`: trạm, giá chặng, `boTri(loaiXe)` (record `ChoXe`, sơ đồ hai loại xe dùng chung cho máy chủ và giao diện), nghiệp vụ `MayChuDatVeXe` (giữ chỗ, thanh toán, nhả theo đoạn, dọn ô quá hạn trong `dinhKy`, số thứ tự lần ghi, `gheNong()`) và `SoKiemToanDatVeXe`; `ServerDatVeXe.java` là bản dòng lệnh.
- `MoPhong.java`: bộ mô phỏng tải; phần NGHIỆP VỤ là `NguCanh` (ô bận theo những gì kết nối biết, chọn đoạn và chỗ trống ngẫu nhiên), `NguoiAo` (lần đầu giữ ghế nóng cho một đoạn ngẫu nhiên, giữ được thì thanh toán, bị từ chối thì đổi đoạn và chỗ), `thamSoDatLai()` = {}.
- `DatVeXe.java`: người đặt vé bản dòng lệnh (lệnh `xem` in sơ đồ xe theo từng chặng).
- `ServerGui.java`: bảng điều khiển máy chủ; `DatVeXeGui.java`: ứng dụng đặt vé (lớp `SoDoXe` vẽ xe, dùng ở cả hai cửa sổ); `DemoGui.java`: kịch bản tự bấm.
- `demo.json`, `ket_qua.txt`, `ket_qua.png`, `ket_qua_gui.png`, `ket_qua_gui_tranh_chap.png`: kịch bản và kết quả thực thi thật; `*_windows.*`: kết quả chạy trên Windows.

## Cách chạy
Trong thư mục bài (cần JDK 21):
```bash
javac -encoding UTF-8 -d out *.java
java -cp out ServerDatVeXe 127.0.0.1 9000                           # cửa sổ 1 (gõ lệnh ở đây)
java -cp out MoPhong 127.0.0.1 9000 --n 500 --so-sanh               # cửa sổ 2: 500 người, giường nóng D01
java -cp out Khach 127.0.0.1 9000 dat_lai ghe_nong=                 # bỏ ghế nóng: chọn chỗ ngẫu nhiên
java -cp out MoPhong 127.0.0.1 9000 --n 300 --lan 3 --so-sanh       # nhiều chỗ, đoạn ngẫu nhiên
java -cp out Khach 127.0.0.1 9000 dat_lai loai_xe=ghe ttl_s=10      # xe ghế 50 chỗ, giữ chỗ 10 s
java -cp out DatVeXe An 127.0.0.1 9000                              # cửa sổ 3, 4…: giu SG DL D01 / tt / nha / xem
```
- Lệnh của máy chủ: `che_do khong_khoa|khoa|lac_quan|hang_doi`, `tre <ms>`, `dat_lai [ttl_s=… loai_xe=… ghe_nong=…]`, `thong_ke`, `thoat`. Tùy chọn: `--che-do`, `--tre-ms`, `--ttl`, `--loai-xe`, `--ghe-nong`.
- Lệnh của `DatVeXe`: `giu SG DL D01 [D02 …]` (hai mã trạm `SG`, `BL`, `DL`, `NT` rồi các chỗ), `tt` và `nha` (các chỗ vừa giữ, kể cả khi đã hết hạn), `xem`, `thoat`. Dòng `@t lệnh` chờ tới giây thứ t rồi mới chạy lệnh.
- `MoPhong --lan L`: mỗi người thử giữ chỗ tối đa L lần.

Tái lập kết quả từ thư mục gốc của kho: `python3 tools/demo.py java/Chuong06/BaiMR6.14_DatVeXeKhach` (cổng 43780, giao diện 43782).

## Ứng dụng giao diện
![Ứng dụng giao diện](ket_qua_gui.png)

Ảnh trên là lần chạy thật của `DemoGui` (Linux, Xvfb, OpenJDK 21, Swing): bảng điều khiển máy chủ (trên trái), An (trên phải), Bình và Chi (dưới). Kịch bản đã chạy so sánh 4 chế độ ngay trong giao diện (500 người giữ giường nóng D01; dòng `khong_khoa` tô đỏ: 7 thành công, 1 sai phạm; `khoa` và `hang_doi` 6 / 0, `lac_quan` 4 / 0 với 7 xung đột), rồi đặt lại thành **xe ghế 50 chỗ**, giữ chỗ 6 s. An giữ ghế 05, 06 Sài Gòn → Bảo Lộc mà không thanh toán. Bình chọn Sài Gòn → Đà Lạt, bấm 06 thì thấy "Chỗ 06 đang có người giữ trên đoạn này". Hết hạn, Bình giữ lại 05, 06 Sài Gòn → Đà Lạt và mua được (540.000 đ, xanh dương trong ứng dụng của Bình). Chi mua **đúng ghế 05** cho Đà Lạt → Nha Trang (130.000 đ). Lúc chụp, An đang giữ 10, 11 cả tuyến (xanh lá, đồng hồ 00:05, nút "THANH TOÁN 800.000 đ"), còn Chi đang chọn 21. Dải vạch nhỏ ở chân mỗi chỗ cho thấy từng chặng: ghế 05 có hai vạch xanh dương và một vạch xám trong cửa sổ của Bình.

![Tranh chấp với người dùng thật](ket_qua_gui_tranh_chap.png)

Ở bước trước đó, xe giường nằm và máy chủ ở `khong_khoa` với độ trễ 0–20 ms. Cả ba chọn D02 Sài Gòn → Nha Trang rồi cùng bấm "GIỮ CHỖ", và cả ba đều nhận trả lời "ĐÃ GIỮ D02". Bản cập nhật kế tiếp cho thấy cả 3 chặng của D02 thuộc An, hai người kia thấy "Chỗ D02 đã bị An lấy mất!". Ô **SAI PHẠM** chuyển đỏ (trùng chỗ 6 = 2 người bị đè × 3 chặng). Ở bước 2 (chế độ `khoa`) An và Bình cùng giữ D01 Sài Gòn → Đà Lạt, còn Chi giữ D01 Đà Lạt → Nha Trang: An giữ được, Bình nhận "Không giữ được: chỗ D01 chặng Sài Gòn–Bảo Lộc đang có người giữ", và Chi cũng giữ được; An trả 350.000 đ, Chi 170.000 đ. Cùng một giường bán hai vé.

- **Máy chủ** (`ServerGui`): phần chung giống MR6.6; **tài nguyên**: sơ đồ xe thu nhỏ (mỗi chỗ có dải 3 vạch theo chặng, ghế nóng viền đỏ), số ô trống – đang giữ – đã bán, số chỗ đã bán theo từng chặng, ghế nóng đang thuộc ai ở chặng nào, ô **Giữ chỗ (s)**, **Loại xe** (`giuong`/`ghe`), **Ghế nóng** và nút **Đặt lại**.
- **Đặt vé** (`DatVeXeGui`): băng rôn nhà xe (tên, chuyến, loại xe, thời hạn giữ chỗ); chọn **Đi từ** và **Đến**, thanh tuyến 4 trạm tô đoạn đã chọn kèm giá từng chặng; **sơ đồ xe** có vô lăng ở đầu xe (xe giường: tầng dưới, tầng trên đặt cạnh nhau; xe ghế: lối đi giữa, hàng cuối 6 ghế); màu theo đoạn đang chọn: **trống**, **đang chọn**, **người khác giữ**, **đã bán**, **bạn giữ**, **vé của bạn**, kèm chú giải và **dải vạch theo chặng** ở chân mỗi chỗ; bấm chỗ để chọn/bỏ chọn (tối đa 6); "Chỗ đã chọn", tạm tính theo đoạn; nút **GIỮ CHỖ**; **đồng hồ giữ chỗ** đếm ngược (đỏ khi còn dưới 10 s); nút **THANH TOÁN** ghi tổng tiền; nút **Nhả chỗ**; khi đang giữ chỗ thì khóa ô chọn đoạn; dòng thông báo; "Vé của bạn" (chỗ và đoạn, ví dụ `05 SG→DL`). Sơ đồ đổi ngay khi người khác giữ hoặc mua (≤ 10 cập nhật/giây).


### Cách chạy
```bash
javac -encoding UTF-8 -d out *.java
java -cp out ServerGui 127.0.0.1 43782          # bấm START SERVER (giữ chỗ 30 s, xe giường nằm)
java -cp out DatVeXeGui An 127.0.0.1 43782
java -cp out DatVeXeGui Bình 127.0.0.1 43782
```
- **Windows:** chỉ cần JDK 21 (`winget install -e --id Microsoft.OpenJDK.21`). **Linux:** `sudo apt install openjdk-21-jdk`. Kịch bản tự động (Linux, cần `xvfb`): `xvfb-run -a -s "-screen 0 2600x1700x24" java -cp out DemoGui 127.0.0.1 43782`.
- Chạy riêng từng cửa sổ: mọi dòng nhật ký và cứ 5 giây một dòng số liệu cũng in ra dòng lệnh.
- Thử trên nhiều máy: máy chủ nghe IP card mạng (hoặc `0.0.0.0`) và mở tường lửa cho cổng; cả lớp cùng tranh giường D01 với các đoạn khác nhau. Khi đó **chỉ dùng `khoa`, `lac_quan`, `hang_doi`**.

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

## Ghi chú
- **Chạy chéo đã thử** (500 người cùng giữ giường nóng D01, so sánh 4 chế độ; thứ tự `khong_khoa` / `khoa` / `lac_quan` / `hang_doi`, mỗi cặp là thành công / sai phạm): bộ mô phỏng Python → máy chủ C#: 93 / 134, 6 / 0, 6 / 0 (10 xung đột), 4 / 0; C# → Python: 9 / 5, 4 / 0, 4 / 0, 6 / 0; Java → Rust: 7 / 1, 4 / 0, 4 / 0, 6 / 0; Rust → Go: 27 / 36, 6 / 0, 4 / 0, 4 / 0; Go → Java: 20 / 23, 6 / 0, 4 / 0, 6 / 0. Bốn người dùng dòng lệnh Python (An), Java (Chi), Rust (Bình) và Go (Dũng, giữ và mua T05, T06 Bảo Lộc → Nha Trang, 600.000 đ) cùng chạy kịch bản giữ chỗ 3 s trên máy chủ C#: diễn biến giống hệt lượt chạy một ngôn ngữ, sổ kiểm toán 0 sai phạm, `Bán 6/120 chỗ-chặng, giữ 0, sổ 4 vé`.
- Ô trạng thái của mỗi (chỗ, chặng) là `Map` mới cho mỗi lần ghi, nên luồng đọc để hiển thị (`trangThaiNghiepVu`) không bao giờ thấy một ô "nửa giữ nửa bán". Các trường nội bộ (`_moi`, `_luc`, `_het_han`, `_o`) được bỏ khỏi trả lời trước khi gửi client.
- Ở tình huống nhiều chỗ, `khong_khoa` kết thúc với "Bán 112/120 chỗ-chặng, giữ 8, sổ 94 vé": 8 ô bị "giữ mồ côi" bởi những người đã bị đè (họ thanh toán thì bị từ chối), và các ô này chỉ trống lại sau 30 s. Ba chế độ đúng bán được 117–120/120 chỗ-chặng (93–107 vé).
- Tham số đặt lại của bộ mô phỏng rỗng: `--so-sanh` đặt lại sơ đồ xe mà vẫn giữ loại xe, thời hạn và ghế nóng hiện tại của máy chủ.
- Trong kịch bản giao diện, máy chủ, bộ mô phỏng và cửa sổ chạy trong một JVM nên yc/s thấp hơn bản dòng lệnh; kịch bản dùng `doClick(0)` và `setSelectedIndex` cho hai ô chọn đoạn.
- Kiểm thử: Linux, OpenJDK 21.0.12 (dòng lệnh và giao diện dưới Xvfb) và Windows (ảnh `*_windows.png`).

## Bài tập gợi ý
1. Đo số vé bán được cho giường nóng D01 khi máy chủ coi **cả giường là một ô** (sửa `khoiTao` để mỗi chỗ chỉ có ô `D01#0`, và `oYC` luôn là ô đó). So với cách chia theo chặng, nhà xe mất bao nhiêu doanh thu trong tình huống 300 người × 3 lần?
2. Chạy tình huống nhiều chỗ với đoạn luôn là cả tuyến (`NguCanh.doan()` trả về `{0, 3}`). Thông lượng của `khoa` và số xung đột của `lac_quan` thay đổi thế nào? Giải thích bằng số ô mỗi yêu cầu chạm vào.
3. Thêm thao tác `doi_cho` (đổi từ chỗ đang giữ sang chỗ khác, nguyên tử: chỉ nhả chỗ cũ khi giữ được chỗ mới). Những ô nào phải khóa cùng lúc? Sắp thứ tự thế nào để không khóa chết?
4. Thêm luật "giường tầng dưới ưu tiên người cao tuổi": người dùng mang cờ `uu_tien` mới giữ được D01–D20 trong 10 phút đầu mở bán. Luật này đặt ở `xuLyYeuCau` hay trong hàm thuần `xuLy`? Vì sao?

## Đọc thêm
- H. T. Kung, J. T. Robinson (1981). *On Optimistic Methods for Concurrency Control*. ACM Transactions on Database Systems 6(2).
- E. G. Coffman, M. J. Elphick, A. Shoshani (1971). *System Deadlocks*. ACM Computing Surveys 3(2): điều kiện khóa chết và cách phòng bằng thứ tự khóa (giữ nhiều chỗ cho nhiều chặng = khóa nhiều ô theo thứ tự tên ô).
- Tài liệu JDK 21: [`java.util.concurrent`](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/concurrent/package-summary.html), [`ReentrantLock`](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/concurrent/locks/ReentrantLock.html), [`System.nanoTime`](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/lang/System.html#nanoTime()), [`JComboBox`](https://docs.oracle.com/en/java/javase/21/docs/api/java.desktop/javax/swing/JComboBox.html), [`Graphics2D`](https://docs.oracle.com/en/java/javase/21/docs/api/java.desktop/java/awt/Graphics2D.html).

Mã nguồn của bài được viết mới cho kho này.
