# Bài mở rộng MR3.1 – Bộ chuyển tiếp cổng TCP (TCP port forwarder) (Java)

| Thông tin | Nội dung |
|---|---|
| Loại | **Bài mở rộng** – không có trong giáo trình, không có mã giả tương ứng |
| Liên hệ với giáo trình | Chương 3: Bài 3.1 (vòng đời socket, `SHUTDOWN` một chiều), Bài 3.2 (`SEND_ALL`); Chương 6: Bài 6.1 (mỗi kết nối một luồng) |
| Thư viện | JDK 21: `java.net.ServerSocket`, `java.net.Socket` (`shutdownOutput`), `java.lang.Thread` |
| Bản ngôn ngữ khác | [C#](../../../csharp/Chuong03/BaiMR3.1_ChuyenTiepCong/), [Python](../../../python/Chuong03/BaiMR3.1_ChuyenTiepCong/), [Rust](../../../rust/Chuong03/BaiMR3.1_ChuyenTiepCong/), [Go](../../../go/Chuong03/BaiMR3.1_ChuyenTiepCong/) |

## Mục tiêu
Viết một bộ chuyển tiếp: mỗi client nối vào cổng nghe được ghép với một kết nối mới tới máy đích, hai luồng (goroutine) bơm dữ liệu theo hai chiều. Điểm mấu chốt là **đóng một chiều (half-close)**: khi một chiều gặp EOF, bộ chuyển tiếp gọi `SHUTDOWN(WRITE)` trên socket bên kia để chuyển tiếp gói FIN, chứ không đóng hẳn, vì chiều ngược lại có thể vẫn đang truyền. Server đích trong bài chỉ trả lời **sau khi** thấy EOF, nên nếu bộ chuyển tiếp làm sai, client và server sẽ chờ nhau mãi. Đây là nguyên lý của `ssh -L`, `socat`, proxy TCP và cân bằng tải lớp 4.

## Tệp
- `ChuyenTiep.java` – bộ chuyển tiếp: `serve` (một kết nối), `pump` (một chiều).
- `ServerDich.java` – server đích “chữ hoa”: đọc tới EOF rồi trả bản viết hoa.
- `ClientThu.java` – client thử: gửi, `shutdownOutput()`, đọc tới EOF, so khớp.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
Biên dịch một lần trong thư mục bài, sau đó mở ba cửa sổ dòng lệnh:
```bash
javac -encoding UTF-8 -d out *.java
# Cửa sổ 1 – server đích
java -cp out ServerDich 127.0.0.1 9001
# Cửa sổ 2 – bộ chuyển tiếp 9000 -> 9001
java -cp out ChuyenTiep 127.0.0.1 9000 127.0.0.1 9001
# Cửa sổ 3 – client nối vào cổng chuyển tiếp
java -cp out ClientThu 127.0.0.1 9000 "Xin chào"
java -cp out ClientThu 127.0.0.1 9000 "dữ liệu lớn " 20000
```
Dừng server đích và bộ chuyển tiếp bằng Ctrl+C. Kịch bản `demo.json` chạy thêm một bộ chuyển tiếp trỏ tới cổng không có server. Ba chương trình dùng cùng giao thức với bản Python, Go nên chạy chéo được (ví dụ client Python qua bộ chuyển tiếp Java tới server Python – đã thử).

Tái lập kết quả từ thư mục gốc của kho: `python3 tools/demo.py java/Chuong03/BaiMR3.1_ChuyenTiepCong`.

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Hai client đi qua bộ chuyển tiếp nhận đúng bản chữ hoa, kể cả 360 000 byte (20 000 lần chuỗi 18 byte UTF-8); số byte lên bằng số byte xuống. Server đích chỉ thấy địa chỉ của bộ chuyển tiếp (127.0.0.1 với một cổng tạm), không thấy client gốc – vì vậy proxy thật thường thêm thông tin nguồn (ví dụ giao thức PROXY, tiêu đề `X-Forwarded-For`). Với bộ chuyển tiếp trỏ tới cổng không có server, kết nối tới đích bị từ chối nên bộ chuyển tiếp đóng kết nối của client; client nhận 0 byte.

## Ghi chú
- `Socket.shutdownOutput()` là `SHUTDOWN(WRITE)`: gửi FIN nhưng vẫn đọc được chiều ngược lại. `InputStream.read` trả về -1 khi gặp EOF (thay cho `b""` của Python).
- Dòng "Dừng" khi nhấn Ctrl+C được in trong *shutdown hook* (`Runtime.addShutdownHook`), vì Java không có `KeyboardInterrupt`.
- Lần chạy minh họa của bản Java ở tình huống đích không chạy cho client thấy EOF (0 byte, không có RST); bản Python thấy RST – xem giải thích ở ý tiếp theo.
- Khi một bên đóng socket trong lúc bộ đệm nhận vẫn còn dữ liệu chưa đọc, TCP gửi RST thay vì FIN. Ở tình huống đích không chạy, kết quả phụ thuộc thời điểm: nếu dữ liệu của client tới trước lúc đóng, client thấy *Connection reset* (RST); nếu không, client thấy EOF bình thường.
- Bộ chuyển tiếp không có xác thực hay giới hạn: chỉ nghe trên `127.0.0.1` và chỉ dùng trong máy, mạng phòng thí nghiệm.
- Mỗi kết nối dùng hai luồng `pump`; luồng phục vụ `join` cả hai rồi mới đóng hai socket.
- Kiểm thử: Linux, OpenJDK 21.0.12.

## Đọc thêm
- M. O. Faruque Sarker, *Python Network Programming Cookbook*, Packt, 2014, Chương 3 (công thức *Forwarding a local port to a remote host*; sách dùng mô-đun `asyncore` đã bị loại khỏi Python 3.12, bài này viết lại bằng socket và luồng).

Mã nguồn của bài được viết mới cho kho này; sách trên chỉ là tài liệu tham khảo ý tưởng.
