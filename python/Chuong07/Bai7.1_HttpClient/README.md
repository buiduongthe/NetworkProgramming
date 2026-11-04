# Bài 7.1 – Client HTTP/1.1 tối giản trên socket TCP (Python)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 7.1 (Client HTTP/1.1 tối giản trên socket TCP) |
| Mục giáo trình | 7.2.1 HTTP/1.1 |
| Kế thừa | Mã giả 2.2 (SAFE_CONNECT), Mã giả 3.4 (RECV_EXACT), Mã giả 3.5 (READ_LINE, Reader) |
| Thư viện | Thư viện chuẩn: `socket` |

## Mục tiêu
Cài đặt `HTTP_REQUEST`, `READ_HEADERS`, `READ_CHUNKED`: ghép dòng yêu cầu và tiêu đề (có `Host` kèm cổng, `Connection: close`, dòng trống), đọc dòng trạng thái, đọc tiêu đề đến dòng trống và đọc thân theo đúng một trong ba cách định khung của HTTP/1.1: `Transfer-Encoding: chunked`, `Content-Length`, hoặc đọc đến khi server đóng kết nối. Server thử đi kèm trả về cả ba kiểu để kiểm tra từng nhánh của mã giả.

## Tệp
- `http_client.py` – Mã giả 7.1: `http_request`, `read_headers`, `read_chunked`, `Reader`, `read_line`, `read_exact`.
- `server_thu.py` – server thử tuần tự trả về ba kiểu định khung thân.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
Mở hai cửa sổ dòng lệnh trong thư mục bài:
```bash
# Cửa sổ 1 – server thử
python3 server_thu.py 0.0.0.0 8080
# Cửa sổ 2 – client, mỗi đường dẫn là một yêu cầu GET
python3 http_client.py 127.0.0.1 8080 /api/v1/terms/TCP /chunked /dong-ket-noi /khong-co
```
Dừng server bằng Ctrl+C.

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Bốn yêu cầu đi qua ba nhánh định khung. Thân JSON có 58 ký tự nhưng `Content-Length` = 71 byte vì chữ tiếng Việt chiếm 2–3 byte UTF-8; thân chunked gồm ba khối (khối đầu có phần mở rộng `;vi-du=mo-rong`, cuối có trailer `X-So-Khoi`) được ghép lại đủ 55 byte; nhánh không có độ dài đọc đến khi server đóng kết nối; mã 404 vẫn có thân `application/problem+json`.

## Ghi chú
- `READ_EXACT(r, n)` lấy phần dư `pending` của bộ đọc trước rồi mới nhận tiếp từ socket, đúng như chú thích của mã giả; đọc thẳng trên socket sẽ mất phần đầu của thân đã đến cùng tiêu đề.
- Theo dòng 29 của Mã giả 7.1, `READ_HEADERS` báo `ConnectionClosed` khi `READ_LINE` trả NULL (server đóng kết nối khi chưa gửi hết phần tiêu đề), và dừng bình thường khi gặp dòng trống. `READ_LINE` từ chối cả dòng có LF nhưng dài hơn `max_len` (dòng 5 của Mã giả 3.5).
- Client chỉ minh họa cấu trúc giao thức: không hỗ trợ TLS, chuyển hướng, nén, giữ kết nối. Ứng dụng thật dùng thư viện HTTP của nền tảng (Mục 7.2.1).
- Thời gian chờ nhận 5 000 ms (không có trong mã giả) để client không treo nếu server không trả lời.
- Có thể thử với server tĩnh của Python: `python3 -m http.server 8000` rồi gọi client với cổng 8000 và đường dẫn một tệp (phản hồi có `Content-Length`).
- Kiểm thử: Linux, Python 3.13.18.
