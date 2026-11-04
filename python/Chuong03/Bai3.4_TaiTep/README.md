# Bài 3.4 – Tải tệp lên server có kiểm tra toàn vẹn (Python)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 3.9 (Server nhận tệp có kiểm tra mã băm), Mã giả 3.10 (Client tải tệp lên server) |
| Mục giáo trình | 3.5.2 Truyền tệp có kiểm tra toàn vẹn, 3.5.3 (ca kiểm thử 8, 9) |
| Kế thừa | Mã giả 3.7 (HANDLE_LOOKUP, thêm nhánh `IF req.op = "put" THEN HANDLE_PUT(conn, req); CONTINUE`), Mã giả 3.8 (client tra cứu), Mã giả 3.6 (khung JSON), Mã giả 3.3 (SEND_ALL), Mã giả 2.1, 2.2 |
| Thư viện | Thư viện chuẩn: `socket`, `struct`, `json`, `hashlib`, `os` |

## Mục tiêu
Mở rộng dịch vụ tra cứu để client tải tệp lên server trong cùng một phiên, kết hợp hai cách định khung: phần điều khiển dùng khung JSON, nội dung tệp là dòng byte thô có độ dài khai báo trước trong tiêu đề. Trình tự bốn bước:

1. Client gửi `{"op":"put","name":…,"size":…,"sha256":…}` (mã băm SHA-256 dạng hex chữ thường).
2. Server kiểm tra tham số, trả `{"status":100}` (sẵn sàng) hoặc `{"status":400,"error":"Sai tham số"}`.
3. Client gửi đúng `size` byte nội dung, đọc tệp theo từng khối 64 KiB.
4. Server ghi tệp tạm `.part`, tính lại mã băm, trả `{"status":200,"saved":…}` nếu khớp (đổi tên tệp tạm thành tệp chính thức) hoặc `{"status":422,"error":"Mã băm không khớp"}` (xóa tệp tạm).

Ba biện pháp an toàn theo giáo trình: chỉ lấy phần tên tệp (chặn duyệt đường dẫn kiểu `../../etc/passwd`), giới hạn `MAX_FILE` = 100 MiB, ghi vào tệp `.part` và chỉ đổi tên khi mã băm khớp.

## Tệp
- `put_server.py` – server: `process_request` (tra cứu), `basename`, `handle_put` (Mã giả 3.9), `handle_lookup` có nhánh `put`.
- `put_client.py` – client: `put_file` (Mã giả 3.10) và tra cứu trong cùng một kết nối.
- `kiem_thu.py` – client kiểm thử tự tạo tiêu đề và nội dung cho ca 8, 9 và hai ca bổ sung (kích thước vượt giới hạn, ngắt giữa chừng).
- `framing.py`, `netutil.py` – thư viện định khung (Bài 3.2) và tiện ích kết nối (Mã giả 2.1, 2.2), chép nguyên để bài tự chứa.
- `glossary.json` – từ điển thuật ngữ dùng cho thao tác tra cứu (như Bài 3.3).
- `demo.json`, `ket_qua.txt`, `ket_qua.png` – kịch bản và kết quả thực thi thật. Bước chuẩn bị của `demo.json` xóa thư mục `uploads/` cũ và tạo tệp mẫu `mau.txt` (3 000 dòng, 198 000 byte); hai tệp này không lưu trong kho mã.

## Cách chạy
Mở các cửa sổ dòng lệnh trong thư mục bài (server đọc `glossary.json` và ghi tệp vào `uploads/` ở thư mục hiện hành):
```bash
# Cửa sổ 1 – server
python3 put_server.py 0.0.0.0 9100
# Cửa sổ 2 – client: nhập ví dụ  put mau.txt  hoặc  TCP
python3 put_client.py 127.0.0.1 9100
# Cửa sổ 3 – client kiểm thử
python3 kiem_thu.py 127.0.0.1 9100
```
Trên Windows dùng `python` thay cho `python3`.
Ở cửa sổ client, nhập `put <đường dẫn tệp>` để tải tệp lên hoặc nhập một thuật ngữ để tra cứu; kết thúc nhập bằng Ctrl+D (Linux/macOS) hoặc Ctrl+Z rồi Enter (Windows). Tệp nhận được nằm trong thư mục `uploads/` của thư mục chạy server; so sánh mã băm bằng `sha256sum` (Linux) hoặc `Get-FileHash` (Windows PowerShell). Dừng server bằng Ctrl+C.

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Tệp `mau.txt` (198 000 byte, gửi thành bốn khối) được lưu với mã băm trùng tệp gốc (cửa sổ 4). Tên `../thu.txt` và `..\..\thu2.txt` chỉ được lưu thành `thu.txt`, `thu2.txt` trong `uploads/` (ca 8); tên `../` không còn phần tên tệp nên bị từ chối với 400; mã băm khai báo sai nhận 422 (ca 9); kích thước 200 MiB bị từ chối; client ngắt sau 100/1000 byte được ghi nhật ký "Tệp chưa nhận đủ" và thư mục `uploads/` không còn tệp `.part` nào.

## Ghi chú
- `BASENAME(REPLACE(name, "\\", "/"))` (dòng 3) được tự cài đặt: đổi `\` thành `/` rồi lấy phần sau dấu `/` cuối cùng. Hàm lấy tên tệp của thư viện chuẩn trên Linux không coi `\` là dấu phân cách, nên một tên như `..\..\x` sẽ nguy hiểm khi cùng mã chạy trên Windows.
- Dòng 4–6 từ chối tên `""`, `.`, `..`; `size` phải là số nguyên (không nhận số thực, chuỗi, true/false) trong khoảng 0…`MAX_FILE`; `sha256` phải là chuỗi đúng 64 ký tự thập lục phân (`IS_HEX`). Sai kiểu hoặc thiếu trường đều nhận 400 "Sai tham số".
- Khối `CATCH ANY` (dòng 23–25) xóa tệp `.part` với mọi lỗi trong vòng nhận (kết nối đóng, bị đặt lại, hết thời gian chờ, lỗi ghi đĩa) rồi ném lại lỗi, nên không còn tệp dở dang trong mọi trường hợp.
- Khung JSON hợp lệ nhưng không phải đối tượng nhận 400 "JSON sai" như dòng 24–27 của Mã giả 3.7.
- `RENAME` dùng `os.replace` để ghi đè tệp đích cùng tên (`os.rename` báo lỗi trên Windows khi tệp đích đã tồn tại).
- Thư mục `uploads/` được tạo khi server khởi động (bước này không có trong mã giả).
- So sánh mã băm phân biệt hoa thường như mã giả; client gửi hex chữ thường. Mã băm không khóa chỉ phát hiện sai lệch ngẫu nhiên, chống sửa đổi có chủ đích cần HMAC hoặc TLS (Mục 3.5.2).
- Client nhận lệnh dạng `put <tệp>` (mã giả chỉ định nghĩa thủ tục `PUT_FILE(s, path)`); tệp không tồn tại được báo lỗi và phiên vẫn tiếp tục. Cửa sổ 4 của bản demo dùng `ls` và `sha256sum` của Linux.
- Kiểm thử: Linux, Python 3.13.18.
