# Bài 7.2 – Server REST tối giản trên HTTP/1.1 (Go)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 7.2 (Server REST tối giản trên HTTP/1.1) |
| Mục giáo trình | 7.2.3 Thiết kế RESTful API |
| Kế thừa | Mã giả 6.3 (server bất đồng bộ), Mã giả 7.1 (READ_HEADERS), Mã giả 3.7 (DICT nạp từ `glossary.json`) |
| Thư viện | Thư viện chuẩn Go: `net`, `bufio`, `encoding/json`, `net/url` (`PathUnescape`, `PathEscape`), `sync`; không dùng `net/http` |

## Mục tiêu
Cài đặt `HTTP_SESSION`, `ROUTE`, `RESPOND` cho dịch vụ tra cứu thuật ngữ theo thiết kế RESTful của Mục 7.2.3: giữ kết nối cho nhiều yêu cầu, từ chối thân chunked (501) và thân quá lớn (413) trước khi đọc, trả lỗi theo Problem Details (`application/problem+json`), tính `Content-Length` theo byte UTF-8. Bổ sung theo bài tập cuối chương: `POST /api/v1/terms` (201 kèm `Location`, 400, 409) và `DELETE /api/v1/terms/{term}` (204).

## Tệp
- `rest_server.go` – Mã giả 7.2: `httpSession`, `route`, `respond`, `problem`; `readLineAsync`, `readHeadersAsync`.
- `rest_client.go` – client thử, gửi nhiều yêu cầu trên một kết nối keep-alive.
- `util.go` – đọc tham số cổng, xử lý Ctrl+C.
- `main.go` – chọn vai trò `server`, `client`.
- `glossary.json` – từ điển thuật ngữ.
- `curl_demo.sh` – ba lệnh curl minh họa 200, 501, 413 (giống bản Python).
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
```bash
go build -o out/app .
# Cửa sổ 1 – server
out/app server 0.0.0.0 8080
# Cửa sổ 2 – client keep-alive
out/app client 127.0.0.1 8080
# Cửa sổ 3 – curl
sh curl_demo.sh 8080
```
Mỗi dòng nhập ở client có dạng `PHƯƠNG_THỨC ĐƯỜNG_DẪN [THÂN_JSON]`, ví dụ:
```
GET /api/v1/terms/TCP
POST /api/v1/terms {"term":"QUIC","text":"Giao thức vận chuyển trên UDP"}
DELETE /api/v1/terms/QUIC
```
Script `curl_demo.sh [port]` chạy ba lệnh curl của phần kết quả (cần `sh` và `curl`; trên Windows dùng Git Bash hoặc WSL).

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Client gửi chín yêu cầu trên cùng một kết nối (server ghi cùng một cổng nguồn cho cả chín yêu cầu) và nhận đủ các mã 200, 404, 201 kèm `Location`, 204 không có thân, 405; `Keep%20alive` được giải mã thành thuật ngữ có dấu cách. Cửa sổ curl cho thấy yêu cầu có `Transfer-Encoding: chunked` nhận 501, yêu cầu khai báo `Content-Length: 4000000000` nhận 413 ngay mà server không cấp phát bộ đệm, và kết nối bị đóng sau hai mã này.

## Ghi chú
- `bufio.Reader` (bộ đệm 64 KiB) đóng vai `Reader`: `READ_LINE_ASYNC` là `ReadSlice('\n')` rồi kiểm tra giới hạn 8 192 byte; `READ_EXACT_ASYNC` là `io.ReadFull`. Mỗi kết nối chạy trong một goroutine (Mã giả 6.3), lời gọi đọc trong goroutine tương ứng `AWAIT`.
- `MAX_BODY` không có giá trị trong giáo trình; bài dùng 1 048 576 byte (bằng `MAX_FRAME` của Mã giả 3.6).
- Theo mã giả: `Content-Length` đọc bằng số nguyên 64 bit (`strconv.ParseInt(…, 64)`, giá trị > 4 GB vẫn đọc đúng), giá trị âm hoặc vượt `MAX_BODY` nhận 413; `READ_HEADERS` báo lỗi khi kết nối đóng giữa phần tiêu đề; 204 không có thân, không có `Content-Type`, `Content-Length`.
- Bổ sung so với mã giả: dòng yêu cầu không đủ ba phần nhận 400 rồi đóng kết nối; `Content-Length` không phải số nguyên kết thúc phiên; `PATH(target)` bỏ phần truy vấn sau `?` (nên `?page=1&size=50` được chấp nhận nhưng chưa phân trang); POST yêu cầu `term`, `text` là chuỗi.
- Như mã giả, server coi mọi kết nối là được giữ lại (không xét HTTP/1.0) và không áp dụng thời gian chờ nhàn rỗi.
- Khác Python (một luồng sự kiện), các goroutine phiên của Go chạy song song thật, nên `DICT` được bảo vệ bằng `sync.Mutex`. Map của Go không giữ thứ tự, nên bài giữ thêm danh sách `KEYS` theo thứ tự trong `glossary.json` (đọc bằng `json.Decoder.Token`) để `GET /api/v1/terms` trả danh sách giống các ngôn ngữ khác.
- JSON được mã hóa bằng `json.Encoder` với `SetEscapeHTML(false)`; mặc định `json.Marshal` đổi `<`, `>`, `&` thành `<`… (vẫn đúng JSON nhưng khó đọc).
- Thử thêm bằng curl: `curl -i http://127.0.0.1:8080/api/v1/terms`, `curl -i -X POST -d '{"term":"QUIC","text":"..."}' http://127.0.0.1:8080/api/v1/terms`.
- Kiểm thử: Linux, Go 1.24.7, curl.
