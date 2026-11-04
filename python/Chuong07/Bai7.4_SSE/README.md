# Bài 7.4 – Bộ đọc luồng Server-Sent Events (Python)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 7.5 (Bộ đọc luồng Server-Sent Events) |
| Mục giáo trình | 7.3.3 Server-Sent Events |
| Kế thừa | Mã giả 7.1 (gửi GET, đọc tiêu đề, READ_CHUNKED), Mã giả 3.5 (READ_LINE) |
| Thư viện | Thư viện chuẩn: `socket`, `threading` |

## Mục tiêu
Cài đặt `SSE_READ`: đọc từng dòng của luồng `text/event-stream`, dòng trống kết thúc một sự kiện, bỏ dòng chú thích `:`, tách trường tại dấu hai chấm đầu tiên, bỏ đúng một dấu cách đầu giá trị, nối nhiều dòng `data`, lưu `id` và `retry`. Vì server SSE thường gửi thân bằng chunked, bộ đọc dòng được đặt phía sau lớp giải mã khối (READ_CHUNKED của Mã giả 7.1 viết dạng luồng). Server SSE nhỏ đi kèm phát một luồng mẫu, trong đó một sự kiện bị cắt giữa dòng qua hai khối.

## Tệp
- `sse_client.py` – Mã giả 7.5 `sse_read`, `Reader`, `read_line`, lớp `ChunkedSource` (giải mã khối dạng luồng).
- `sse_server.py` – server SSE nhỏ phát luồng mẫu bằng chunked.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
```bash
# Cửa sổ 1 – server SSE
python3 sse_server.py 0.0.0.0 8080
# Cửa sổ 2 – bộ đọc luồng
python3 sse_client.py 127.0.0.1 8080 /events
```
Có thể xem luồng thô bằng `curl -N http://127.0.0.1:8080/events`.

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Server gửi bảy khối; client nhận bốn sự kiện: hai sự kiện `delta` (cả `data: x` và `data:x` cho cùng giá trị), một sự kiện mặc định `message` có hai dòng `data` được nối bằng ký tự xuống dòng dù bị cắt giữa chữ “thứ” qua hai khối, và sự kiện `done`. Dòng chú thích và dòng `retry` không tạo sự kiện; `id` cuối = 3 và `retry` = 3000 ms được ghi nhận để dùng khi kết nối lại.

## Ghi chú
- Lớp giải mã khối trả về dữ liệu thân thuần cho bộ đọc dòng, nên các dòng độ dài khối (hex) không lẫn vào luồng sự kiện – chi tiết được nêu ngay sau Mã giả 7.5.
- Ngoài mã giả: client kiểm tra mã 200 trước khi đọc luồng; không cài đặt kết nối lại tự động với `Last-Event-ID` (chỉ lưu `id` và `retry`).
- Server phát cùng một luồng mẫu cho mọi client rồi đóng (khối 0); mỗi kết nối một luồng (Mã giả 6.1).
- `Reader` nhận bất kỳ đối tượng nào có phương thức `recv(n)`: socket hoặc `ChunkedSource`.
- Kiểm thử: Linux, Python 3.13.18.
