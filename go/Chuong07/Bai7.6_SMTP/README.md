# Bài 7.6 – Phiên gửi thư SMTP có STARTTLS và xác thực (Go)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 7.7 (Phiên gửi thư SMTP có STARTTLS và xác thực) |
| Mục giáo trình | 7.5.1 Thư điện tử: SMTP và IMAP |
| Kế thừa | Mã giả 2.2 (SAFE_CONNECT), Mã giả 3.5 (READ_LINE); TLS_WRAP (Mục 8.4) |
| Thư viện | Thư viện chuẩn Go: `net`, `bufio`, `crypto/tls`, `crypto/x509`, `encoding/base64`; server thử viết bằng Go (thư viện chuẩn) |

## Mục tiêu
Cài đặt `SMTP_REPLY` (phản hồi nhiều dòng `250-…`/`250 …`), `CMD` (gửi lệnh, so chữ số đầu của mã), `SEND_MAIL` (lời chào 220, EHLO, STARTTLS, nâng cấp TLS có kiểm tra chứng chỉ và tên máy, EHLO lại, AUTH PLAIN, MAIL FROM, RCPT TO, DATA, nội dung kết thúc bằng dòng `.`, QUIT) và `DOT_STUFF`. Thư thử có Subject tiếng Việt mã hóa MIME và một dòng bắt đầu bằng dấu chấm. Server thử nghiệm cục bộ bắt buộc STARTTLS bằng chứng chỉ tự ký và chỉ cho AUTH sau TLS.

## Tệp
- `smtp_client.go` – Mã giả 7.7: `smtpReply`, `cmd`, `sendMail`, `dotStuff`, `buildMessage`, `Reader`.
- `smtp_server_thu.go` – server SMTP thử nghiệm: bắt buộc STARTTLS, AUTH PLAIN sau TLS, bỏ dấu chấm do DOT_STUFF thêm, in thư nhận được (không chuyển thư đi đâu). Đóng vai `aiosmtpd` của bản Python để bài Go tự chứa.
- `main.go` – chọn vai trò `client`, `server`.
- `tao_chung_chi.sh` – tạo chứng chỉ tự ký `cert.pem`/`key.pem` (CN và SAN: `localhost`, `127.0.0.1`) bằng openssl (giống bản Python).
- `chay_server_thu.sh` – tạo chứng chỉ nếu chưa có, biên dịch rồi khởi động server thử.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
Chuẩn bị server thử (cần openssl; trên Windows chạy các lệnh `sh` bằng Git Bash hoặc WSL):
```bash
go build -o out/app .
# Cửa sổ 1 – server thử (tạo cert.pem, key.pem nếu chưa có); cổng 587 cần quyền quản trị nên dùng 2525
sh chay_server_thu.sh 127.0.0.1 2525
```
Tài khoản thử lấy từ biến môi trường `SMTP_USER`, `SMTP_PASS` (mặc định `sinhvien` / `matkhau-thu`); client đọc cùng hai biến này.
```bash
# Cửa sổ 2 – client (host, cổng, tệp CA)
export SMTP_USER=sinhvien SMTP_PASS=matkhau-thu
out/app client localhost 2525 cert.pem
```

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Cửa sổ client in toàn bộ hội thoại `C:`/`S:`: danh sách mở rộng trước TLS có `STARTTLS` nhưng chưa có `AUTH`; sau STARTTLS (TLS 1.3, chứng chỉ được kiểm tra với `cert.pem` và tên `localhost`) danh sách có `AUTH PLAIN`; xác thực 235, DATA 354, thư được nhận 250. Server thử in thư nhận được, trong đó dòng bắt đầu bằng dấu chấm đã được server bỏ dấu chấm thêm vào. Cửa sổ thứ ba chạy với mật khẩu sai: server trả 535 và client báo `SmtpError` thay vì gửi tiếp. Cửa sổ thứ tư dùng kho chứng chỉ của hệ điều hành (tham số `-`): chứng chỉ tự ký không được tin cậy nên client dừng ở bắt tay TLS, server ghi nhận `bad certificate`.

## Ghi chú
- `TLS_WRAP(s, CLIENT, host)` là `tls.Client(s, &tls.Config{ServerName: host, RootCAs: …})` rồi `Handshake()` (kiểm tra chuỗi chứng chỉ và tên máy trong SAN; Go không dùng CN để so tên máy). `TLS 1.3` là tên do `tls.VersionName` trả về.
- Mật khẩu không ghi trong mã nguồn mà lấy từ biến môi trường; dòng AUTH được in dạng `AUTH PLAIN ****`.
- Chứng chỉ tự ký được tin cậy bằng cách nạp `cert.pem` vào `x509.CertPool`, không tắt kiểm tra (không dùng `InsecureSkipVerify`).
- Sau STARTTLS, `Reader` được tạo mới nên mọi byte nhận trước khi bắt tay TLS bị bỏ, tránh lỗi chèn lệnh vào phiên đã mã hóa (server thử cũng làm như vậy).
- `SMTP_REPLY` theo mã giả: kết nối đóng (NULL) hoặc dòng ngắn hơn 3 ký tự → `ProtocolError`; dòng chỉ gồm mã ba chữ số (RFC 5321 cho phép bỏ phần văn bản) là dòng cuối, phần văn bản rỗng.
- Ngoài mã giả: kiểm tra mã lời chào là 220. Thân thư dùng `Content-Transfer-Encoding: 8bit` và server thử có quảng bá 8BITMIME; client đầy đủ nên gửi `BODY=8BITMIME` hoặc mã hóa quoted-printable/base64.
- Mã giả cố định cổng 587; bài nhận cổng qua tham số (mặc định 587) để chạy server thử ở cổng không đặc quyền.
- Thư viện chuẩn Go có gói `net/smtp` (`smtp.Dial`, `StartTLS`, `Auth`, `smtp.PlainAuth`, `SendMail`) thực hiện đúng chuỗi lệnh này; bài tự viết phiên trên socket để thấy từng lệnh và mã phản hồi của Mã giả 7.7. Ứng dụng thật nên dùng `net/smtp` hoặc thư viện SMTP/MIME đầy đủ.
- Server thử khác `aiosmtpd` của bản Python ở lời chào, danh sách mở rộng (`AUTH PLAIN`, không có `SMTPUTF8`) và yêu cầu AUTH trước MAIL FROM; client không phụ thuộc các khác biệt này.
- Kiểm thử: Linux, Go 1.24.7, OpenSSL 3.
