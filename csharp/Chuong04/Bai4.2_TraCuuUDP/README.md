# Bài 4.2 – Dịch vụ tra cứu thuật ngữ trên UDP có gửi lại (C#)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 4.3 (Server tra cứu thuật ngữ trên UDP), Mã giả 4.4 (UDP_REQUEST – gửi lại có lùi thời gian, ghép mã định danh) |
| Mục giáo trình | 4.3.1 Dịch vụ tra cứu thuật ngữ trên UDP |
| Kế thừa | Mã giả 4.1, 4.2 (vòng RECVFROM/SENDTO, thời gian chờ); `PROCESS_REQUEST`, `DICT` của server tra cứu TCP (Mục 3.5.1); Mã giả 2.1 |
| Thư viện | .NET BCL: `System.Net.Sockets.Socket`, `System.Text.Json.Nodes` |

## Mục tiêu
Chuyển dịch vụ tra cứu của Chương 3 sang UDP: mỗi yêu cầu, mỗi phản hồi là một datagram JSON, không cần trường độ dài; yêu cầu mang `id` để ghép phản hồi; client gửi lại tối đa 4 lần với thời gian chờ 500 → 1000 → 2000 → 4000 ms. Server có tham số mô phỏng mất gói (bỏ qua N datagram đầu tiên) để quan sát cơ chế gửi lại.

## Tệp
- `Program.cs` – chọn vai trò `server` hoặc `client`.
- `UdpLookupServer.cs` – Mã giả 4.3 và `ProcessRequest`.
- `UdpLookupClient.cs` – Mã giả 4.4 `UdpRequest` và vòng đọc thuật ngữ.
- `NetUtil.cs` – `Endpoint.Parse` (Mã giả 2.1), mã hóa/giải mã JSON UTF-8.
- `glossary.json` – từ điển thuật ngữ.
- `Bai4.2_TraCuuUDP.csproj` – dự án .NET 10.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
Trong thư mục bài (cần .NET SDK 10), lệnh giống nhau trên Windows và Linux:
```bash
# Cửa sổ 1 – server, bỏ qua 2 datagram đầu tiên
dotnet run -- server 0.0.0.0 9100 2
# Cửa sổ 2 – client
dotnet run -- client 127.0.0.1 9100
```
Chạy trong thư mục bài (nơi có `glossary.json`). Nhập tên thuật ngữ ở cửa sổ client (ví dụ `UDP`, `QUIC`, `XYZ`); tham số thứ ba của server là số datagram đầu tiên bị bỏ qua, bỏ trống hoặc 0 để không mô phỏng mất gói.

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Server bỏ qua hai datagram đầu tiên nên yêu cầu `UDP` (id=1) chỉ thành công ở lần gửi thứ ba, sau hai lần hết thời gian chờ 500 ms và 1000 ms (tổng khoảng 1,5 giây); cả ba lần gửi mang cùng `id`. Các yêu cầu sau thành công ngay lần đầu; `XYZ` nhận mã 404. Cửa sổ 3 gửi tới cổng không có server: sau 4 lần gửi (chờ tổng cộng 0,5 + 1 + 2 + 4 = 7,5 giây), hàm báo lỗi "Không có phản hồi sau 4 lần".

## Ghi chú
- Tham số thứ ba của server (số datagram bị bỏ qua) được thêm để mô phỏng mất gói; mã giả không có tham số này. Bài tập 4.31 yêu cầu mô phỏng mất ngẫu nhiên 30%, có thể sửa điều kiện bỏ qua thành phép thử ngẫu nhiên.
- Các dòng nhật ký `[client]   Lần k: ...`, `Hết ... ms không có phản hồi` trong hàm gửi yêu cầu là nhật ký minh họa, không có trong Mã giả 4.4.
- Ngoại lệ `JsonError` của mã giả gồm cả lỗi giải mã UTF-8 và trường hợp JSON hợp lệ nhưng không phải đối tượng (ví dụ `[1,2]`); server trả `{"status":400,"error":"JSON sai","id":null}`.
- JSON được ghi dạng gọn, giữ nguyên chữ có dấu (không thoát `\uXXXX`) ở cả ba ngôn ngữ nên kích thước datagram giống nhau (ví dụ phản hồi `UDP` dài 176 byte).
- Server bắt lỗi ICMP port unreachable của Windows (Mục 4.2.2) và tiếp tục vòng nhận. Client xử lý lỗi này (`ConnectionReset`) như hết thời gian chờ theo dòng 12–13 của Mã giả 4.4: `BREAK` sang lần thử kế tiếp với thời gian chờ gấp đôi.
- Chưa có phân trang cho thao tác `list`; với từ điển lớn, phản hồi vượt `MAX_DGRAM` = 1200 byte sẽ được thay bằng mã 413 như mã giả.
- Dùng `System.Text.Json.Nodes.JsonObject`; bộ mã hóa `JavaScriptEncoder.UnsafeRelaxedJsonEscaping` để không thoát chữ có dấu. `ReceiveTimeout` có độ phân giải mili giây nên tổng thời gian chờ đo được có thể dài hơn 7,5 giây vài trăm mili giây.
- Kiểm thử: Linux, .NET SDK 10.0.114.
