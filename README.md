# NetworkProgramming – Mã nguồn minh họa Giáo trình Lập trình mạng

Kho mã nguồn đi kèm **Giáo trình Lập trình mạng** (Trường Đại học Giao thông vận tải Thành phố Hồ Chí Minh). Mỗi mã giả trong giáo trình được cài đặt bằng ba ngôn ngữ **C# (.NET 10)**, **Java (JDK 21)** và **Python (3.11 trở lên)** – ba ngôn ngữ được giáo trình dẫn chiếu – và hai ngôn ngữ bổ sung **Rust (1.85 trở lên)**, **Go (1.24 trở lên)**, kèm chú thích tiếng Việt, hướng dẫn chạy và ảnh kết quả thực thi thật. Ngoài 39 bài bám theo mã giả, kho có thêm các *bài mở rộng* (không có trong giáo trình) về những chủ đề thực tiễn như tự động hóa mạng, giám sát và ứng dụng AI trong vận hành mạng.

> **English summary.** Companion source code for the Vietnamese textbook *Network Programming*. Every pseudocode listing in the book is implemented in C#, Java and Python (the languages referenced by the book) and additionally in Rust and Go, using the standard socket APIs, from TCP/UDP client–server basics to asynchronous I/O, multi-client servers, broadcast/multicast, HTTP, WebSocket, SSE, gRPC, SMTP, ICMP, MQTT, LLM APIs, the Model Context Protocol (MCP) and TLS. Each example is self-contained, commented, and ships with a reproducible demo script and a screenshot of its real output.

Quy mô: <!-- THONG_KE -->39 bài × 5 ngôn ngữ = 195 chương trình; 191 chương trình đã chạy và có ảnh kết quả thực thi; 19 bài mở rộng<!-- /THONG_KE -->.

## Mục lục
- [Mục tiêu và nguyên tắc thiết kế](#mục-tiêu-và-nguyên-tắc-thiết-kế)
- [Cấu trúc kho](#cấu-trúc-kho)
- [Yêu cầu môi trường](#yêu-cầu-môi-trường)
- [Bắt đầu nhanh](#bắt-đầu-nhanh)
- [Tái lập kết quả thực thi](#tái-lập-kết-quả-thực-thi)
- [Danh mục bài](#danh-mục-bài)
- [Quy ước](#quy-ước)
- [Kiểm thử](#kiểm-thử)
- [An toàn và sử dụng có trách nhiệm](#an-toàn-và-sử-dụng-có-trách-nhiệm)
- [Tài liệu tham khảo](#tài-liệu-tham-khảo)
- [Trích dẫn](#trích-dẫn)
- [Lời cảm ơn](#lời-cảm-ơn)
- [Giấy phép và đóng góp](#giấy-phép-và-đóng-góp)

## Mục tiêu và nguyên tắc thiết kế
Giáo trình trình bày thuật toán bằng mã giả độc lập với ngôn ngữ, dựa trên tập hàm socket cơ sở của Berkeley socket API. Kho mã này nối mã giả với chương trình chạy được, nhằm ba mục tiêu: (1) giúp người học kiểm chứng từng khái niệm bằng thực nghiệm; (2) cho thấy cùng một thuật toán được ánh xạ sang năm nền tảng với mô hình lập trình khác nhau (máy ảo có bộ thu gom rác, thông dịch, biên dịch với quản lý bộ nhớ theo quyền sở hữu, goroutine) như thế nào; (3) cung cấp bộ ví dụ có thể tái lập cho giảng dạy và thực hành.

Các nguyên tắc áp dụng thống nhất cho mọi bài:
1. **Bám sát mã giả.** Cùng thứ tự bước, giá trị mặc định và cách xử lý lỗi như mã giả; chú thích cuối dòng ghi lệnh mã giả tương ứng (ví dụ `// LISTEN, backlog 16`). Khác biệt bắt buộc do nền tảng được ghi trong mục *Ghi chú* của README từng bài.
2. **Mức socket cơ sở và thư viện chuẩn.** Ưu tiên lớp socket của thư viện chuẩn; không dùng thư viện ngoài, trừ bài gRPC (cần bộ sinh mã Protocol Buffers). Riêng Rust, thư viện chuẩn không có TLS, JSON, bất đồng bộ nên dùng một số crate phổ biến, phiên bản cố định trong `Cargo.lock` (ghi trong README từng bài).
3. **Tự chứa.** Mỗi thư mục bài chạy độc lập; các thủ tục kế thừa từ chương trước (đọc cấu hình, kết nối an toàn, định khung JSON…) được chép gọn vào bài, có chú thích nguồn gốc mã giả.
4. **Đơn giản, dễ đọc.** Mã ngắn, chú thích tiếng Việt cho người mới học; thông điệp in ra màn hình giống nhau giữa các ngôn ngữ để dễ đối chiếu.
5. **Tái lập được.** Kết quả trong `ket_qua.txt` và `ket_qua.png` được sinh tự động từ lần chạy thật bằng `tools/demo.py`, không chỉnh sửa tay.

## Cấu trúc kho
```text
NetworkProgramming/
├── README.md                 Tài liệu này
├── CITATION.cff              Thông tin trích dẫn
├── LICENSE                   Giấy phép MIT
├── docs/
│   ├── DanhMucBai.md         Danh mục bài, ánh xạ Bài ↔ Mã giả ↔ Mục giáo trình
│   └── BaiMoRong.md          Bài mở rộng và nhận xét tài liệu tham khảo
├── tools/
│   ├── demo.py               Chạy kịch bản demo.json của một bài, lưu kết quả thật
│   ├── render_output.py      Vẽ nhật ký đầu ra thành ảnh cửa sổ dòng lệnh
│   └── gen_docs.py           Sinh danh mục bài từ docs/_chapters/*.json, docs/_status/*.json
├── csharp/                   C# (.NET 10) – NetworkProgramming.sln gồm 39 dự án
│   └── Chuong01 … Chuong08/BaiN.k_TenBai/
│       ├── BaiN.k_TenBai.csproj, Program.cs, *.cs
│       ├── demo.json, README.md
│       └── ket_qua.txt, ket_qua.png
├── java/                     Java (JDK 21) – mỗi bài biên dịch bằng javac
│   └── Chuong01 … Chuong08/BaiN.k_TenBai/  (*.java, demo.json, README.md, ket_qua.*)
├── python/                   Python (≥ 3.11)
│   └── Chuong01 … Chuong08/BaiN.k_TenBai/  (*.py, demo.json, README.md, ket_qua.*)
├── rust/                     Rust (≥ 1.85) – mỗi bài một gói Cargo, dùng chung rust/target
│   └── Chuong01 … Chuong08/BaiN.k_TenBai/  (Cargo.toml, Cargo.lock, src/, demo.json, README.md, ket_qua.*)
└── go/                       Go (≥ 1.24) – một module go/go.mod, chỉ thư viện chuẩn
    └── Chuong01 … Chuong08/BaiN.k_TenBai/  (*.go, demo.json, README.md, ket_qua.*)
```
Bài mở rộng nằm cùng chương với chủ đề liên quan và có tên `BaiMRN.k_TenBai`, ví dụ `python/Chuong06/BaiMR6.1_…`.
Tên thư mục bài giống nhau ở cả năm ngôn ngữ và trùng với dòng *“Mã nguồn C#, Java, Python”* in dưới mỗi mã giả trong giáo trình, ví dụ Mã giả 3.1 → `Chuong03/Bai3.1_EchoTCP`.

## Yêu cầu môi trường
| Thành phần | Phiên bản | Ghi chú |
|---|---|---|
| .NET SDK | 10.0 (LTS) | `dotnet --version` |
| JDK | 21 (LTS) trở lên | `java -version`; bài 7.5 cần thêm Maven |
| Python | 3.11 trở lên | Bài 5.3 cần Tkinter; bài 7.5 cần `grpcio`, `grpcio-tools`; bài 7.6 cần `aiosmtpd` (xem `requirements.txt` trong thư mục bài) |
| Rust | 1.85 trở lên | `cargo --version`; lần biên dịch đầu tải crate từ crates.io |
| Go | 1.24 trở lên | `go version`; bài 7.5 cần thêm module gRPC (`go mod download`) |
| Công cụ tùy chọn | – | `mosquitto` (bài 7.8), `openssl` (bài 7.6, 8.4), quyền quản trị cho ICMP thô (bài 7.7) |

Bài 5.3 bản C# dùng Windows Forms nên chỉ chạy trên Windows.

## Bắt đầu nhanh
Ví dụ với Bài 3.1 (TCP Echo). Mở hai cửa sổ dòng lệnh trong thư mục bài của ngôn ngữ cần chạy:
```bash
# C#
cd csharp/Chuong03/Bai3.1_EchoTCP
dotnet run -- server 0.0.0.0 9000          # cửa sổ 1
dotnet run -- client 127.0.0.1 9000        # cửa sổ 2

# Java
cd java/Chuong03/Bai3.1_EchoTCP
javac -encoding UTF-8 -d out *.java
java -cp out EchoServer 0.0.0.0 9000       # cửa sổ 1
java -cp out EchoClient 127.0.0.1 9000     # cửa sổ 2

# Python
cd python/Chuong03/Bai3.1_EchoTCP
python3 echo_server.py 0.0.0.0 9000        # cửa sổ 1
python3 echo_client.py 127.0.0.1 9000      # cửa sổ 2

# Rust
cd rust/Chuong03/Bai3.1_EchoTCP
cargo run --release -- server 0.0.0.0 9000 # cửa sổ 1
cargo run --release -- client 127.0.0.1 9000 # cửa sổ 2

# Go
cd go/Chuong03/Bai3.1_EchoTCP
go run . server 0.0.0.0 9000               # cửa sổ 1
go run . client 127.0.0.1 9000             # cửa sổ 2
```
Trên Windows dùng `python` thay cho `python3`. Hướng dẫn chi tiết, tham số và kết quả mong đợi của từng bài nằm trong README của thư mục bài.

## Tái lập kết quả thực thi
Mỗi bài có tệp `demo.json` mô tả kịch bản minh họa: bước biên dịch, các tiến trình nền (server, broker…) và tiến trình tiền cảnh (client) cùng dữ liệu nhập. Chạy lại toàn bộ kịch bản và sinh lại kết quả:
```bash
python3 tools/demo.py python/Chuong03/Bai3.1_EchoTCP
```
Công cụ ghi `ket_qua.txt` (toàn bộ đầu ra theo từng tiến trình) và `ket_qua.png` (ảnh cửa sổ dòng lệnh). Kịch bản dùng cổng riêng cho từng bài nên có thể chạy nhiều bài liên tiếp mà không xung đột. Cần thư viện Pillow (`pip install pillow`) để vẽ ảnh.

## Danh mục bài
Ký hiệu: ✔ đã chạy và có ảnh kết quả; ○ chưa chạy trong môi trường kiểm thử (lý do trong [docs/DanhMucBai.md](docs/DanhMucBai.md)). Mỗi ký hiệu là liên kết tới thư mục bài.
<!-- DANH_MUC_BAT_DAU -->

### Chương 1. Tổng quan về lập trình mạng trong thời đại AI

| Bài | Nội dung | Mã giả | Mục | C# | Java | Python | Rust | Go |
|---|---|---|---|:-:|:-:|:-:|:-:|:-:|
| `Bai1.1_PhanGiaiTenMien` | Phân giải tên miền thành danh sách địa chỉ | 1.1 | 1.2.3 | [✔](csharp/Chuong01/Bai1.1_PhanGiaiTenMien/) | [✔](java/Chuong01/Bai1.1_PhanGiaiTenMien/) | [✔](python/Chuong01/Bai1.1_PhanGiaiTenMien/) | [✔](rust/Chuong01/Bai1.1_PhanGiaiTenMien/) | [✔](go/Chuong01/Bai1.1_PhanGiaiTenMien/) |
| `Bai1.2_KiemTraCong` | Kiểm tra trạng thái một cổng TCP | 1.2 | 1.2.5 | [✔](csharp/Chuong01/Bai1.2_KiemTraCong/) | [✔](java/Chuong01/Bai1.2_KiemTraCong/) | [✔](python/Chuong01/Bai1.2_KiemTraCong/) | [✔](rust/Chuong01/Bai1.2_KiemTraCong/) | [✔](go/Chuong01/Bai1.2_KiemTraCong/) |

### Chương 2. Công cụ, môi trường lập trình mạng và trợ lý lập trình AI

| Bài | Nội dung | Mã giả | Mục | C# | Java | Python | Rust | Go |
|---|---|---|---|:-:|:-:|:-:|:-:|:-:|
| `Bai2.1_DocCauHinh` | Đọc cấu hình địa chỉ và cổng (tham số dòng lệnh, biến môi trường) | 2.1 | 2.1.3 | [✔](csharp/Chuong02/Bai2.1_DocCauHinh/) | [✔](java/Chuong02/Bai2.1_DocCauHinh/) | [✔](python/Chuong02/Bai2.1_DocCauHinh/) | [✔](rust/Chuong02/Bai2.1_DocCauHinh/) | [✔](go/Chuong02/Bai2.1_DocCauHinh/) |
| `Bai2.2_KetNoiAnToan` | Kết nối có thời gian chờ và phân loại lỗi | 2.2 | 2.2.3 | [✔](csharp/Chuong02/Bai2.2_KetNoiAnToan/) | [✔](java/Chuong02/Bai2.2_KetNoiAnToan/) | [✔](python/Chuong02/Bai2.2_KetNoiAnToan/) | [✔](rust/Chuong02/Bai2.2_KetNoiAnToan/) | [✔](go/Chuong02/Bai2.2_KetNoiAnToan/) |
| `Bai2.3_ClientDNS` | Client tra cứu DNS dùng bộ phân giải của hệ điều hành | 2.3 | 2.5 | [✔](csharp/Chuong02/Bai2.3_ClientDNS/) | [✔](java/Chuong02/Bai2.3_ClientDNS/) | [✔](python/Chuong02/Bai2.3_ClientDNS/) | [✔](rust/Chuong02/Bai2.3_ClientDNS/) | [✔](go/Chuong02/Bai2.3_ClientDNS/) |

### Chương 3. Lập trình socket TCP Client/Server

| Bài | Nội dung | Mã giả | Mục | C# | Java | Python | Rust | Go |
|---|---|---|---|:-:|:-:|:-:|:-:|:-:|
| `Bai3.1_EchoTCP` | TCP Echo Server và Echo Client | 3.1, 3.2 | 3.2 | [✔](csharp/Chuong03/Bai3.1_EchoTCP/) | [✔](java/Chuong03/Bai3.1_EchoTCP/) | [✔](python/Chuong03/Bai3.1_EchoTCP/) | [✔](rust/Chuong03/Bai3.1_EchoTCP/) | [✔](go/Chuong03/Bai3.1_EchoTCP/) |
| `Bai3.2_DinhKhung` | Thư viện định khung thông điệp (SEND_ALL, RECV_EXACT, READ_LINE, khung JSON) | 3.3, 3.4, 3.5, 3.6 | 3.4 | [✔](csharp/Chuong03/Bai3.2_DinhKhung/) | [✔](java/Chuong03/Bai3.2_DinhKhung/) | [✔](python/Chuong03/Bai3.2_DinhKhung/) | [✔](rust/Chuong03/Bai3.2_DinhKhung/) | [✔](go/Chuong03/Bai3.2_DinhKhung/) |
| `Bai3.3_TraCuuThuatNgu` | Dịch vụ tra cứu thuật ngữ dùng JSON | 3.7, 3.8 | 3.5.1 | [✔](csharp/Chuong03/Bai3.3_TraCuuThuatNgu/) | [✔](java/Chuong03/Bai3.3_TraCuuThuatNgu/) | [✔](python/Chuong03/Bai3.3_TraCuuThuatNgu/) | [✔](rust/Chuong03/Bai3.3_TraCuuThuatNgu/) | [✔](go/Chuong03/Bai3.3_TraCuuThuatNgu/) |
| `Bai3.4_TaiTep` | Tải tệp lên server có kiểm tra SHA-256 | 3.9, 3.10 | 3.5.2 | [✔](csharp/Chuong03/Bai3.4_TaiTep/) | [✔](java/Chuong03/Bai3.4_TaiTep/) | [✔](python/Chuong03/Bai3.4_TaiTep/) | [✔](rust/Chuong03/Bai3.4_TaiTep/) | [✔](go/Chuong03/Bai3.4_TaiTep/) |

### Chương 4. Lập trình socket UDP Client/Server

| Bài | Nội dung | Mã giả | Mục | C# | Java | Python | Rust | Go |
|---|---|---|---|:-:|:-:|:-:|:-:|:-:|
| `Bai4.1_EchoUDP` | UDP Echo Server và UDP Echo Client có thời gian chờ | 4.1, 4.2 | 4.2.1 | [✔](csharp/Chuong04/Bai4.1_EchoUDP/) | [✔](java/Chuong04/Bai4.1_EchoUDP/) | [✔](python/Chuong04/Bai4.1_EchoUDP/) | [✔](rust/Chuong04/Bai4.1_EchoUDP/) | [✔](go/Chuong04/Bai4.1_EchoUDP/) |
| `Bai4.2_TraCuuUDP` | Dịch vụ tra cứu thuật ngữ trên UDP có gửi lại | 4.3, 4.4 | 4.3.1 | [✔](csharp/Chuong04/Bai4.2_TraCuuUDP/) | [✔](java/Chuong04/Bai4.2_TraCuuUDP/) | [✔](python/Chuong04/Bai4.2_TraCuuUDP/) | [✔](rust/Chuong04/Bai4.2_TraCuuUDP/) | [✔](go/Chuong04/Bai4.2_TraCuuUDP/) |
| `Bai4.3_TruyVanDNS` | Tự tạo và phân tích truy vấn DNS qua UDP | 4.5 | 4.3.2 | [✔](csharp/Chuong04/Bai4.3_TruyVanDNS/) | [✔](java/Chuong04/Bai4.3_TruyVanDNS/) | [✔](python/Chuong04/Bai4.3_TruyVanDNS/) | [✔](rust/Chuong04/Bai4.3_TruyVanDNS/) | [✔](go/Chuong04/Bai4.3_TruyVanDNS/) |
| `Bai4.4_DungVaCho` | Truyền tệp tin cậy trên UDP bằng giao thức dừng và chờ | 4.6, 4.7 | 4.4.1 | [✔](csharp/Chuong04/Bai4.4_DungVaCho/) | [✔](java/Chuong04/Bai4.4_DungVaCho/) | [✔](python/Chuong04/Bai4.4_DungVaCho/) | [✔](rust/Chuong04/Bai4.4_DungVaCho/) | [✔](go/Chuong04/Bai4.4_DungVaCho/) |
| `Bai4.5_CookieChongGiaMao` | Xác thực địa chỉ nguồn bằng mã thách thức không trạng thái | 4.8 | 4.6.1 | [✔](csharp/Chuong04/Bai4.5_CookieChongGiaMao/) | [✔](java/Chuong04/Bai4.5_CookieChongGiaMao/) | [✔](python/Chuong04/Bai4.5_CookieChongGiaMao/) | [✔](rust/Chuong04/Bai4.5_CookieChongGiaMao/) | [✔](go/Chuong04/Bai4.5_CookieChongGiaMao/) |

### Chương 5. Lập trình bất đồng bộ – Ứng dụng mạng có giao diện

| Bài | Nội dung | Mã giả | Mục | C# | Java | Python | Rust | Go |
|---|---|---|---|:-:|:-:|:-:|:-:|:-:|
| `Bai5.1_KhungBatDongBo` | Các hàm gửi, nhận khung JSON bất đồng bộ | 5.1 | 5.2.4 | [✔](csharp/Chuong05/Bai5.1_KhungBatDongBo/) | [✔](java/Chuong05/Bai5.1_KhungBatDongBo/) | [✔](python/Chuong05/Bai5.1_KhungBatDongBo/) | [✔](rust/Chuong05/Bai5.1_KhungBatDongBo/) | [✔](go/Chuong05/Bai5.1_KhungBatDongBo/) |
| `Bai5.2_QuetCongBatDongBo` | Kiểm tra nhiều cổng đồng thời có giới hạn | 5.2 | 5.2.5 | [✔](csharp/Chuong05/Bai5.2_QuetCongBatDongBo/) | [✔](java/Chuong05/Bai5.2_QuetCongBatDongBo/) | [✔](python/Chuong05/Bai5.2_QuetCongBatDongBo/) | [✔](rust/Chuong05/Bai5.2_QuetCongBatDongBo/) | [✔](go/Chuong05/Bai5.2_QuetCongBatDongBo/) |
| `Bai5.3_GiaoDienBatDongBo` | Ứng dụng tra cứu có giao diện bất đồng bộ | 5.3, 5.4 | 5.3.2 | [○](csharp/Chuong05/Bai5.3_GiaoDienBatDongBo/) | [✔](java/Chuong05/Bai5.3_GiaoDienBatDongBo/) | [✔](python/Chuong05/Bai5.3_GiaoDienBatDongBo/) | [✔](rust/Chuong05/Bai5.3_GiaoDienBatDongBo/) | [✔](go/Chuong05/Bai5.3_GiaoDienBatDongBo/) |
| `Bai5.4_ChatClient` | Lớp dịch vụ ChatClient bất đồng bộ | 5.5 | 5.3.3 | [✔](csharp/Chuong05/Bai5.4_ChatClient/) | [✔](java/Chuong05/Bai5.4_ChatClient/) | [✔](python/Chuong05/Bai5.4_ChatClient/) | [✔](rust/Chuong05/Bai5.4_ChatClient/) | [✔](go/Chuong05/Bai5.4_ChatClient/) |
| `Bai5.5_TaiTepTienTrinh` | Tải tệp bất đồng bộ có báo cáo tiến trình và hủy | 5.6 | 5.3.4 | [✔](csharp/Chuong05/Bai5.5_TaiTepTienTrinh/) | [✔](java/Chuong05/Bai5.5_TaiTepTienTrinh/) | [✔](python/Chuong05/Bai5.5_TaiTepTienTrinh/) | [✔](rust/Chuong05/Bai5.5_TaiTepTienTrinh/) | [✔](go/Chuong05/Bai5.5_TaiTepTienTrinh/) |
| `Bai5.6_HanChotThuLai` | Hạn chót, hủy và thử lại có lùi thời gian | 5.7, 5.8 | 5.4.2 | [✔](csharp/Chuong05/Bai5.6_HanChotThuLai/) | [✔](java/Chuong05/Bai5.6_HanChotThuLai/) | [✔](python/Chuong05/Bai5.6_HanChotThuLai/) | [✔](rust/Chuong05/Bai5.6_HanChotThuLai/) | [✔](go/Chuong05/Bai5.6_HanChotThuLai/) |

### Chương 6. Mô hình Multi-Client Server – Broadcast và Multicast

| Bài | Nội dung | Mã giả | Mục | C# | Java | Python | Rust | Go |
|---|---|---|---|:-:|:-:|:-:|:-:|:-:|
| `Bai6.1_ServerDaLuong` | Server tra cứu đa luồng và server nhóm luồng | 6.1, 6.2 | 6.2.1, 6.2.2 | [✔](csharp/Chuong06/Bai6.1_ServerDaLuong/) | [✔](java/Chuong06/Bai6.1_ServerDaLuong/) | [✔](python/Chuong06/Bai6.1_ServerDaLuong/) | [✔](rust/Chuong06/Bai6.1_ServerDaLuong/) | [✔](go/Chuong06/Bai6.1_ServerDaLuong/) |
| `Bai6.2_ServerBatDongBo` | Server tra cứu bất đồng bộ | 6.3 | 6.2.3 | [✔](csharp/Chuong06/Bai6.2_ServerBatDongBo/) | [✔](java/Chuong06/Bai6.2_ServerBatDongBo/) | [✔](python/Chuong06/Bai6.2_ServerBatDongBo/) | [✔](rust/Chuong06/Bai6.2_ServerBatDongBo/) | [✔](go/Chuong06/Bai6.2_ServerBatDongBo/) |
| `Bai6.3_Reactor` | Server echo một luồng theo mô hình reactor | 6.4 | 6.2.4 | [✔](csharp/Chuong06/Bai6.3_Reactor/) | [✔](java/Chuong06/Bai6.3_Reactor/) | [✔](python/Chuong06/Bai6.3_Reactor/) | [✔](rust/Chuong06/Bai6.3_Reactor/) | [✔](go/Chuong06/Bai6.3_Reactor/) |
| `Bai6.4_ServerChat` | Server chat nhiều phòng | 6.5, 6.6 | 6.3 | [✔](csharp/Chuong06/Bai6.4_ServerChat/) | [✔](java/Chuong06/Bai6.4_ServerChat/) | [✔](python/Chuong06/Bai6.4_ServerChat/) | [✔](rust/Chuong06/Bai6.4_ServerChat/) | [✔](go/Chuong06/Bai6.4_ServerChat/) |
| `Bai6.5_KhamPhaBroadcast` | Khám phá server trong LAN bằng broadcast | 6.7 | 6.4.2 | [✔](csharp/Chuong06/Bai6.5_KhamPhaBroadcast/) | [✔](java/Chuong06/Bai6.5_KhamPhaBroadcast/) | [✔](python/Chuong06/Bai6.5_KhamPhaBroadcast/) | [✔](rust/Chuong06/Bai6.5_KhamPhaBroadcast/) | [✔](go/Chuong06/Bai6.5_KhamPhaBroadcast/) |
| `Bai6.6_Multicast` | Phát và nhận thông báo dịch vụ qua multicast | 6.8 | 6.4.3 | [✔](csharp/Chuong06/Bai6.6_Multicast/) | [✔](java/Chuong06/Bai6.6_Multicast/) | [✔](python/Chuong06/Bai6.6_Multicast/) | [✔](rust/Chuong06/Bai6.6_Multicast/) | [✔](go/Chuong06/Bai6.6_Multicast/) |
| `Bai6.7_KiemThuTai` | Kiểm thử tải: thông lượng và phân vị độ trễ | 6.9 | 6.5.1 | [✔](csharp/Chuong06/Bai6.7_KiemThuTai/) | [✔](java/Chuong06/Bai6.7_KiemThuTai/) | [✔](python/Chuong06/Bai6.7_KiemThuTai/) | [✔](rust/Chuong06/Bai6.7_KiemThuTai/) | [✔](go/Chuong06/Bai6.7_KiemThuTai/) |

### Chương 7. Giao thức tầng ứng dụng và dịch vụ mạng hiện đại

| Bài | Nội dung | Mã giả | Mục | C# | Java | Python | Rust | Go |
|---|---|---|---|:-:|:-:|:-:|:-:|:-:|
| `Bai7.1_HttpClient` | Client HTTP/1.1 tối giản trên socket TCP | 7.1 | 7.2.1 | [✔](csharp/Chuong07/Bai7.1_HttpClient/) | [✔](java/Chuong07/Bai7.1_HttpClient/) | [✔](python/Chuong07/Bai7.1_HttpClient/) | [✔](rust/Chuong07/Bai7.1_HttpClient/) | [✔](go/Chuong07/Bai7.1_HttpClient/) |
| `Bai7.2_RestServer` | Server REST tối giản trên HTTP/1.1 | 7.2 | 7.2.3 | [✔](csharp/Chuong07/Bai7.2_RestServer/) | [✔](java/Chuong07/Bai7.2_RestServer/) | [✔](python/Chuong07/Bai7.2_RestServer/) | [✔](rust/Chuong07/Bai7.2_RestServer/) | [✔](go/Chuong07/Bai7.2_RestServer/) |
| `Bai7.3_WebSocket` | Server WebSocket tối giản và client thử | 7.3, 7.4 | 7.3.2 | [✔](csharp/Chuong07/Bai7.3_WebSocket/) | [✔](java/Chuong07/Bai7.3_WebSocket/) | [✔](python/Chuong07/Bai7.3_WebSocket/) | [✔](rust/Chuong07/Bai7.3_WebSocket/) | [✔](go/Chuong07/Bai7.3_WebSocket/) |
| `Bai7.4_SSE` | Bộ đọc luồng Server-Sent Events và server SSE nhỏ | 7.5 | 7.3.3 | [✔](csharp/Chuong07/Bai7.4_SSE/) | [✔](java/Chuong07/Bai7.4_SSE/) | [✔](python/Chuong07/Bai7.4_SSE/) | [✔](rust/Chuong07/Bai7.4_SSE/) | [✔](go/Chuong07/Bai7.4_SSE/) |
| `Bai7.5_gRPC` | Dịch vụ gRPC tra cứu thuật ngữ (Lookup, Watch) | 7.6 | 7.4.3 | [○](csharp/Chuong07/Bai7.5_gRPC/) | [○](java/Chuong07/Bai7.5_gRPC/) | [✔](python/Chuong07/Bai7.5_gRPC/) | [✔](rust/Chuong07/Bai7.5_gRPC/) | [○](go/Chuong07/Bai7.5_gRPC/) |
| `Bai7.6_SMTP` | Phiên gửi thư SMTP có STARTTLS và AUTH PLAIN | 7.7 | 7.5.1 | [✔](csharp/Chuong07/Bai7.6_SMTP/) | [✔](java/Chuong07/Bai7.6_SMTP/) | [✔](python/Chuong07/Bai7.6_SMTP/) | [✔](rust/Chuong07/Bai7.6_SMTP/) | [✔](go/Chuong07/Bai7.6_SMTP/) |
| `Bai7.7_Ping` | Gửi ICMP Echo Request và đo thời gian khứ hồi | 7.8 | 7.5.3 | [✔](csharp/Chuong07/Bai7.7_Ping/) | [✔](java/Chuong07/Bai7.7_Ping/) | [✔](python/Chuong07/Bai7.7_Ping/) | [✔](rust/Chuong07/Bai7.7_Ping/) | [✔](go/Chuong07/Bai7.7_Ping/) |
| `Bai7.8_MQTT` | Thiết bị cảm biến và bảng theo dõi dùng MQTT | 7.9 | 7.5.5 | [✔](csharp/Chuong07/Bai7.8_MQTT/) | [✔](java/Chuong07/Bai7.8_MQTT/) | [✔](python/Chuong07/Bai7.8_MQTT/) | [✔](rust/Chuong07/Bai7.8_MQTT/) | [✔](go/Chuong07/Bai7.8_MQTT/) |

### Chương 8. Lập trình mạng cho ứng dụng AI và an toàn bảo mật

| Bài | Nội dung | Mã giả | Mục | C# | Java | Python | Rust | Go |
|---|---|---|---|:-:|:-:|:-:|:-:|:-:|
| `Bai8.1_GoiLLM` | Gọi API LLM: truyền luồng SSE, giới hạn tần suất, thử lại | 8.1, 8.2 | 8.2 | [✔](csharp/Chuong08/Bai8.1_GoiLLM/) | [✔](java/Chuong08/Bai8.1_GoiLLM/) | [✔](python/Chuong08/Bai8.1_GoiLLM/) | [✔](rust/Chuong08/Bai8.1_GoiLLM/) | [✔](go/Chuong08/Bai8.1_GoiLLM/) |
| `Bai8.2_ThungToken` | Thùng token giới hạn tần suất | 8.3 | 8.2.4 | [✔](csharp/Chuong08/Bai8.2_ThungToken/) | [✔](java/Chuong08/Bai8.2_ThungToken/) | [✔](python/Chuong08/Bai8.2_ThungToken/) | [✔](rust/Chuong08/Bai8.2_ThungToken/) | [✔](go/Chuong08/Bai8.2_ThungToken/) |
| `Bai8.3_McpServer` | MCP server công cụ mạng qua kênh stdio | 8.4, 8.5, 8.7 | 8.3.5 | [✔](csharp/Chuong08/Bai8.3_McpServer/) | [✔](java/Chuong08/Bai8.3_McpServer/) | [✔](python/Chuong08/Bai8.3_McpServer/) | [✔](rust/Chuong08/Bai8.3_McpServer/) | [✔](go/Chuong08/Bai8.3_McpServer/) |
| `Bai8.4_TlsChat` | Bổ sung TLS và xác thực token cho hệ thống chat | 8.6 | 8.4.5 | [✔](csharp/Chuong08/Bai8.4_TlsChat/) | [✔](java/Chuong08/Bai8.4_TlsChat/) | [✔](python/Chuong08/Bai8.4_TlsChat/) | [✔](rust/Chuong08/Bai8.4_TlsChat/) | [✔](go/Chuong08/Bai8.4_TlsChat/) |

<!-- DANH_MUC_KET_THUC -->

### Bài mở rộng (không có trong giáo trình)
Các bài dưới đây bổ sung chủ đề thực tiễn, gợi ý từ các tài liệu tham khảo mới; nhận xét chi tiết về tài liệu và lý do chọn bài nằm trong [docs/BaiMoRong.md](docs/BaiMoRong.md).

<!-- MO_RONG -->
| Bài | Nội dung | Chương liên quan | Ngôn ngữ |
|---|---|:-:|---|
| `BaiMR1.1_QuanLyDiaChiIP` | Quản lý địa chỉ IP (IPAM): chia mạng con VLSM, khối còn trống, kiểm tra chồng lấn, cấp phát | 1 | [Python](python/Chuong01/BaiMR1.1_QuanLyDiaChiIP/), [Go](go/Chuong01/BaiMR1.1_QuanLyDiaChiIP/) |
| `BaiMR1.2_MoPhongGiaoThuc` | Mô phỏng sự kiện rời rạc các giao thức và kiểu truyền: TCP (bắt tay, cwnd, gửi lại nhanh, RTO, FIN), UDP, unicast, broadcast, multicast/IGMP, anycast; hoạt hình HTML | 1 | [Python](python/Chuong01/BaiMR1.2_MoPhongGiaoThuc/), [Go](go/Chuong01/BaiMR1.2_MoPhongGiaoThuc/) |
| `BaiMR3.1_ChuyenTiepCong` | Bộ chuyển tiếp cổng TCP có chuyển tiếp đóng một chiều (half-close) | 3 | [Python](python/Chuong03/BaiMR3.1_ChuyenTiepCong/), [Go](go/Chuong03/BaiMR3.1_ChuyenTiepCong/) |
| `BaiMR3.2_TuDongHoaCLI` | Tự động hóa cấu hình thiết bị qua CLI: đọc tới dấu nhắc, sinh lệnh khác biệt, lũy đẳng, chạy thử | 3 | [Python](python/Chuong03/BaiMR3.2_TuDongHoaCLI/), [Go](go/Chuong03/BaiMR3.2_TuDongHoaCLI/) |
| `BaiMR3.3_GameDoanSo` | Trò chơi đoán số nhiều người chơi qua TCP: giao thức yêu cầu/phản hồi khung JSON, máy trạng thái, kiểm tra đầu vào, hết giờ, mất kết nối | 3 | [Python](python/Chuong03/BaiMR3.3_GameDoanSo/), [Go](go/Chuong03/BaiMR3.3_GameDoanSo/) |
| `BaiMR4.1_ThuThapNetFlow` | Thu thập và giải mã NetFlow v5 trên UDP, phát hiện mất gói, thống kê lưu lượng | 4 | [Python](python/Chuong04/BaiMR4.1_ThuThapNetFlow/), [Go](go/Chuong04/BaiMR4.1_ThuThapNetFlow/) |
| `BaiMR4.2_GameDuaXeUDP` | Đua xe thời gian thực qua UDP: vòng tick 20 Hz có thẩm quyền, đầu vào có số thứ tự và dư thừa, ảnh chụp mới nhất thắng, dự đoán phía client, giả lập mất gói 0 % và 20 % | 4 | [Python](python/Chuong04/BaiMR4.2_GameDuaXeUDP/), [Go](go/Chuong04/BaiMR4.2_GameDuaXeUDP/) |
| `BaiMR5.1_TruyenNhieuTep` | Truyền nhiều tệp đồng thời: start/pause/resume/stop, giới hạn tốc độ (thùng token) theo phiên và theo tệp, tải tiếp bằng offset + SHA-256 | 5 | [Python](python/Chuong05/BaiMR5.1_TruyenNhieuTep/), [Go](go/Chuong05/BaiMR5.1_TruyenNhieuTep/) |
| `BaiMR5.2_MoPhongDongThoi` | So sánh mô hình đồng thời trên cùng khối việc (20 yêu cầu mạng 200 ms, việc CPU, mạng + CPU): tuần tự, luồng/goroutine, nhóm luồng/worker, asyncio/kênh, nhóm tiến trình; biểu đồ Gantt văn bản; GIL; tranh chấp dữ liệu, bế tắc | 5 | [Python](python/Chuong05/BaiMR5.2_MoPhongDongThoi/), [Go](go/Chuong05/BaiMR5.2_MoPhongDongThoi/) |
| `BaiMR5.3_GiaoDienDaTacVu` | Giao diện luôn phản hồi khi nhiều tác vụ mạng chạy song song: tải HTTP có tiến trình, kiểm tra cổng, tra cứu tên UDP, hủy từng tác vụ/hủy tất cả, giới hạn đồng thời đổi được; nút “làm sai” chặn luồng giao diện và đo khoảng nghỉ giữa hai nhịp đồng hồ | 5 | [Python](python/Chuong05/BaiMR5.3_GiaoDienDaTacVu/), [Go](go/Chuong05/BaiMR5.3_GiaoDienDaTacVu/) |
| `BaiMR6.1_TatServerAnToan` | Tắt server an toàn khi nhận SIGTERM/SIGINT: ngừng nhận, làm nốt, hạn chờ, hủy | 6 | [Python](python/Chuong06/BaiMR6.1_TatServerAnToan/), [Go](go/Chuong06/BaiMR6.1_TatServerAnToan/) |
| `BaiMR6.2_HoiThoaiAmThanh` | Phòng hội thoại âm thanh nhiều client: RTP/UDP, điều khiển JSON/TCP, trộn mix-minus, bộ đệm jitter, dBFS, tắt mic/độ lợi/âm lượng, ghi WAV và phân tích Goertzel | 6 | [Python](python/Chuong06/BaiMR6.2_HoiThoaiAmThanh/), [Go](go/Chuong06/BaiMR6.2_HoiThoaiAmThanh/) |
| `BaiMR6.3_HoiThoaiVideo` | Phòng hội thoại video nhiều client: khung nén zlib chia mảnh ≤ 1200 byte trên UDP, server ghép lưới (MCU), tắt/bật camera, giới hạn fps/độ phân giải gửi và nhận, chụp lưới PNG | 6 | [Python](python/Chuong06/BaiMR6.3_HoiThoaiVideo/), [Go](go/Chuong06/BaiMR6.3_HoiThoaiVideo/) |
| `BaiMR6.4_GameCaro` | Caro 15×15 nhiều phòng qua TCP: sảnh, tạo/vào/xem phòng, server làm trọng tài, phát nước đi tới người xem, hết giờ mỗi nước, chơi lại; khám phá server bằng UDP broadcast | 6 | [Python](python/Chuong06/BaiMR6.4_GameCaro/), [Go](go/Chuong06/BaiMR6.4_GameCaro/) |
| `BaiMR7.1_TaiTiepHTTP` | Tải tệp HTTP có tải tiếp: Range, If-Range, 206/416, thử lại có lùi thời gian, kiểm tra SHA-256 | 7 | [Python](python/Chuong07/BaiMR7.1_TaiTiepHTTP/), [Go](go/Chuong07/BaiMR7.1_TaiTiepHTTP/) |
| `BaiMR8.1_TacTuChanDoan` | Tác tử AI chẩn đoán mạng bằng gọi công cụ, có lớp kiểm soát (danh mục, lược đồ, chính sách đích, giới hạn vòng) | 8 | [Python](python/Chuong08/BaiMR8.1_TacTuChanDoan/), [Go](go/Chuong08/BaiMR8.1_TacTuChanDoan/) |
| `BaiMR8.2_KiemDuyetCauHinhAI` | Kiểm duyệt cấu hình do LLM sinh: JSON, lược đồ, ngữ nghĩa, chính sách, vòng phản hồi | 8 | [Python](python/Chuong08/BaiMR8.2_KiemDuyetCauHinhAI/), [Go](go/Chuong08/BaiMR8.2_KiemDuyetCauHinhAI/) |
| `BaiMR8.3_ChongSlowloris` | Phòng thủ tấn công từ chối dịch vụ mức ứng dụng kiểu Slowloris: hạn đọc tiêu đề, hạn rỗi, cỡ tiêu đề tối đa, giới hạn kết nối theo IP, trần toàn cục (503) | 8 | [Python](python/Chuong08/BaiMR8.3_ChongSlowloris/), [Go](go/Chuong08/BaiMR8.3_ChongSlowloris/) |
| `BaiMR8.4_ChongSSRF_MCP` | Chống SSRF trong công cụ MCP fetch_url: allowlist giao thức, chặn dải nội bộ/loopback/link-local/metadata, phân giải một lần + nối đúng IP (chống DNS rebinding), kiểm lại mọi chặng chuyển hướng, giới hạn kích thước/thời gian/số hop, gỡ thông tin đăng nhập | 8 | [Python](python/Chuong08/BaiMR8.4_ChongSSRF_MCP/), [Go](go/Chuong08/BaiMR8.4_ChongSSRF_MCP/) |
<!-- /MO_RONG -->

## Quy ước
- **Địa chỉ và cổng.** Đọc theo thứ tự ưu tiên: tham số dòng lệnh > biến môi trường `APP_PORT` > giá trị mặc định của giáo trình (thường là `9000`) – Mã giả 2.1.
- **Mã hóa ký tự.** Mọi dữ liệu văn bản truyền qua mạng dùng UTF-8; thông điệp có cấu trúc dùng JSON với khung tiền tố độ dài 4 byte big-endian (Mã giả 3.6) trừ khi giao thức quy định khác.
- **Xử lý lỗi.** Lỗi của một kết nối không làm dừng server; thời gian chờ được đặt cho mọi thao tác có thể bị chặn.
- **Đặt tên.** `ChuongNN/BaiN.k_TenKhongDau`; C#: một dự án mỗi bài, vai trò chọn bằng tham số (`server`, `client`…); Java: gói mặc định, một lớp `main` cho mỗi vai trò; Python: một tệp cho mỗi vai trò; Rust, Go: một chương trình mỗi bài, vai trò chọn bằng tham số như C#.

## Kiểm thử
- Môi trường kiểm thử: Ubuntu 24.04 (Linux x86-64), .NET SDK 10.0.114, OpenJDK 21.0.12, Python 3.13.18 (Python 3.12 với Tk 8.6 cho giao diện), Rust 1.97.0, Go 1.24.7, mosquitto 2.0.18; tháng 11/2026.
- Mỗi phiên bản ✔ được biên dịch và chạy theo `demo.json`; ảnh kết quả phản ánh đúng đầu ra của lần chạy đó.
- Quy trình GitHub Actions (`.github/workflows/build.yml`) biên dịch lại toàn bộ C#, Java, Rust, Go và kiểm tra cú pháp Python sau mỗi lần đẩy mã, gồm cả dự án Windows Forms (trên máy Windows) và dự án gRPC (có NuGet, Maven).
- Một số kết quả phụ thuộc môi trường mạng (phân giải tên miền công cộng, cổng bị lọc, ICMP); README của bài ghi rõ khi kết quả trên máy khác có thể khác.

## An toàn và sử dụng có trách nhiệm
Các chương trình kiểm tra cổng, quét cổng, ping, kiểm thử tải và thử nghiệm tấn công chèn chỉ dẫn (prompt injection) chỉ được chạy trên máy và mạng thuộc quyền quản lý của người dùng hoặc trong phòng thí nghiệm cô lập. Chứng chỉ, khóa và mật khẩu trong các bài TLS/SMTP chỉ dùng cho thử nghiệm; không dùng lại trong hệ thống thật. Bài gọi API mô hình ngôn ngữ lớn mặc định dùng server giả lập cục bộ; khi dùng dịch vụ thật, khóa API phải đặt trong biến môi trường, không ghi vào mã nguồn.

## Tài liệu tham khảo
Danh mục trình bày theo chuẩn IEEE. Giáo trình đi kèm kho mã có danh mục tài liệu tham khảo đầy đủ; dưới đây là các tài liệu được dùng trực tiếp khi xây dựng kho mã và các bài mở rộng.

**Giáo trình đi kèm**

[1] Bùi Dương Thế và nhóm tác giả, *Giáo trình Lập trình mạng*. Thành phố Hồ Chí Minh: Trường Đại học Giao thông vận tải Thành phố Hồ Chí Minh, 2026.

**Sách**

[2] J. F. Kurose and K. W. Ross, *Computer Networking: A Top-Down Approach*, 9th ed. Hoboken, NJ, USA: Pearson, 2026.

[3] I. Grigorik, *High Performance Browser Networking*. Sebastopol, CA, USA: O’Reilly Media, 2013.

[4] E. R. Harold, *Java Network Programming*, 4th ed. Sebastopol, CA, USA: O’Reilly Media, 2014.

[5] R. Blum, *C# Network Programming*. Alameda, CA, USA: Sybex, 2003.

[6] F. Reid, *Network Programming in .NET with C# and Visual Basic .NET*. Burlington, MA, USA: Elsevier Digital Press, 2004.

[7] C. Nagel, A. Mungale, V. Kumar, N. Laghari, A. Krowczyk, T. Parker, S. Sivakumar, and A. Serban, *Pro .NET 1.1 Network Programming*, 2nd ed. Berkeley, CA, USA: Apress, 2004.

[8] S. Burns, *Hands-On Network Programming with C# and .NET Core*. Birmingham, U.K.: Packt Publishing, 2019.

[9] J. Sharp, *Microsoft Visual C# Step by Step*, 9th ed. Hoboken, NJ, USA: Pearson Education (Microsoft Press), 2018.

[10] M. O. F. Sarker, *Python Network Programming Cookbook*. Birmingham, U.K.: Packt Publishing, 2014.

[11] E. Chou, M. Kennedy, and M. Whaley, *Mastering Python Networking*, 4th ed. Birmingham, U.K.: Packt Publishing, 2023.

[12] C. Töpke, *Network Programming and Automation Essentials: Get started in the realm of network automation using Python and Go*. Birmingham, U.K.: Packt Publishing, 2023.

[13] E. Chou, *AI Networking Cookbook: Practical recipes for AI-assisted network automation and development*. Birmingham, U.K.: Packt Publishing, 2025.

[14] M. Butcher and M. Farina, *Go in Practice*. Shelter Island, NY, USA: Manning Publications, 2016.

**Đặc tả giao thức và tiêu chuẩn**

[15] J. Postel, “User Datagram Protocol,” Internet Engineering Task Force (IETF), RFC 768, Aug. 1980. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc768

[16] J. Postel, “Internet Control Message Protocol,” Internet Engineering Task Force (IETF), RFC 792, Sep. 1981. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc792

[17] J. Mogul, “Broadcasting Internet datagrams,” Internet Engineering Task Force (IETF), RFC 919, Oct. 1984. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc919

[18] P. Mockapetris, “Domain names – Implementation and specification,” Internet Engineering Task Force (IETF), RFC 1035, Nov. 1987. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc1035

[19] S. Deering, “Host extensions for IP multicasting,” Internet Engineering Task Force (IETF), RFC 1112, Aug. 1989. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc1112

[20] B. Cain, S. Deering, I. Kouvelas, B. Fenner, and A. Thyagarajan, “Internet Group Management Protocol, Version 3,” Internet Engineering Task Force (IETF), RFC 3376, Oct. 2002. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc3376

[21] S. Deering and R. Hinden, “Internet Protocol, Version 6 (IPv6) specification,” Internet Engineering Task Force (IETF), RFC 8200, Jul. 2017. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc8200

[22] W. Eddy, Ed., “Transmission Control Protocol (TCP),” Internet Engineering Task Force (IETF), RFC 9293, Aug. 2022. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc9293

[23] V. Paxson, M. Allman, J. Chu, and M. Sargent, “Computing TCP’s retransmission timer,” Internet Engineering Task Force (IETF), RFC 6298, Jun. 2011. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc6298

[24] L. Eggert, G. Fairhurst, and G. Shepherd, “UDP usage guidelines,” Internet Engineering Task Force (IETF), RFC 8085, Mar. 2017. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc8085

[25] R. Fielding, M. Nottingham, and J. Reschke, Eds., “HTTP semantics,” Internet Engineering Task Force (IETF), RFC 9110, Jun. 2022. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc9110

[26] R. Fielding, M. Nottingham, and J. Reschke, Eds., “HTTP/1.1,” Internet Engineering Task Force (IETF), RFC 9112, Jun. 2022. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc9112

[27] M. Thomson and C. Benfield, Eds., “HTTP/2,” Internet Engineering Task Force (IETF), RFC 9113, Jun. 2022. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc9113

[28] M. Bishop, Ed., “HTTP/3,” Internet Engineering Task Force (IETF), RFC 9114, Jun. 2022. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc9114

[29] J. Iyengar and M. Thomson, Eds., “QUIC: A UDP-based multiplexed and secure transport,” Internet Engineering Task Force (IETF), RFC 9000, May 2021. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc9000

[30] I. Fette and A. Melnikov, “The WebSocket protocol,” Internet Engineering Task Force (IETF), RFC 6455, Dec. 2011. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc6455

[31] J. Klensin, “Simple Mail Transfer Protocol,” Internet Engineering Task Force (IETF), RFC 5321, Oct. 2008. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc5321

[32] E. Rescorla, “The Transport Layer Security (TLS) protocol version 1.3,” Internet Engineering Task Force (IETF), RFC 8446, Aug. 2018. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc8446

[33] D. Cooper et al., “Internet X.509 public key infrastructure certificate and certificate revocation list (CRL) profile,” Internet Engineering Task Force (IETF), RFC 5280, May 2008. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc5280

[34] D. Hardt, Ed., “The OAuth 2.0 authorization framework,” Internet Engineering Task Force (IETF), RFC 6749, Oct. 2012. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc6749

[35] M. Jones, J. Bradley, and N. Sakimura, “JSON Web Token (JWT),” Internet Engineering Task Force (IETF), RFC 7519, May 2015. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc7519

[36] M. Nottingham, E. Wilde, and S. Dalal, “Problem details for HTTP APIs,” Internet Engineering Task Force (IETF), RFC 9457, Jul. 2023. [Trực tuyến]. Địa chỉ: https://www.rfc-editor.org/rfc/rfc9457

[37] OASIS, “MQTT Version 5.0,” OASIS Standard, Mar. 2019. [Trực tuyến]. Địa chỉ: https://docs.oasis-open.org/mqtt/mqtt/v5.0/mqtt-v5.0.html (truy cập 01/11/2026).

[38] JSON-RPC Working Group, “JSON-RPC 2.0 specification,” 2013. [Trực tuyến]. Địa chỉ: https://www.jsonrpc.org/specification (truy cập 01/11/2026).

[39] Model Context Protocol, “Specification, revision 2026-07-28,” Jul. 2026. [Trực tuyến]. Địa chỉ: https://modelcontextprotocol.io/specification/2026-07-28 (truy cập 01/11/2026).

[40] The gRPC Authors, “gRPC documentation,” [Trực tuyến]. Địa chỉ: https://grpc.io/docs/ (truy cập 01/11/2026).

**Tài liệu chính thức của nền tảng và thư viện**

[41] Microsoft, “.NET documentation – networking, asynchronous programming, desktop and gRPC topics, and .NET API reference,” Microsoft Learn. [Trực tuyến]. Địa chỉ: https://learn.microsoft.com/dotnet/ (truy cập 03/11/2026).

[42] Microsoft, “Windows Sockets 2 (Winsock) documentation,” Microsoft Learn, Win32 apps. [Trực tuyến]. Địa chỉ: https://learn.microsoft.com/windows/win32/winsock/ (truy cập 03/11/2026).

[43] Oracle, “Java SE API documentation (java.net, java.nio.channels, javax.net.ssl, javax.swing) and Java Platform, Standard Edition Core Libraries (Virtual Threads),” [Trực tuyến]. Địa chỉ: https://docs.oracle.com/en/java/javase/ (truy cập 03/11/2026).

[44] Python Software Foundation, “The Python Standard Library: socket, selectors, asyncio, ssl, errno, signal,” Python 3 documentation. [Trực tuyến]. Địa chỉ: https://docs.python.org/3/library/ (truy cập 03/11/2026).

[45] The Linux man-pages project, “Linux manual pages: socket(2), socket(7), send(2), recv(2), select(2), udp(7),” Linux man-pages 6.19. [Trực tuyến]. Địa chỉ: https://man7.org/linux/man-pages/ (truy cập 03/11/2026).

[46] The Rust Project Developers, “The Rust Standard Library (std::net, std::io, std::thread)” và S. Klabnik, C. Nichols, *The Rust Programming Language*. [Trực tuyến]. Địa chỉ: https://doc.rust-lang.org/std/ , https://doc.rust-lang.org/book/

[47] Tokio Contributors, “Tokio – an asynchronous Rust runtime,” documentation. [Trực tuyến]. Địa chỉ: https://tokio.rs/

[48] The Go Authors, “The Go Programming Language Specification” và “Go standard library (net, net/http, crypto/tls, context)”. [Trực tuyến]. Địa chỉ: https://go.dev/ref/spec , https://pkg.go.dev/std


## Trích dẫn
Khi sử dụng kho mã trong giảng dạy hoặc nghiên cứu, đề nghị trích dẫn giáo trình và kho mã (xem `CITATION.cff`):
> Bùi Dương Thế và nhóm tác giả, *NetworkProgramming: Mã nguồn minh họa Giáo trình Lập trình mạng* (C#, Java, Python, Rust, Go), phiên bản 1.1.0, 2026. [Trực tuyến]. Địa chỉ: https://github.com/buiduongthe/NetworkProgramming

## Lời cảm ơn
Nhóm tác giả trân trọng cảm ơn:
- **Tác giả của các sách tham khảo** – J. F. Kurose, K. W. Ross, I. Grigorik, E. R. Harold, R. Blum, F. Reid, C. Nagel và các đồng tác giả, S. Burns, J. Sharp, M. O. F. Sarker, E. Chou, M. Kennedy, M. Whaley, C. Töpke, M. Butcher, M. Farina – mà tác phẩm đã định hướng nội dung và gợi ý nhiều chủ đề cho các bài mở rộng. Mã nguồn trong kho được viết mới; các sách chỉ được dẫn ở mục *Đọc thêm* của từng bài.
- **Cộng đồng phát triển ngôn ngữ và nền tảng** – Microsoft và .NET Foundation (C#, .NET), cộng đồng OpenJDK (Java), Python Software Foundation (Python), Rust Project (Rust), nhóm phát triển Go (Go) – cùng các dự án mã nguồn mở được dùng trong kho: Tokio, rustls, serde, gRPC và Protocol Buffers, Eclipse Mosquitto, OpenSSL, aiosmtpd, Playwright.
- **Các tổ chức tiêu chuẩn** – IETF, OASIS, W3C/WHATWG và nhóm phát triển Model Context Protocol – vì các đặc tả mở là nền tảng của mọi bài trong kho.
- **Giảng viên và sinh viên** ngành Công nghệ thông tin và ngành Mạng máy tính và truyền thông dữ liệu, Trường Đại học Giao thông vận tải Thành phố Hồ Chí Minh, đã đóng góp ý kiến, chạy thử và báo lỗi trong quá trình biên soạn giáo trình và kho mã.

## Giấy phép và đóng góp
Mã nguồn phát hành theo giấy phép [MIT](LICENSE). Góp ý, báo lỗi và đề xuất cải tiến xin gửi qua mục *Issues* của kho hoặc email buiduongthe@gmail.com. Khi đóng góp mã mới, đề nghị giữ đúng cấu trúc thư mục, bổ sung `demo.json`, chạy `tools/demo.py` để cập nhật kết quả và cập nhật `docs/_chapters/chNN.json` rồi chạy `tools/gen_docs.py`.
